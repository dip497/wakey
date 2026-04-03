//! Deepgram TTS — On-demand WebSocket for text-to-speech.
//!
//! Unlike the persistent STT connection, TTS connects on-demand.
//! This allows interruption: we can just close the WebSocket.
//!
//! Protocol:
//! - Send: JSON {"type": "Speak", "text": "..."} then {"type": "Flush"}
//! - Receive: raw PCM16 binary audio frames

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async_with_config, tungstenite::client::IntoClientRequest};
use tracing::{debug, error, info};
use wakey_types::{WakeyError, WakeyResult};

/// Deepgram TTS WebSocket endpoint
const DEEPGRAM_TTS_URL: &str = "wss://api.deepgram.com/v1/speak";

/// Number of audio chunks to pre-buffer before starting playback
const PRE_BUFFER_CHUNKS: usize = 5;

/// TTS configuration
#[derive(Debug, Clone)]
pub struct TtsConfig {
    /// API key
    api_key: String,

    /// TTS model
    model: String,

    /// Sample rate (Hz)
    sample_rate: u32,
}

impl TtsConfig {
    pub fn new(api_key: String, model: String, sample_rate: u32) -> Self {
        Self {
            api_key,
            model,
            sample_rate,
        }
    }

    pub fn from_voice_config(config: &wakey_types::config::VoiceConfig) -> WakeyResult<Self> {
        let api_key = std::env::var(&config.api_key_env)
            .map_err(|_| WakeyError::Config(format!("Missing API key: {}", config.api_key_env)))?;

        Ok(Self::new(
            api_key,
            config.tts_model.clone(),
            config.tts_sample_rate,
        ))
    }
}

/// Deepgram TTS client
///
/// Connects on-demand, streams audio, can be cancelled.
pub struct DeepgramTts {
    config: TtsConfig,
    running: Arc<AtomicBool>,
}

impl DeepgramTts {
    pub fn new(config: TtsConfig, running: Arc<AtomicBool>) -> Self {
        Self { config, running }
    }

    /// Speak text and stream audio to the output channel
    ///
    /// Returns a receiver for audio chunks (PCM16 bytes).
    /// Audio chunks should be pushed to the speaker.
    ///
    /// This method runs until:
    /// - Synthesis completes (Flushed signal from Deepgram)
    /// - running is set to false (interruption)
    /// - Error occurs
    pub async fn speak(self, text: &str, audio_tx: mpsc::Sender<Vec<u8>>) -> Result<(), TtsError> {
        if text.is_empty() {
            return Ok(());
        }

        let url = format!(
            "{}?encoding=linear16&sample_rate={}&model={}",
            DEEPGRAM_TTS_URL, self.config.sample_rate, self.config.model
        );

        debug!("Connecting to Deepgram TTS: {}", url);

        // Build request with auth header
        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", self.config.api_key).parse().unwrap(),
        );

        let (ws_stream, _) = connect_async_with_config(request, None, false)
            .await
            .map_err(|e| TtsError::WebSocket(e.to_string()))?;

        info!("Deepgram TTS WebSocket connected");

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        // Send Speak message
        let speak_msg = TtsMessage::speak(text);
        let speak_json = serde_json::to_string(&speak_msg)?;
        ws_tx
            .send(WsMessage::Text(speak_json.into()))
            .await
            .map_err(|e| TtsError::WebSocket(e.to_string()))?;

        // Send Flush message
        let flush_msg = TtsMessage::flush();
        let flush_json = serde_json::to_string(&flush_msg)?;
        ws_tx
            .send(WsMessage::Text(flush_json.into()))
            .await
            .map_err(|e| TtsError::WebSocket(e.to_string()))?;

        debug!("TTS sent Speak + Flush");

        // Pre-buffer first N chunks
        let mut chunk_count = 0;

        #[allow(clippy::collapsible_if)]
        while chunk_count < PRE_BUFFER_CHUNKS {
            match ws_rx.next().await {
                Some(Ok(WsMessage::Binary(audio_data))) => {
                    if !audio_data.is_empty() {
                        if audio_tx.send(audio_data.to_vec()).await.is_err() {
                            debug!("TTS audio channel closed during pre-buffer");
                            return Ok(());
                        }
                        chunk_count += 1;
                        debug!("TTS pre-buffer chunk {}", chunk_count);
                    }
                }
                Some(Ok(WsMessage::Text(text))) => {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) {
                        match meta["type"].as_str() {
                            Some("Flushed") => {
                                debug!("TTS Flushed during pre-buffer (short text)");
                                return Ok(());
                            }
                            Some("Error") => {
                                error!("TTS error: {}", meta["message"].as_str().unwrap_or("?"));
                                return Err(TtsError::Api(
                                    meta["message"].as_str().unwrap_or("TTS error").to_string(),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
                Some(Ok(WsMessage::Close(_))) => {
                    debug!("TTS WebSocket closed");
                    return Ok(());
                }
                Some(Err(e)) => {
                    return Err(TtsError::WebSocket(e.to_string()));
                }
                None => return Ok(()),
                _ => {}
            }
        }

        debug!("TTS pre-buffer complete, streaming remaining audio");

        // Continue receiving audio
        while self.running.load(Ordering::SeqCst) {
            match ws_rx.next().await {
                #[allow(clippy::collapsible_if)]
                Some(Ok(WsMessage::Binary(audio_data))) => {
                    if !audio_data.is_empty() {
                        if audio_tx.send(audio_data.to_vec()).await.is_err() {
                            debug!("TTS audio channel closed");
                            break;
                        }
                    }
                }
                Some(Ok(WsMessage::Text(text))) => {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&text) {
                        match meta["type"].as_str() {
                            Some("Flushed") => {
                                debug!("TTS synthesis complete");
                                break;
                            }
                            Some("Error") => {
                                error!("TTS error: {}", meta["message"].as_str().unwrap_or("?"));
                                return Err(TtsError::Api(
                                    meta["message"].as_str().unwrap_or("TTS error").to_string(),
                                ));
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
                    return Err(TtsError::WebSocket(e.to_string()));
                }
                None => break,
                _ => {}
            }
        }

        // Close connection
        let _ = ws_tx.send(WsMessage::Close(None)).await;

        info!("TTS session completed");
        Ok(())
    }
}

/// TTS message types
#[derive(Debug, Serialize)]
struct TtsMessage {
    #[serde(rename = "type")]
    type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

impl TtsMessage {
    fn speak(text: &str) -> Self {
        Self {
            type_: "Speak".to_string(),
            text: Some(text.to_string()),
        }
    }

    fn flush() -> Self {
        Self {
            type_: "Flush".to_string(),
            text: None,
        }
    }
}

/// TTS errors
#[derive(Debug, thiserror::Error)]
pub enum TtsError {
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl From<tokio_tungstenite::tungstenite::Error> for TtsError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        TtsError::WebSocket(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tts_message_speak() {
        let msg = TtsMessage::speak("hello");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Speak"));
        assert!(json.contains("hello"));
    }

    #[test]
    fn test_tts_message_flush() {
        let msg = TtsMessage::flush();
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Flush"));
        assert!(!json.contains("text"));
    }
}
