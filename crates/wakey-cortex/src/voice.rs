//! Voice Mode — Real-time speech conversation with Wakey.
//!
//! Architecture:
//! Mic capture (cpal) → WebSocket to Qwen ASR → Text → LLM → TTS → Audio output (cpal)
//!
//! Based on Qwen DashScope real-time speech APIs:
//! - ASR: https://www.alibabacloud.com/help/en/model-studio/qwen-real-time-speech-recognition
//! - TTS: https://www.alibabacloud.com/help/en/model-studio/qwen-tts-realtime

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::connect_async;
use tracing::{debug, error, info};
use wakey_spine::Spine;
use wakey_types::config::VoiceConfig;
use wakey_types::WakeyEvent;

// ── Constants ──

/// DashScope WebSocket endpoint (Singapore/international region)
const DASHSCOPE_WS_URL: &str = "wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime";

/// Audio chunk size for sending (~100ms of 16kHz PCM16 audio)
const AUDIO_CHUNK_BYTES: usize = 3200;

/// System prompt for voice conversations
const VOICE_SYSTEM_PROMPT: &str = r#"You are Wakey, a friendly voice companion that lives on the user's laptop.
You're warm, curious, and a little playful. Keep responses conversational and concise — this is a voice conversation, so avoid long paragraphs.
You can see what the user is doing on their screen (when they share it), remember past conversations, and help with tasks.
Be yourself. Don't announce that you're an AI. Just be Wakey."#;

// ── Voice Session State ──

/// The voice session orchestrates the full voice conversation flow.
pub struct VoiceSession {
    config: VoiceConfig,
    spine: Spine,
    api_key: String,
    
    /// Audio input stream (microphone)
    input_stream: Option<Stream>,
    
    /// Audio output stream (speaker)
    output_stream: Option<Stream>,
    
    /// Buffer for outgoing audio (mic → ASR)
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    
    /// Buffer for incoming audio (TTS → speaker)
    audio_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,
    
    /// Flag to signal session should stop
    running: Arc<AtomicBool>,
    
