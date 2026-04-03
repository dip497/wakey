//! Voice Mode — Real-time speech conversation with Wakey via Deepgram.
//!
//! Architecture:
//! Mic (cpal 16kHz) → WebSocket to Deepgram STT (raw PCM binary) → Text → LLM → TTS WebSocket → Speaker (cpal)
//!
//! Deepgram protocol:
//! - STT: Send raw PCM16 bytes, receive JSON with transcript
//! - TTS: Send JSON {"type":"Speak","text":"..."}, receive raw PCM16 bytes
//! - Auth: "Token xxx" header (not Bearer)

use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async_with_config, tungstenite::client::IntoClientRequest};
use tracing::{debug, error, info};
use wakey_spine::Spine;
use wakey_types::WakeyEvent;
use wakey_types::config::VoiceConfig;

use crate::llm::LlmProvider;

// ── Constants ──

/// Deepgram STT WebSocket endpoint
const DEEPGRAM_STT_URL: &str = "wss://api.deepgram.com/v1/listen";

/// Deepgram TTS WebSocket endpoint
const DEEPGRAM_TTS_URL: &str = "wss://api.deepgram.com/v1/speak";

/// Audio chunk size (~100ms of 16kHz PCM16 = 3200 bytes)
const AUDIO_CHUNK_BYTES: usize = 3200;

/// System prompt for voice conversations
const VOICE_SYSTEM_PROMPT: &str = r#"You are Wakey, a friendly voice companion that lives on the user's laptop.
You're warm, curious, and a little playful. Keep responses conversational and concise — this is a voice conversation, so avoid long paragraphs.
You can see what the user is doing on their screen, remember past conversations, and help with tasks.
Be yourself. Don't announce that you're an AI. Just be Wakey."#;

// ── Voice Session ──

/// Orchestrates the full voice conversation flow.
pub struct VoiceSession {
    config: VoiceConfig,
    llm_config: wakey_types::config::LlmProviderConfig,
    spine: Spine,
    api_key: String,

    /// Audio input stream (microphone)
    input_stream: Option<Stream>,

    /// Audio output stream (speaker)
    output_stream: Option<Stream>,

    /// Channel for outgoing audio (mic → STT)
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,

    /// Buffer for incoming audio (TTS → speaker)
    audio_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,

    /// Flag to signal session should stop
    running: Arc<AtomicBool>,

