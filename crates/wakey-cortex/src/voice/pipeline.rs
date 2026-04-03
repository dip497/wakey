//! Voice Pipeline — Orchestrates the full conversation flow with interruption.
//!
//! Pipeline flow:
//! ```text
//! ALWAYS RUNNING:
//!   Mic → VAD (local, ~1ms) → is someone speaking?
//!
//! WHEN USER STARTS SPEAKING:
//!   VAD detects voice →
//!     IF TTS is playing → STOP TTS immediately (interruption!)
//!     Start/continue streaming audio to Deepgram STT
//!
//! WHEN USER STOPS SPEAKING:
//!   VAD detects silence for N ms →
//!     Get final transcript from Deepgram
//!     Send to LLM
//!     Stream LLM response to Deepgram TTS
//!     Play audio from TTS
//!
//! WHEN WAKEY IS SPEAKING:
//!   TTS audio playing → speaker
//!   VAD still listening → if user speaks, CANCEL TTS and restart pipeline
//! ```
//!
//! This module is the orchestrator. It coordinates all the voice components.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::{error, info};

use wakey_spine::Spine;
use wakey_types::config::VoiceConfig;
use wakey_types::{ChatMessage, WakeyError, WakeyEvent};

use crate::llm::LlmProvider;
use crate::voice::mic::{MicCapture, MicConfig, MicError};
use crate::voice::speaker::{AudioPlayer, SpeakerConfig, SpeakerError};
use crate::voice::stt::{DeepgramStt, SttConfig, SttError};
use crate::voice::tts::{DeepgramTts, TtsConfig, TtsError};
use crate::voice::vad::{SimpleVad, VadConfig, VadEvent};

/// System prompt for voice conversations
const VOICE_SYSTEM_PROMPT: &str = r#"You are Wakey, a friendly voice companion that lives on the user's laptop.
You're warm, curious, and a little playful. Keep responses conversational and concise — this is a voice conversation, so avoid long paragraphs.
You can see what the user is doing on their screen, remember past conversations, and help with tasks.
Be yourself. Don't announce that you're an AI. Just be Wakey."#;

