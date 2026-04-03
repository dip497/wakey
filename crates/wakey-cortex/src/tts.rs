//! Standalone TTS Speaker — Speak text through Deepgram TTS.
//!
//! Used by the decision loop to speak ShouldSpeak events through the speaker.
//! Independent of voice session - can be called anytime Wakey wants to say something.
//!
//! Deepgram TTS protocol:
//! - Connect to WebSocket: wss://api.deepgram.com/v1/speak
//! - Send: {"type": "Speak", "text": "..."} then {"type": "Flush"}
//! - Receive: raw PCM16 audio binary frames
//! - Play through cpal output device

use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async_with_config, tungstenite::client::IntoClientRequest};
use tracing::{debug, error, info};
use wakey_types::WakeyResult;

/// Deepgram TTS WebSocket endpoint
const DEEPGRAM_TTS_URL: &str = "wss://api.deepgram.com/v1/speak";

/// Default TTS model (aura-2-theia-en is natural and fast)
const DEFAULT_TTS_MODEL: &str = "aura-2-theia-en";

/// Default sample rate for TTS output
const DEFAULT_SAMPLE_RATE: u32 = 24000;

// ── TTS Speaker ──

/// Standalone TTS speaker that speaks text through Deepgram.
///
/// Thread-safe: can be called from any async context.
/// Uses cpal for audio output (same as voice session).
pub struct TtsSpeaker {
    /// Deepgram API key
    api_key: String,

    /// TTS model name
    model: String,

    /// Audio sample rate (Hz)
    sample_rate: u32,

    /// Audio output stream (speaker)
    output_stream: Option<Stream>,

    /// Buffer for incoming audio (TTS → speaker)
    audio_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,

    /// Flag to signal playback should stop
    playing: Arc<AtomicBool>,
}

impl TtsSpeaker {
    /// Create a new TTS speaker.
    ///
    /// Reads API key from environment variable.
    pub fn new(api_key_env: &str) -> WakeyResult<Self> {
        let api_key = std::env::var(api_key_env).map_err(|_| {
            wakey_types::WakeyError::Config(format!("Missing API key: {}", api_key_env))
        })?;

        Ok(Self {
            api_key,
            model: DEFAULT_TTS_MODEL.to_string(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            output_stream: None,
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            playing: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Create TTS speaker with custom model and sample rate.
    pub fn with_config(api_key_env: &str, model: &str, sample_rate: u32) -> WakeyResult<Self> {
        let api_key = std::env::var(api_key_env).map_err(|_| {
            wakey_types::WakeyError::Config(format!("Missing API key: {}", api_key_env))
        })?;

        Ok(Self {
            api_key,
            model: model.to_string(),
            sample_rate,
            output_stream: None,
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            playing: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Speak text through the speaker.
    ///
    /// Connects to Deepgram TTS, sends text, receives audio, plays through speaker.
    /// Returns when audio playback is complete.
    pub async fn speak(&mut self, text: &str) -> WakeyResult<()> {
        if text.is_empty() {
            return Ok(());
        }

        info!(text_len = text.len(), "TTS speaking");

        // Build WebSocket URL with query parameters
        let url = format!(
            "{}?encoding=linear16&sample_rate={}&model={}",
            DEEPGRAM_TTS_URL, self.sample_rate, self.model
        );

        debug!("Connecting to Deepgram TTS: {}", url);

        // Build request with auth header
        let mut request = url
            .into_client_request()
            .map_err(|e| wakey_types::WakeyError::Network(e.to_string()))?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.api_key).parse().unwrap(),
        );

        // Connect to WebSocket
        let (ws_stream, _) = connect_async_with_config(request, None, false)
            .await
            .map_err(|e| wakey_types::WakeyError::Network(e.to_string()))?;

        info!("Deepgram TTS WebSocket connected");

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        // Send the text for synthesis
        let speak_msg = TtsSpeak {
            type_: "Speak".to_string(),
            text: text.to_string(),
        };
        let speak_json =
            serde_json::to_string(&speak_msg).map_err(wakey_types::WakeyError::Serde)?;

        ws_tx
            .send(WsMessage::Text(speak_json.into()))
            .await
            .map_err(|e| wakey_types::WakeyError::Network(e.to_string()))?;

        // Send flush to signal end of input
        let flush_msg = TtsFlush {
            type_: "Flush".to_string(),
        };
        let flush_json =
            serde_json::to_string(&flush_msg).map_err(wakey_types::WakeyError::Serde)?;

        ws_tx
            .send(WsMessage::Text(flush_json.into()))
            .await
            .map_err(|e| wakey_types::WakeyError::Network(e.to_string()))?;

        debug!("TTS sent Speak + Flush");

        // Start audio output stream
        self.start_audio_output()?;

        self.playing.store(true, Ordering::SeqCst);
        let playing = self.playing.clone();
        let audio_buffer = self.audio_buffer.clone();

        // Receive audio chunks (raw PCM16 binary)
        while playing.load(Ordering::SeqCst) {
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

        info!("TTS playback completed");
        Ok(())
    }

    /// Stop any ongoing playback.
    pub fn stop(&mut self) {
        self.playing.store(false, Ordering::SeqCst);
        self.stop_audio_output();
    }

    /// Check if currently playing audio.
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    // ── Audio Output ──

    /// Start audio output stream for TTS playback.
    fn start_audio_output(&mut self) -> WakeyResult<()> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or_else(|| wakey_types::WakeyError::Hardware("No output audio device".into()))?;

        let channels = 1u16; // Mono

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let audio_buffer = self.audio_buffer.clone();
        let playing = self.playing.clone();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    // Only play if we're still in playing state
                    if !playing.load(Ordering::SeqCst) {
                        for sample in data.iter_mut() {
                            *sample = 0;
                        }
                        return;
                    }

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
            .map_err(|e| wakey_types::WakeyError::Hardware(e.to_string()))?;

        stream
            .play()
            .map_err(|e| wakey_types::WakeyError::Hardware(e.to_string()))?;

        self.output_stream = Some(stream);
        debug!("Audio output started ({}Hz mono)", self.sample_rate);
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
        let max_wait = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();

        while start.elapsed() < max_wait && self.playing.load(Ordering::SeqCst) {
            let buf = self.audio_buffer.lock().unwrap();
            if buf.is_empty() {
                break;
            }
            drop(buf);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // Mark as done
        self.playing.store(false, Ordering::SeqCst);
    }
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

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_speak_serialization() {
        let msg = TtsSpeak {
            type_: "Speak".to_string(),
            text: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"Speak","text":"Hello world"}"#);
    }

    #[test]
    fn test_tts_flush_serialization() {
        let msg = TtsFlush {
            type_: "Flush".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"Flush"}"#);
    }
}