    /// Conversation history (last N exchanges for context)
    conversation_history: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl VoiceSession {
    /// Create a new voice session.
    pub fn new(
        config: VoiceConfig,
        llm_config: wakey_types::config::LlmProviderConfig,
        spine: Spine,
    ) -> Result<Self, VoiceError> {
        let api_key = std::env::var(&config.api_key_env)
            .map_err(|_| VoiceError::MissingApiKey(config.api_key_env.clone()))?;

        Ok(Self {
            config,
            llm_config,
            spine,
            api_key,
            input_stream: None,
            output_stream: None,
            audio_tx: None,
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(false)),
            conversation_history: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Start a voice session. Returns when the session ends.
    pub async fn start(&mut self) -> Result<(), VoiceError> {
        if !self.config.enabled {
            return Err(VoiceError::Disabled);
        }

        self.running.store(true, Ordering::SeqCst);
        self.spine.emit(WakeyEvent::VoiceListeningStarted);
        info!("Voice session started (Deepgram)");

        // Create channel for audio chunks
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(100);
        self.audio_tx = Some(audio_tx);

        // Start microphone capture
        self.start_mic_capture()?;

        // Run STT session
        let transcribed_text = self.run_stt_session(audio_rx).await?;

        // Stop mic capture
        self.stop_mic_capture();

        // If we got text, process through LLM and respond
        if !transcribed_text.is_empty() {
            self.spine.emit(WakeyEvent::VoiceListeningStopped);

            // Emit what user said
            self.spine.emit(WakeyEvent::VoiceUserSpeaking {
                text: transcribed_text.clone(),
                is_final: true,
            });

            // Thinking state
            self.spine.emit(WakeyEvent::VoiceWakeyThinking);

            // Get LLM response
            let response = self.get_llm_response(&transcribed_text).await?;

            // Speaking state
            self.spine.emit(WakeyEvent::VoiceWakeySpeaking {
                text: response.clone(),
            });

            // Run TTS and play audio
            self.run_tts_session(&response).await?;

            self.spine.emit(WakeyEvent::VoiceSessionEnded);
        } else {
            self.spine.emit(WakeyEvent::VoiceListeningStopped);
        }

        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Stop the voice session.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.stop_mic_capture();
        self.spine.emit(WakeyEvent::VoiceSessionEnded);
    }

    // ── Microphone Capture ──

    /// Start capturing audio from the microphone.
    fn start_mic_capture(&mut self) -> Result<(), VoiceError> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(VoiceError::NoInputDevice)?;

        let sample_rate = self.config.asr_sample_rate;
        let channels = 1u16; // Mono for STT

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let audio_tx = self.audio_tx.clone().unwrap();
        let running = self.running.clone();

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !running.load(Ordering::SeqCst) {
                        return;
                    }

                    // Log first callback to confirm mic is working
                    use std::sync::atomic::AtomicBool;
                    static LOGGED_FIRST: AtomicBool = AtomicBool::new(false);
                    if !LOGGED_FIRST.swap(true, Ordering::Relaxed) {
                        eprintln!("[VOICE] First mic callback: {} samples", data.len());
                    }

                    // Convert i16 samples to bytes (PCM16 little-endian)
                    let bytes: Vec<u8> = data
                        .iter()
                        .flat_map(|sample| sample.to_le_bytes())
                        .collect();

                    // Send in chunks (try_send is non-blocking, safe from any thread)
                    for chunk in bytes.chunks(AUDIO_CHUNK_BYTES) {
                        match audio_tx.try_send(chunk.to_vec()) {
                            Ok(_) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                // Buffer full, drop audio chunk (acceptable for real-time)
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                break;
                            }
                        }
                    }
                },
                |err| error!("Audio input error: {}", err),
                None,
            )
            .map_err(|e| VoiceError::AudioStream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| VoiceError::AudioStream(e.to_string()))?;

