//! Voice Module — LiveKit-quality voice conversation with interruption.
//!
//! Architecture:
//! ```text
//! voice/
//! ├── mod.rs          # This file — exports
//! ├── pipeline.rs     # Orchestrator (mic → vad → stt → llm → tts → speaker)
//! ├── mic.rs          # Mic capture via cpal
//! ├── vad.rs          # Voice Activity Detection (energy-based)
//! ├── stt.rs          # Deepgram STT WebSocket (persistent connection)
//! ├── tts.rs          # Deepgram TTS WebSocket (on-demand)
//! └── speaker.rs      # Audio output via cpal (interruptible)
//! ```
//!
//! Key features:
//! - Natural interruption: user can interrupt Wakey mid-speech
//! - Proper turn detection: VAD + Deepgram endpointing
//! - Persistent STT connection: no cycling, lower latency
//! - Proactive speech support: TTS can be interrupted by voice pipeline

pub mod mic;
pub mod pipeline;
pub mod speaker;
pub mod stt;
pub mod tts;
pub mod vad;

// Re-export main types
pub use mic::{MicCapture, MicConfig, MicError};
pub use pipeline::{InterruptionChecker, PipelineError, VoicePipeline};
pub use speaker::{AudioPlayer, SpeakerConfig, SpeakerError};
pub use stt::{DeepgramStt, SttConfig, SttError, SttResult};
pub use tts::{DeepgramTts, TtsConfig, TtsError};
pub use vad::{SimpleVad, VadConfig, VadEvent, VadProcessor};

// ── Backward-compatible exports (from old voice.rs) ──

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tracing::error;
use wakey_spine::Spine;
use wakey_types::config::VoiceConfig;

use crate::llm::LlmProvider;

/// Voice session error (backward-compatible)
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("Voice mode is disabled")]
    Disabled,

    #[error("Missing API key: {0}")]
    MissingApiKey(String),

    #[error("Pipeline error: {0}")]
    Pipeline(String),
}

/// Voice session handle (backward-compatible)
///
/// Wraps the new VoicePipeline for backward compatibility with existing code.
pub struct VoiceSession {
    config: VoiceConfig,
    llm: Arc<dyn LlmProvider>,
    spine: Spine,
    pipeline: Option<VoicePipeline>,
    running: Arc<AtomicBool>,
}

impl VoiceSession {
    /// Create a new voice session
    pub fn new(
        config: VoiceConfig,
        llm_config: wakey_types::config::LlmProviderConfig,
        spine: Spine,
    ) -> Result<Self, VoiceError> {
        if !config.enabled {
            return Err(VoiceError::Disabled);
        }

        // Create LLM provider
        let llm = Arc::new(
            crate::llm::OpenAiCompatible::new(&llm_config)
                .map_err(|e| VoiceError::Pipeline(e.to_string()))?,
        );

        Ok(Self {
            config,
            llm,
            spine,
            pipeline: None,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the voice session
    ///
    /// Blocks until the session ends or stop() is called.
    pub async fn start(&mut self) -> Result<(), VoiceError> {
        let pipeline =
            VoicePipeline::new(self.config.clone(), self.llm.clone(), self.spine.clone())
                .map_err(|e| VoiceError::Pipeline(e.to_string()))?;

        self.running.store(true, Ordering::SeqCst);
        self.pipeline = Some(pipeline);

        // Run the pipeline
        if let Some(ref mut pipeline) = self.pipeline {
            pipeline
                .run()
                .await
                .map_err(|e| VoiceError::Pipeline(e.to_string()))?;
        }

        Ok(())
    }

    /// Stop the voice session
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(ref pipeline) = self.pipeline {
            pipeline.stop();
        }
        self.spine.emit(wakey_types::WakeyEvent::VoiceSessionEnded);
    }
}

/// Push-to-talk handler (backward-compatible)
///
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

    /// Called when push-to-talk key is pressed
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
                    self.spine.emit(wakey_types::WakeyEvent::VoiceError {
                        message: e.to_string(),
                    });
                }
            }
            Err(e) => {
                error!("Failed to create voice session: {}", e);
                self.spine.emit(wakey_types::WakeyEvent::VoiceError {
                    message: e.to_string(),
                });
            }
        }

        self.active.store(false, Ordering::SeqCst);
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}