/// Voice pipeline errors
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error("Microphone error: {0}")]
    Mic(#[from] MicError),

    #[error("Speaker error: {0}")]
    Speaker(#[from] SpeakerError),

    #[error("STT error: {0}")]
    Stt(#[from] SttError),

    #[error("TTS error: {0}")]
    Tts(#[from] TtsError),

    #[error("LLM error: {0}")]
    Llm(#[from] WakeyError),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Voice pipeline orchestrator
///
/// Manages the full voice conversation flow with interruption support.
pub struct VoicePipeline {
    /// Voice configuration
    config: VoiceConfig,

    /// LLM provider for responses
    llm: Arc<dyn LlmProvider>,

    /// Event spine for emitting events
    spine: Spine,

    /// Running flag (shared with all components)
    running: Arc<AtomicBool>,

    /// Is TTS currently playing? (for interruption check)
    is_tts_playing: Arc<AtomicBool>,

    /// Conversation history (last N exchanges)
    history: Vec<ChatMessage>,
}

impl VoicePipeline {
    /// Create a new voice pipeline
    pub fn new(
        config: VoiceConfig,
        llm: Arc<dyn LlmProvider>,
        spine: Spine,
    ) -> Result<Self, PipelineError> {
        Ok(Self {
            config,
            llm,
            spine,
            running: Arc::new(AtomicBool::new(false)),
            is_tts_playing: Arc::new(AtomicBool::new(false)),
            history: Vec::with_capacity(10),
        })
    }

    /// Run the voice pipeline
    ///
    /// Blocks until shutdown. Call stop() to stop.
    pub async fn run(&mut self) -> Result<(), PipelineError> {
        self.running.store(true, Ordering::SeqCst);

        info!("Voice pipeline starting");

        // Setup components
        let mic_config = MicConfig::from_voice_config(&self.config);
        let vad_config = VadConfig::from_voice_config(&self.config);
        let stt_config = SttConfig::from_voice_config(&self.config)?;
        let tts_config = TtsConfig::from_voice_config(&self.config)?;
        let speaker_config = SpeakerConfig::from_voice_config(&self.config);

        // Start mic capture
        let mut mic = MicCapture::new(mic_config, self.running.clone());
        let mut mic_rx = mic.start()?;

        // Start speaker
        let mut speaker = AudioPlayer::new(speaker_config);
        speaker.start()?;

        // Start STT
        let stt = DeepgramStt::new(stt_config, self.running.clone());
        let (stt_audio_tx, mut stt_result_rx) = stt.run()?;

        // Setup VAD
        let mut vad = SimpleVad::new(vad_config);

        // Emit listening started
        self.spine.emit(WakeyEvent::VoiceListeningStarted);

        // Main loop
        while self.running.load(Ordering::SeqCst) {
            tokio::select! {
                // Receive audio from mic
                Some(audio_frame) = mic_rx.recv() => {
                    // Run VAD
                    let vad_event = vad.process(&audio_frame);

                    match vad_event {
                        VadEvent::SpeechStarted => {
                            info!("VAD: User started speaking");

                            // INTERRUPTION CHECK
                            if self.is_tts_playing.load(Ordering::SeqCst) {
                                info!("INTERRUPTION: Stopping TTS");
                                speaker.set_playing(false);
                                speaker.clear();
                                self.is_tts_playing.store(false, Ordering::SeqCst);
                                self.spine.emit(WakeyEvent::VoiceListeningStopped);
                            }

                            self.spine.emit(WakeyEvent::VoiceListeningStarted);
                        }
                        VadEvent::SpeechEnded => {
                            info!("VAD: User stopped speaking (endpointed)");

                            // Wait for STT to finish
                            // The STT will send is_speech_final when the turn is complete
                        }
                        VadEvent::SpeechActive | VadEvent::SilenceActive => {
                            // Continuous state, no action needed
                        }
                    }

                    // Send audio to STT if listening (or always send to keep connection active)
                    // Deepgram handles silence gracefully
                    let bytes: Vec<u8> = audio_frame
                        .iter()
                        .flat_map(|s| s.to_le_bytes())
                        .collect();

                    if stt_audio_tx.try_send(bytes).is_err() {
                        // Buffer full, drop audio (acceptable for real-time)
                    }
                }

                // Receive transcript from STT
                Some(result) = stt_result_rx.recv() => {
                    if result.is_speech_final {
                        // Turn complete — process and respond
                        info!("STT: Turn complete: {}", result.text);

                        if !result.text.is_empty() {
                            // Stop listening
                            self.spine.emit(WakeyEvent::VoiceListeningStopped);

                            // Emit what user said
                            self.spine.emit(WakeyEvent::VoiceUserSpeaking {
                                text: result.text.clone(),
                                is_final: true,
                            });

                            // Get LLM response
                            let response = self.get_llm_response(&result.text).await?;

                            // Speak response
                            self.speak_response(&response, &tts_config, &mut speaker).await?;
                        }
                    } else if result.is_final {
                        // Final interim (not speech_final)
                        self.spine.emit(WakeyEvent::VoiceUserSpeaking {
                            text: result.text.clone(),
                            is_final: true,
                        });
                    } else {
                        // Interim — emit for UI feedback
                        self.spine.emit(WakeyEvent::VoiceUserSpeaking {
                            text: result.text.clone(),
                            is_final: false,
                        });
                    }
                }

                // Small sleep to prevent tight loop
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }

        // Cleanup
        mic.stop();
        speaker.stop();
        self.spine.emit(WakeyEvent::VoiceSessionEnded);

        info!("Voice pipeline stopped");
        Ok(())
    }

    /// Stop the voice pipeline
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if pipeline is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Check if currently speaking
    pub fn is_speaking(&self) -> bool {
        self.is_tts_playing.load(Ordering::SeqCst)
    }

    /// Get LLM response for user text
    async fn get_llm_response(&mut self, user_text: &str) -> Result<String, PipelineError> {
        self.spine.emit(WakeyEvent::VoiceWakeyThinking);

        // Build messages
        let mut messages = vec![ChatMessage::system(VOICE_SYSTEM_PROMPT)];
        messages.extend(self.history.clone());
        messages.push(ChatMessage::user(user_text));

        // Call LLM
        let response = self.llm.chat(&messages).await?;

        // Update history
        self.history.push(ChatMessage::user(user_text.to_string()));
        self.history.push(ChatMessage::assistant(response.clone()));

        // Limit history
        if self.history.len() > 10 {
            self.history.drain(0..2);
        }

        Ok(response)
    }

    /// Speak response through TTS
    async fn speak_response(
        &mut self,
        text: &str,
        tts_config: &TtsConfig,
        speaker: &mut AudioPlayer,
    ) -> Result<(), PipelineError> {
        if text.is_empty() {
            return Ok(());
        }

        self.spine.emit(WakeyEvent::VoiceWakeySpeaking {
            text: text.to_string(),
        });

        self.is_tts_playing.store(true, Ordering::SeqCst);
        speaker.set_playing(true);

        // Create audio channel
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(100);

        let tts = DeepgramTts::new(tts_config.clone(), self.running.clone());
        let text_owned = text.to_string(); // Clone for the async task

        // Spawn TTS task
        let mut tts_task = tokio::spawn(async move {
            if let Err(e) = tts.speak(&text_owned, audio_tx).await {
                error!("TTS error: {}", e);
            }
        });

        // Play audio from TTS
        loop {
            if !self.running.load(Ordering::SeqCst) || !self.is_tts_playing.load(Ordering::SeqCst) {
                break;
            }

            tokio::select! {
                Some(audio) = audio_rx.recv() => {
                    speaker.push(audio);
                }
                _ = &mut tts_task => {
                    // TTS finished
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        }

        // Wait for audio to finish playing (unless interrupted)
        if self.is_tts_playing.load(Ordering::SeqCst) {
            speaker.wait_for_playback(30_000);
        }

        self.is_tts_playing.store(false, Ordering::SeqCst);
        speaker.set_playing(false);

        // Re-emit listening if still running
        if self.running.load(Ordering::SeqCst) {
            self.spine.emit(WakeyEvent::VoiceListeningStarted);
        }

        Ok(())
    }
}

/// Interruption checker
///
/// Checks if user is speaking (for external interruption of proactive TTS)
pub struct InterruptionChecker {
    vad: SimpleVad,
    running: Arc<AtomicBool>,
}

impl InterruptionChecker {
    pub fn new(config: &VoiceConfig) -> Self {
        let vad_config = VadConfig::from_voice_config(config);
        Self {
            vad: SimpleVad::new(vad_config),
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Check if the given audio frame indicates interruption
    pub fn check(&mut self, audio: &[i16]) -> bool {
        let event = self.vad.process(audio);
        matches!(event, VadEvent::SpeechStarted)
    }

    /// Stop the checker
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