        self.input_stream = Some(stream);
        info!("Microphone capture started ({}Hz mono)", sample_rate);
        Ok(())
    }

    /// Stop microphone capture.
    fn stop_mic_capture(&mut self) {
        self.input_stream = None;
        self.audio_tx = None;
        debug!("Microphone capture stopped");
    }

    // ── Deepgram STT ──

    /// Run STT WebSocket session and return transcribed text.
    async fn run_stt_session(
        &self,
        mut audio_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<String, VoiceError> {
        // Build WebSocket URL with query parameters
        let url = format!(
            "{}?encoding=linear16&sample_rate={}&channels=1&model={}&smart_format=true&endpointing={}&language={}",
            DEEPGRAM_STT_URL,
            self.config.asr_sample_rate,
            self.config.stt_model,
            self.config.endpointing_ms,
            self.config.language
        );

        debug!("Connecting to Deepgram STT: {}", url);

        // Build request with auth header
        let mut request = url
            .into_client_request()
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.api_key).parse().unwrap(),
        );

        let (ws_stream, _) = connect_async_with_config(request, None, false)
            .await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;

        info!("Deepgram STT WebSocket connected");

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        let running_rx = self.running.clone();
        let running_tx = self.running.clone();
        let spine = self.spine.clone();

        // Track accumulated transcription
        let transcription = Arc::new(Mutex::new(String::new()));
        let transcription_clone = transcription.clone();
        let speech_ended = Arc::new(AtomicBool::new(false));
        let speech_ended_clone = speech_ended.clone();

        // Task to receive STT events
        let receive_task = tokio::spawn(async move {
            while running_rx.load(Ordering::SeqCst) {
                match ws_rx.next().await {
                    Some(Ok(WsMessage::Text(text))) => {
                        debug!(
                            "STT raw response: {}",
                            &text.chars().take(200).collect::<String>()
                        );
                        if let Ok(response) = serde_json::from_str::<SttResponse>(&text) {
                            // Extract transcript from response structure
                            if let Some(channel) = &response.channel
                                && let Some(alternatives) = &channel.alternatives
                                && let Some(first) = alternatives.first()
                            {
                                let transcript = &first.transcript;
                                if !transcript.is_empty() {
                                    let is_final = response.is_final.unwrap_or(false);

                                    if is_final {
                                        // Final transcript - add to accumulated text
                                        debug!("STT final: {}", transcript);
                                        let mut t = transcription_clone.lock().unwrap();
                                        if !t.is_empty() {
                                            t.push(' ');
                                        }
                                        t.push_str(transcript);
                                    } else {
                                        // Interim transcript - emit for UI feedback
                                        spine.emit(WakeyEvent::VoiceUserSpeaking {
                                            text: transcript.clone(),
                                            is_final: false,
                                        });
                                    }
                                }
                            }

                            // Check for endpointing (speech ended)
                            // Only end if we actually have transcribed text —
                            // Deepgram sends speech_final on silence too
                            if response.speech_final.unwrap_or(false) {
                                let has_text = !transcription_clone.lock().unwrap().is_empty();
                                if has_text {
                                    debug!("STT speech_final with text — ending");
                                    speech_ended_clone.store(true, Ordering::SeqCst);
                                } else {
                                    debug!("STT speech_final but empty — continuing to listen");
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Binary(data))) => {
                        // Deepgram doesn't send binary on STT, but handle gracefully
                        debug!("STT received unexpected binary: {} bytes", data.len());
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        debug!("STT WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("STT WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        });

        // Task to send audio chunks (raw binary, not base64!)
        let send_task = tokio::spawn(async move {
            while running_tx.load(Ordering::SeqCst) {
                match audio_rx.recv().await {
                    Some(audio_data) => {
                        // Send raw PCM16 bytes directly - NO base64, NO JSON wrapper
                        debug!("Sending {} audio bytes to Deepgram", audio_data.len());
                        if ws_tx
                            .send(WsMessage::Binary(audio_data.into()))
                            .await
                            .is_err()
                        {
                            error!("Failed to send audio to Deepgram WebSocket");
                            break;
                        }
                    }
                    None => break,
                }
            }

            // Close connection when done
            let _ = ws_tx.send(WsMessage::Close(None)).await;
        });

        // Wait for speech to end (endpointing) or timeout
        let timeout = tokio::time::Duration::from_secs(30);
        let start = std::time::Instant::now();

        while self.running.load(Ordering::SeqCst)
            && !speech_ended.load(Ordering::SeqCst)
            && start.elapsed() < timeout
        {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Signal stop and wait for tasks
        receive_task.abort();
        send_task.abort();

        let result = transcription.lock().unwrap().clone();
        info!("STT transcription complete: {}", result);

        Ok(result)
    }

    // ── LLM Response ──

    /// Get LLM response for transcribed text.
    async fn get_llm_response(&self, user_text: &str) -> Result<String, VoiceError> {
        // Build messages with history
        let mut messages = vec![serde_json::json!({
            "role": "system",
            "content": VOICE_SYSTEM_PROMPT
        })];

        // Add conversation history
        {
            let history = self.conversation_history.lock().unwrap();
            for msg in history.iter() {
                messages.push(msg.clone());
            }
        }

        // Add current user message
        messages.push(serde_json::json!({
            "role": "user",
            "content": user_text
        }));

        // Use LLM provider from config (same provider as main decision loop)
        let llm_config = self.llm_config.clone();

        let provider = crate::llm::OpenAiCompatible::new(&llm_config)
            .map_err(|e| VoiceError::Llm(e.to_string()))?;

        // Convert to ChatMessage format
        let chat_messages: Vec<wakey_types::ChatMessage> = messages
            .iter()
            .filter_map(|m| {
                let role = m.get("role")?.as_str()?;
                let content = m.get("content")?.as_str()?;
                Some(wakey_types::ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                })
            })
            .collect();

        let response = provider
            .chat(&chat_messages)
            .await
            .map_err(|e| VoiceError::Llm(e.to_string()))?;

        // Update conversation history
        {
            let mut history = self.conversation_history.lock().unwrap();
            history.push(serde_json::json!({
                "role": "user",
                "content": user_text
            }));
            history.push(serde_json::json!({
                "role": "assistant",
                "content": &response
            }));

            // Keep last 10 messages (5 exchanges)
            while history.len() > 10 {
                history.remove(0);
            }
        }

        debug!("LLM response: {}", response);
        Ok(response)
    }

    // ── Deepgram TTS ──

    /// Run TTS WebSocket session and play audio.
    async fn run_tts_session(&mut self, text: &str) -> Result<(), VoiceError> {
        // Build WebSocket URL with query parameters
        let url = format!(
            "{}?encoding=linear16&sample_rate={}&model={}",
            DEEPGRAM_TTS_URL, self.config.tts_sample_rate, self.config.tts_model
        );

        debug!("Connecting to Deepgram TTS: {}", url);

        // Build request with auth header
        let mut request = url
            .into_client_request()
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.api_key).parse().unwrap(),
        );

        let (ws_stream, _) = connect_async_with_config(request, None, false)
            .await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;

        info!("Deepgram TTS WebSocket connected");

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        // Send the text for synthesis
        let speak_msg = TtsSpeak {
            type_: "Speak".to_string(),
            text: text.to_string(),
        };
        let speak_json = serde_json::to_string(&speak_msg)
            .map_err(|e| VoiceError::Serialization(e.to_string()))?;

        ws_tx
            .send(WsMessage::Text(speak_json.into()))
            .await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;

        // Send flush to signal end of input
        let flush_msg = TtsFlush {
            type_: "Flush".to_string(),
        };
        let flush_json = serde_json::to_string(&flush_msg)
            .map_err(|e| VoiceError::Serialization(e.to_string()))?;

        ws_tx
            .send(WsMessage::Text(flush_json.into()))
            .await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;

        debug!("TTS sent Speak + Flush");

        // Start audio output stream
        self.start_audio_output()?;

        let audio_buffer = self.audio_buffer.clone();
        let running = self.running.clone();

        // Receive audio chunks (raw PCM16 binary)
        while running.load(Ordering::SeqCst) {
            match ws_rx.next().await {
                Some(Ok(WsMessage::Binary(audio_data))) => {
                    // Raw PCM16 audio - queue for playback
                    if !audio_data.is_empty() {
                        let mut buf = audio_buffer.lock().unwrap();
                        buf.push_back(audio_data.to_vec());
                        debug!("TTS received {} bytes", audio_data.len());
                    }
                }
                Some(Ok(WsMessage::Text(text))) => {
                    // JSON metadata (clear, flushed, error)
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) {
                        match meta["type"].as_str() {
                            Some("Clear") => {
                                debug!("TTS Clear signal");
                            }
                            Some("Flushed") => {
                                debug!("TTS Flushed signal - synthesis complete");
                                // Wait for playback to finish then break
                                self.wait_for_audio_playback();
                                break;
                            }
                            Some("Error") => {
                                error!(
                                    "TTS Error: {}",
                                    meta["message"].as_str().unwrap_or("unknown")
                                );
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Some(Ok(WsMessage::Close(_))) => {
                    debug!("TTS WebSocket closed");
                    break;
                }
                Some(Err(e)) => {
                    error!("TTS WebSocket error: {}", e);
                    break;
                }
                None => break,
                _ => {}
            }
        }

        // Stop audio output
        self.stop_audio_output();

        // Close connection
        let _ = ws_tx.send(WsMessage::Close(None)).await;

        info!("TTS session completed");
        Ok(())
    }

    // ── Audio Output ──

    /// Start audio output stream for TTS playback.
    fn start_audio_output(&mut self) -> Result<(), VoiceError> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or(VoiceError::NoOutputDevice)?;

        let sample_rate = self.config.tts_sample_rate;
        let channels = 1u16;

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let audio_buffer = self.audio_buffer.clone();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    let mut buf = audio_buffer.lock().unwrap();

                    for sample in data.iter_mut() {
                        // Get next sample from buffer
                        loop {
                            if let Some(front) = buf.front_mut() {
                                if front.len() >= 2 {
                                    // Extract i16 sample (2 bytes LE)
                                    let bytes = [front.remove(0), front.remove(0)];
                                    *sample = i16::from_le_bytes(bytes);
                                    break;
                                } else if front.is_empty() {
                                    buf.pop_front();
                                } else {
                                    // Single byte leftover - discard
                                    front.remove(0);
                                    buf.pop_front();
                                }
                            } else {
                                // No audio data - output silence
                                *sample = 0;
                                break;
                            }
                        }
                    }
                },
                |err| error!("Audio output error: {}", err),
                None,
            )
            .map_err(|e| VoiceError::AudioStream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| VoiceError::AudioStream(e.to_string()))?;

        self.output_stream = Some(stream);
        debug!("Audio output started ({}Hz mono)", sample_rate);
        Ok(())
    }

    /// Stop audio output.
    fn stop_audio_output(&mut self) {
        self.output_stream = None;
        self.audio_buffer.lock().unwrap().clear();
        debug!("Audio output stopped");
    }

    /// Wait for audio buffer to be fully played.
    fn wait_for_audio_playback(&self) {
        let max_wait = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();

        while start.elapsed() < max_wait {
            let buf = self.audio_buffer.lock().unwrap();
            if buf.is_empty() {
                break;
            }
            drop(buf);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

// ── Deepgram STT Response Types ──

#[derive(Debug, Deserialize)]
struct SttResponse {
    #[serde(default)]
    is_final: Option<bool>,
    #[serde(default)]
    speech_final: Option<bool>,
    #[serde(default)]
    channel: Option<SttChannel>,
}

#[derive(Debug, Deserialize)]
struct SttChannel {
    #[serde(default)]
    alternatives: Option<Vec<SttAlternative>>,
}

#[derive(Debug, Deserialize)]
struct SttAlternative {
    transcript: String,
    #[serde(default)]
    #[allow(dead_code)]
    confidence: Option<f32>,
}

// ── Deepgram TTS Message Types ──

#[derive(Debug, Serialize)]
struct TtsSpeak {
    #[serde(rename = "type")]
    type_: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct TtsFlush {
    #[serde(rename = "type")]
    type_: String,
}

// ── Error Types ──

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("Voice mode is disabled")]
    Disabled,

    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("No input audio device")]
    NoInputDevice,

    #[error("No output audio device")]
    NoOutputDevice,

    #[error("Audio stream error: {0}")]
    AudioStream(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("LLM error: {0}")]
    Llm(String),
}

// ── Push-to-Talk Handler ──

/// Handles push-to-talk key detection and voice session triggering.
pub struct PushToTalkHandler {
    config: VoiceConfig,
    llm_config: wakey_types::config::LlmProviderConfig,
    spine: Spine,
    active: Arc<AtomicBool>,
}

impl PushToTalkHandler {
    pub fn new(
        config: VoiceConfig,
        llm_config: wakey_types::config::LlmProviderConfig,
        spine: Spine,
    ) -> Self {
        Self {
            llm_config,
            config,
            spine,
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Called when push-to-talk key is pressed.
    pub async fn start_session(&mut self) {
        if self.active.load(Ordering::SeqCst) {
            return;
        }

        self.active.store(true, Ordering::SeqCst);

        match VoiceSession::new(
            self.config.clone(),
            self.llm_config.clone(),
            self.spine.clone(),
        ) {
            Ok(mut session) => {
                if let Err(e) = session.start().await {
                    error!("Voice session error: {}", e);
                    self.spine.emit(WakeyEvent::VoiceError {
                        message: e.to_string(),
                    });
                }
            }
            Err(e) => {
                error!("Failed to create voice session: {}", e);
                self.spine.emit(WakeyEvent::VoiceError {
                    message: e.to_string(),
                });
            }
        }

        self.active.store(false, Ordering::SeqCst);
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}