    /// Conversation history (last N messages for context)
    conversation_history: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl VoiceSession {
    /// Create a new voice session.
    pub fn new(config: VoiceConfig, spine: Spine) -> Result<Self, VoiceError> {
        let api_key = std::env::var(&config.dashscope_api_key_env)
            .map_err(|_| VoiceError::MissingApiKey(config.dashscope_api_key_env.clone()))?;
        
        Ok(Self {
            config,
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
    
    /// Start a voice session (push-to-talk or VAD-triggered).
    /// Returns when the session ends.
    pub async fn start(&mut self) -> Result<(), VoiceError> {
        if !self.config.enabled {
            return Err(VoiceError::Disabled);
        }
        
        self.running.store(true, Ordering::SeqCst);
        
        // Emit listening started event
        self.spine.emit(WakeyEvent::VoiceListeningStarted);
        info!("Voice session started");
        
        // Create channel for audio chunks
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(100);
        self.audio_tx = Some(audio_tx);
        
        // Start microphone capture
        self.start_mic_capture()?;
        
        // Start the ASR WebSocket connection
        let transcribed_text = self.run_asr_session(audio_rx).await?;
        
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
    
    /// Start capturing audio from the microphone.
    fn start_mic_capture(&mut self) -> Result<(), VoiceError> {
        let host = cpal::default_host();
        
        let device = host
            .default_input_device()
            .ok_or_else(|| VoiceError::NoInputDevice)?;
        
        let supported_config = device
            .default_input_config()
            .map_err(|e| VoiceError::AudioConfig(e.to_string()))?;
        
        debug!(
            "Mic config: {:?}, format: {:?}",
            supported_config, supported_config.sample_format()
        );
        
        let sample_rate = self.config.asr_sample_rate;
        let channels = 1u16; // Mono for ASR
        
        // Create config for 16kHz mono PCM
        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        
        let audio_tx = self.audio_tx.clone().unwrap();
        let running = self.running.clone();
        
        let stream = device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if !running.load(Ordering::SeqCst) {
                    return;
                }
                
                // Convert i16 samples to bytes (PCM 16-bit little-endian)
                let bytes: Vec<u8> = data
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect();
                
                // Send in chunks
                for chunk in bytes.chunks(AUDIO_CHUNK_BYTES) {
                    if audio_tx.blocking_send(chunk.to_vec()).is_err() {
                        debug!("Audio channel closed");
                        break;
                    }
                }
            },
            |err| {
                error!("Audio input error: {}", err);
            },
            None,
        ).map_err(|e| VoiceError::AudioStream(e.to_string()))?;
        
        stream.play().map_err(|e| VoiceError::AudioStream(e.to_string()))?;
        self.input_stream = Some(stream);
        
        info!("Microphone capture started ({}Hz, mono)", sample_rate);
        Ok(())
    }
    
    /// Stop microphone capture.
    fn stop_mic_capture(&mut self) {
        self.input_stream = None;
        self.audio_tx = None;
        debug!("Microphone capture stopped");
    }
    
    /// Run the ASR WebSocket session and return transcribed text.
    async fn run_asr_session(
        &self,
        mut audio_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<String, VoiceError> {
        let model = &self.config.asr_model;
        let url = format!("{}?model={}", DASHSCOPE_WS_URL, model);
        
        debug!("Connecting to ASR WebSocket: {}", url);
        
        // Build request with auth header
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("OpenAI-Beta", "realtime=v1")
            .body(())
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        info!("ASR WebSocket connected");
        
        let (mut ws_tx, mut ws_rx) = ws_stream.split();
        
        // Send session.update event
        let session_update = AsrSessionUpdate {
            event_id: "event_init".to_string(),
            r#type: "session.update".to_string(),
            session: AsrSessionConfig {
                modalities: vec!["text".to_string()],
                input_audio_format: "pcm".to_string(),
                sample_rate: self.config.asr_sample_rate,
                input_audio_transcription: AsrTranscriptionConfig {
                    language: self.config.language.clone(),
                },
                turn_detection: if self.config.use_server_vad {
                    Some(AsrTurnDetection {
                        r#type: "server_vad".to_string(),
                        threshold: 0.0,
                        silence_duration_ms: 400,
                    })
                } else {
                    None
                },
            },
        };
        
        let session_update_json = serde_json::to_string(&session_update)
            .map_err(|e| VoiceError::Serialization(e.to_string()))?;
        
        ws_tx.send(WsMessage::Text(session_update_json)).await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        debug!("Sent ASR session.update");
        
        let running = self.running.clone();
        let spine = self.spine.clone();
        
        // Track accumulated transcription
        let transcription = Arc::new(Mutex::new(String::new()));
        let transcription_clone = transcription.clone();
        let session_ended = Arc::new(AtomicBool::new(false));
        let session_ended_clone = session_ended.clone();
        
        // Task to receive ASR events
        let receive_task = tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                match ws_rx.next().await {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(event) = serde_json::from_str::<AsrEvent>(&text) {
                            match event.r#type.as_str() {
                                "conversation.item.input_audio_transcription.text" => {
                                    // Intermediate transcription
                                    if let Some(stash) = event.stash {
                                        debug!("Intermediate: {}", stash);
                                        spine.emit(WakeyEvent::VoiceUserSpeaking {
                                            text: stash,
                                            is_final: false,
                                        });
                                    }
                                }
                                "conversation.item.input_audio_transcription.completed" => {
                                    // Final transcription
                                    if let Some(transcript) = event.transcript {
                                        debug!("Final: {}", transcript);
                                        let mut t = transcription_clone.lock().unwrap();
                                        if !t.is_empty() {
                                            t.push(' ');
                                        }
                                        t.push_str(&transcript);
                                    }
                                }
                                "input_audio_buffer.speech_started" => {
                                    debug!("VAD: Speech started");
                                }
                                "input_audio_buffer.speech_stopped" => {
                                    debug!("VAD: Speech stopped");
                                    // In VAD mode, speech stopped means turn is done
                                    session_ended_clone.store(true, Ordering::SeqCst);
                                }
                                "session.finished" => {
                                    debug!("ASR session finished");
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        debug!("ASR WebSocket closed");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("ASR WebSocket error: {}", e);
                        break;
                    }
                    None => break,
                    _ => {}
                }
            }
        });
        
        // Task to send audio chunks
        let send_task = tokio::spawn(async move {
            let mut event_id = 0u64;
            while running.load(Ordering::SeqCst) {
                match audio_rx.recv().await {
                    Some(audio_data) => {
                        // Encode audio as base64
                        let audio_b64 = BASE64.encode(&audio_data);
                        
                        let audio_event = AudioBufferAppend {
                            event_id: format!("event_{}", event_id),
                            r#type: "input_audio_buffer.append".to_string(),
                            audio: audio_b64,
                        };
                        
                        event_id += 1;
                        
                        let event_json = serde_json::to_string(&audio_event)
                            .unwrap_or_default();
                        
                        if ws_tx.send(WsMessage::Text(event_json)).await.is_err() {
                            break;
                        }
                        
                        // Small delay to simulate real-time streaming
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                    None => break,
                }
            }
            
            // Send session.finish when done
            let finish_event = serde_json::json!({
                "event_id": "event_finish",
                "type": "session.finish"
            });
            let _ = ws_tx.send(WsMessage::Text(finish_event.to_string())).await;
        });
        
        // Wait for session to end (VAD detected end of speech or timeout)
        let timeout = tokio::time::Duration::from_secs(30);
        let start = std::time::Instant::now();
        
        while self.running.load(Ordering::SeqCst) 
            && !session_ended.load(Ordering::SeqCst)
            && start.elapsed() < timeout
        {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        
        // Stop receiving
        receive_task.abort();
        send_task.abort();
        
        let result = transcription.lock().unwrap().clone();
        info!("ASR transcription: {}", result);
        
        Ok(result)
    }
    
    /// Get LLM response for the transcribed text.
    async fn get_llm_response(&self, user_text: &str) -> Result<String, VoiceError> {
        // Build messages with history
        let mut messages = vec![
            serde_json::json!({
                "role": "system",
                "content": VOICE_SYSTEM_PROMPT
            }),
        ];
        
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
        
        // Get LLM config
        let llm_config = wakey_types::config::LlmProviderConfig {
            name: "voice".to_string(),
            api_base: std::env::var("LLM_API_BASE")
                .unwrap_or_else(|_| "http://localhost:11434/v1".to_string()),
            model: std::env::var("LLM_MODEL")
                .unwrap_or_else(|_| "qwen2.5:7b".to_string()),
            api_key_env: "LLM_API_KEY".to_string(),
        };
        
        let provider = super::llm::OpenAiCompatible::new(&llm_config)
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
        
        let response = provider.chat(&chat_messages).await
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
            
            // Keep only last 10 messages (5 exchanges)
            while history.len() > 10 {
                history.remove(0);
            }
        }
        
        debug!("LLM response: {}", response);
        Ok(response)
    }
    
    /// Run the TTS WebSocket session.
    async fn run_tts_session(&self, text: &str) -> Result<(), VoiceError> {
        let model = &self.config.tts_model;
        let url = format!("{}?model={}", DASHSCOPE_WS_URL, model);
        
        debug!("Connecting to TTS WebSocket: {}", url);
        
        // Build request with auth header
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .body(())
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        info!("TTS WebSocket connected");
        
        let (mut ws_tx, mut ws_rx) = ws_stream.split();
        
        // Send session.update event
        let session_update = TtsSessionUpdate {
            event_id: "event_init".to_string(),
            r#type: "session.update".to_string(),
            session: TtsSessionConfig {
                modalities: vec!["text".to_string(), "audio".to_string()],
                input_audio_format: None,
                output_audio_format: format!("pcm_{}hz_mono_16bit", self.config.tts_sample_rate),
                voice: self.config.voice.clone(),
                turn_detection: None,
            },
        };
        
        let session_update_json = serde_json::to_string(&session_update)
            .map_err(|e| VoiceError::Serialization(e.to_string()))?;
        
        ws_tx.send(WsMessage::Text(session_update_json)).await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        debug!("Sent TTS session.update");
        
        // Wait for session.created
        let _session_created = ws_rx.next().await
            .ok_or_else(|| VoiceError::WebSocket("No session.created response".to_string()))?;
        
        // Send the text for synthesis
        let text_event = ConversationItemCreate {
            event_id: "event_text".to_string(),
            r#type: "conversation.item.create".to_string(),
            item: ConversationItem {
                r#type: "message".to_string(),
                role: "user".to_string(),
                content: vec![ConversationContent {
                    r#type: "input_text".to_string(),
                    text: text.to_string(),
                }],
            },
        };
        
        let text_event_json = serde_json::to_string(&text_event)
            .map_err(|e| VoiceError::Serialization(e.to_string()))?;
        
        ws_tx.send(WsMessage::Text(text_event_json)).await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        // Create response
        let response_create = serde_json::json!({
            "event_id": "event_response",
            "type": "response.create"
        });
        
        ws_tx.send(WsMessage::Text(response_create.to_string())).await
            .map_err(|e| VoiceError::WebSocket(e.to_string()))?;
        
        // Start audio output stream
        self.start_audio_output()?;
        
        // Receive audio chunks
        let audio_buffer = self.audio_buffer.clone();
        let running = self.running.clone();
        
        while running.load(Ordering::SeqCst) {
            match ws_rx.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    if let Ok(event) = serde_json::from_str::<serde_json::Value>(&text) {
                        match event["type"].as_str() {
                            Some("response.audio.delta") => {
                                if let Some(delta) = event["delta"].as_str() {
                                    // Decode base64 audio
                                    if let Ok(audio_data) = BASE64.decode(delta) {
                                        // Queue for playback
                                        let mut buf = audio_buffer.lock().unwrap();
                                        buf.push_back(audio_data);
                                    }
                                }
                            }
                            Some("response.done") => {
                                debug!("TTS response done");
                                break;
                            }
                            Some("session.finished") => {
                                debug!("TTS session finished");
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
        
        // Wait for audio to finish playing
        self.wait_for_audio_playback();
        
        // Stop audio output
        self.stop_audio_output();
        
        // Finish TTS session
        let finish_event = serde_json::json!({
            "event_id": "event_finish",
            "type": "session.finish"
        });
        let _ = ws_tx.send(WsMessage::Text(finish_event.to_string())).await;
        
        info!("TTS session completed");
        Ok(())
    }
    
    /// Start the audio output stream for TTS playback.
    fn start_audio_output(&mut self) -> Result<(), VoiceError> {
        let host = cpal::default_host();
        
        let device = host
            .default_output_device()
            .ok_or_else(|| VoiceError::NoOutputDevice)?;
        
        let sample_rate = self.config.tts_sample_rate;
        let channels = 1u16;
        
        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        
        let audio_buffer = self.audio_buffer.clone();
        let running = self.running.clone();
        
        let stream = device.build_output_stream(
            &config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                let mut buf = audio_buffer.lock().unwrap();
                
                for sample in data.iter_mut() {
                    // Get next sample from buffer
                    loop {
                        if let Some(front) = buf.front_mut() {
                            if front.len() >= 2 {
                                // Get next sample (i16 = 2 bytes)
                                let bytes = [front.remove(0), front.remove(0)];
                                *sample = i16::from_le_bytes(bytes);
                                break;
                            } else if front.is_empty() {
                                buf.pop_front();
                            } else {
                                // Single byte left, remove it
                                front.remove(0);
                                buf.pop_front();
                            }
                        } else {
                            // No audio data, output silence
                            *sample = 0;
                            break;
                        }
                    }
                }
            },
            |err| {
                error!("Audio output error: {}", err);
            },
            None,
        ).map_err(|e| VoiceError::AudioStream(e.to_string()))?;
        
        stream.play().map_err(|e| VoiceError::AudioStream(e.to_string()))?;
        self.output_stream = Some(stream);
        
        debug!("Audio output started ({}Hz, mono)", sample_rate);
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
        // Wait until buffer is empty
        let max_wait = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();
        
        while start.elapsed() < max_wait {
            let buf = self.audio_buffer.lock().unwrap();
            if buf.is_empty() {
                break;
            }
            drop(buf);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

// ── ASR Types ──

#[derive(Debug, Serialize)]
struct AsrSessionUpdate {
    event_id: String,
    r#type: String,
    session: AsrSessionConfig,
}

#[derive(Debug, Serialize)]
struct AsrSessionConfig {
    modalities: Vec<String>,
    input_audio_format: String,
    sample_rate: u32,
    input_audio_transcription: AsrTranscriptionConfig,
    turn_detection: Option<AsrTurnDetection>,
}

#[derive(Debug, Serialize)]
struct AsrTranscriptionConfig {
    language: String,
}

#[derive(Debug, Serialize)]
struct AsrTurnDetection {
    r#type: String,
    threshold: f32,
    silence_duration_ms: u32,
}

#[derive(Debug, Serialize)]
struct AudioBufferAppend {
    event_id: String,
    r#type: String,
    audio: String,
}

#[derive(Debug, Deserialize)]
struct AsrEvent {
    r#type: String,
    #[serde(default)]
    stash: Option<String>,
    #[serde(default)]
    transcript: Option<String>,
}

// ── TTS Types ──

#[derive(Debug, Serialize)]
struct TtsSessionUpdate {
    event_id: String,
    r#type: String,
    session: TtsSessionConfig,
}

#[derive(Debug, Serialize)]
struct TtsSessionConfig {
    modalities: Vec<String>,
    input_audio_format: Option<String>,
    output_audio_format: String,
    voice: String,
    turn_detection: Option<()>,
}

#[derive(Debug, Serialize)]
struct ConversationItemCreate {
    event_id: String,
    r#type: String,
    item: ConversationItem,
}

#[derive(Debug, Serialize)]
struct ConversationItem {
    r#type: String,
    role: String,
    content: Vec<ConversationContent>,
}

#[derive(Debug, Serialize)]
struct ConversationContent {
    r#type: String,
    text: String,
}

// ── Error Types ──

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("Voice mode is disabled")]
    Disabled,
    
    #[error("Missing API key: {0}")]
    MissingApiKey(String),
    
    #[error("No input audio device available")]
    NoInputDevice,
    
    #[error("No output audio device available")]
    NoOutputDevice,
    
    #[error("Audio configuration error: {0}")]
    AudioConfig(String),
    
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
    session: Option<VoiceSession>,
    config: VoiceConfig,
    spine: Spine,
    active: Arc<AtomicBool>,
}

impl PushToTalkHandler {
    pub fn new(config: VoiceConfig, spine: Spine) -> Self {
        Self {
            session: None,
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
        
        match VoiceSession::new(self.config.clone(), self.spine.clone()) {
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
    
    /// Called when push-to-talk key is released.
    pub fn stop_session(&mut self) {
        if let Some(ref mut session) = self.session {
            session.stop();
        }
    }
    
    /// Check if a session is currently active.
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}