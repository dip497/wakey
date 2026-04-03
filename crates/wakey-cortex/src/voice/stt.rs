//! Deepgram STT — Persistent WebSocket connection.
//!
//! Key change from old voice.rs: connection stays open, no cycling.
//! Send audio when VAD says speech, use is_final for turn completion.
//!
//! Protocol:
//! - Send: raw PCM16 binary bytes (no base64, no JSON)
//! - Receive: JSON with transcript, is_final, speech_final

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async_with_config, tungstenite::client::IntoClientRequest};
use tracing::{debug, error, info};
use wakey_types::{WakeyError, WakeyResult};

/// Deepgram STT WebSocket endpoint
const DEEPGRAM_STT_URL: &str = "wss://api.deepgram.com/v1/listen";

/// STT configuration
#[derive(Debug, Clone)]
pub struct SttConfig {
    /// API key
    api_key: String,

    /// STT model
    model: String,

    /// Sample rate (Hz)
    sample_rate: u32,

    /// Language code
    language: String,

    /// Endpointing timeout (ms)
    endpointing_ms: u32,
}

impl SttConfig {
    pub fn new(
        api_key: String,
        model: String,
        sample_rate: u32,
        language: String,
        endpointing_ms: u32,
    ) -> Self {
        Self {
            api_key,
            model,
            sample_rate,
            language,
            endpointing_ms,
        }
    }

    pub fn from_voice_config(config: &wakey_types::config::VoiceConfig) -> WakeyResult<Self> {
        let api_key = std::env::var(&config.api_key_env)
            .map_err(|_| WakeyError::Config(format!("Missing API key: {}", config.api_key_env)))?;

        Ok(Self::new(
            api_key,
            config.stt_model.clone(),
            config.asr_sample_rate,
            config.language.clone(),
            config.endpointing_ms,
        ))
    }
}

/// Deepgram STT WebSocket client
///
/// Maintains a persistent connection. Can be interrupted.
pub struct DeepgramStt {
    config: SttConfig,
    running: Arc<AtomicBool>,
}

impl DeepgramStt {
    pub fn new(config: SttConfig, running: Arc<AtomicBool>) -> Self {
        Self { config, running }
    }

    /// Run STT session
    ///
    /// Returns (audio_sender, transcript_receiver).
    /// Send PCM16 audio frames to audio_sender.
    /// Receive transcripts from transcript_receiver.
    ///
    /// The connection stays open until running is false or channels close.
    pub fn run(self) -> Result<(mpsc::Sender<Vec<u8>>, mpsc::Receiver<SttResult>), SttError> {
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(200);
        let (result_tx, result_rx) = mpsc::channel::<SttResult>(32);

        let running = self.running.clone();
        let config = self.config;

        tokio::spawn(async move {
            if let Err(e) = Self::run_session(config, running, audio_rx, result_tx).await {
                error!("STT session error: {}", e);
            }
        });

        Ok((audio_tx, result_rx))
    }

    async fn run_session(
        config: SttConfig,
        running: Arc<AtomicBool>,
        mut audio_rx: mpsc::Receiver<Vec<u8>>,
        result_tx: mpsc::Sender<SttResult>,
    ) -> Result<(), SttError> {
        // Build WebSocket URL with query parameters
        let url = format!(
            "{}?encoding=linear16&sample_rate={}&channels=1&model={}&smart_format=true&endpointing={}&language={}",
            DEEPGRAM_STT_URL,
            config.sample_rate,
            config.model,
            config.endpointing_ms,
            config.language
        );

        debug!("Connecting to Deepgram STT: {}", url);

        // Build request with auth header
        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            "Authorization",
            format!("Token {}", config.api_key).parse().unwrap(),
        );

        let (ws_stream, _) = connect_async_with_config(request, None, false)
            .await
            .map_err(|e| SttError::WebSocket(e.to_string()))?;

        info!("Deepgram STT WebSocket connected");

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        let running_tx = running.clone();
        let running_rx = running.clone();

        // Track accumulated final transcript
        let accumulated = Arc::new(std::sync::Mutex::new(String::new()));
        let accumulated_rx = accumulated.clone();

        // Track speech_final flag
        let speech_final = Arc::new(AtomicBool::new(false));
        let speech_final_rx = speech_final.clone();

        // Receive task
        let receive_task = tokio::spawn(async move {
            while running_rx.load(Ordering::SeqCst) {
                match ws_rx.next().await {
                    Some(Ok(WsMessage::Text(text))) => {
                        debug!(
                            "STT response: {}",
                            text.chars().take(100).collect::<String>()
                        );

                        if let Ok(response) = serde_json::from_str::<SttResponse>(&text) {
                            if let Some(transcript) = extract_transcript(&response)
                                && !transcript.is_empty()
                            {
                                let is_final = response.is_final.unwrap_or(false);

                                if is_final {
                                    // Append to accumulated (drop lock before await)
                                    {
                                        let mut acc = accumulated_rx.lock().unwrap();
                                        if !acc.is_empty() {
                                            acc.push(' ');
                                        }
                                        acc.push_str(&transcript);
                                    }
                                    debug!("STT final transcript: {}", transcript);

                                    // Send final result
                                    if result_tx
                                        .send(SttResult {
                                            text: transcript,
                                            is_final: true,
                                            is_speech_final: false,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                } else {
                                    // Interim — send for UI feedback
                                    if result_tx
                                        .send(SttResult {
                                            text: transcript,
                                            is_final: false,
                                            is_speech_final: false,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }

                            // Check for speech_final (turn complete)
                            if response.speech_final.unwrap_or(false) {
                                // Get accumulated text (drop lock before await)
                                let (has_text, acc_text) = {
                                    let acc = accumulated_rx.lock().unwrap();
                                    (!acc.is_empty(), acc.clone())
                                };

                                if has_text {
                                    debug!("STT speech_final — turn complete");
                                    speech_final_rx.store(true, Ordering::SeqCst);

                                    // Send speech_final marker
                                    if result_tx
                                        .send(SttResult {
                                            text: acc_text,
                                            is_final: true,
                                            is_speech_final: true,
                                        })
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                } else {
                                    debug!("STT speech_final but empty — continuing");
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) => {
                        debug!("STT WebSocket closed by server");
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

        // Send task
        let send_task = tokio::spawn(async move {
            while running_tx.load(Ordering::SeqCst) {
                match audio_rx.recv().await {
                    Some(audio_data) => {
                        // Send raw PCM16 bytes
                        if ws_tx
                            .send(WsMessage::Binary(audio_data.into()))
                            .await
                            .is_err()
                        {
                            error!("STT failed to send audio");
                            break;
                        }
                    }
                    None => {
                        debug!("STT audio channel closed");
                        break;
                    }
                }
            }

            // Close connection
            let _ = ws_tx.send(WsMessage::Close(None)).await;
        });

        // Wait for tasks to complete
        let _ = tokio::try_join!(receive_task, send_task);

        info!("STT session ended");
        Ok(())
    }
}

/// Extract transcript from Deepgram response
fn extract_transcript(response: &SttResponse) -> Option<String> {
    response
        .channel
        .as_ref()
        .and_then(|c| c.alternatives.as_ref())
        .and_then(|a| a.first())
        .map(|a| a.transcript.clone())
        .filter(|t| !t.is_empty())
}

/// STT result
#[derive(Debug, Clone)]
pub struct SttResult {
    /// Transcript text
    pub text: String,

    /// Is this a final transcript (vs interim)
    pub is_final: bool,

    /// Did speech end (turn complete)
    pub is_speech_final: bool,
}

// ── Deepgram Response Types ──

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
}

/// STT errors
#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Connection error: {0}")]
    Connection(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for SttError {
    fn from(e: tokio_tungstenite::tungstenite::Error) -> Self {
        SttError::WebSocket(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_transcript() {
        let response = SttResponse {
            is_final: Some(true),
            speech_final: None,
            channel: Some(SttChannel {
                alternatives: Some(vec![SttAlternative {
                    transcript: "hello world".to_string(),
                }]),
            }),
        };

        let transcript = extract_transcript(&response);
        assert_eq!(transcript, Some("hello world".to_string()));
    }

    #[test]
    fn test_extract_transcript_empty() {
        let response = SttResponse {
            is_final: Some(true),
            speech_final: None,
            channel: Some(SttChannel {
                alternatives: Some(vec![SttAlternative {
                    transcript: "".to_string(),
                }]),
            }),
        };

        let transcript = extract_transcript(&response);
        assert_eq!(transcript, None);
    }
}
