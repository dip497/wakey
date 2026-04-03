//! Microphone Capture — Audio input via cpal.
//!
//! Captures mono 16kHz PCM16 audio from default input device.
//! Non-blocking: sends audio frames through a channel.
//!
//! Thread-safe: callback runs on audio thread, sends via try_send.

use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Audio frame size in samples (~10ms at 16kHz)
const FRAME_SAMPLES: usize = 160;

/// Mic capture configuration
#[derive(Debug, Clone)]
pub struct MicConfig {
    /// Sample rate (Hz) - 16000 for Deepgram STT
    pub sample_rate: u32,
    /// Channels - 1 for mono
    pub channels: u16,
}

impl Default for MicConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            channels: 1,
        }
    }
}

impl MicConfig {
    pub fn from_voice_config(config: &wakey_types::config::VoiceConfig) -> Self {
        Self {
            sample_rate: config.asr_sample_rate,
            channels: 1,
        }
    }
}

/// Microphone capture handle
///
/// Holds the cpal stream and provides audio frames through a channel.
pub struct MicCapture {
    config: MicConfig,
    stream: Option<Stream>,
    running: Arc<AtomicBool>,
}

impl MicCapture {
    /// Create mic capture with configuration
    pub fn new(config: MicConfig, running: Arc<AtomicBool>) -> Self {
        Self {
            config,
            stream: None,
            running,
        }
    }

    /// Start capturing audio
    ///
    /// Returns a receiver for audio frames (PCM16 samples as Vec<i16>).
    pub fn start(&mut self) -> Result<mpsc::Receiver<Vec<i16>>, MicError> {
        let host = cpal::default_host();

        let device = host.default_input_device().ok_or(MicError::NoInputDevice)?;

        let cpal_config = cpal::StreamConfig {
            channels: self.config.channels,
            sample_rate: cpal::SampleRate(self.config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>(200);
        let running = self.running.clone();

        // Track first callback for logging
        let first_callback = Arc::new(AtomicBool::new(false));

        let stream = device
            .build_input_stream(
                &cpal_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !running.load(Ordering::SeqCst) {
                        return;
                    }

                    // Log first callback
                    if !first_callback.swap(true, Ordering::Relaxed) {
                        debug!("Mic first callback: {} samples", data.len());
                    }

                    // Send audio frame (non-blocking)
                    // Frame size may differ from expected — just send what we get
                    if data.len() >= FRAME_SAMPLES {
                        // Send full frame
                        let frame = data[..FRAME_SAMPLES].to_vec();
                        if audio_tx.try_send(frame).is_err() {
                            // Buffer full, drop frame (acceptable for real-time)
                        }
                    } else {
                        // Small frame, send anyway
                        let frame = data.to_vec();
                        if audio_tx.try_send(frame).is_err() {
                            // Buffer full
                        }
                    }
                },
                |err| error!("Mic audio error: {}", err),
                None,
            )
            .map_err(|e| MicError::Stream(e.to_string()))?;

        stream.play().map_err(|e| MicError::Stream(e.to_string()))?;

        self.stream = Some(stream);

        info!(
            "Mic capture started ({}Hz, {} channels)",
            self.config.sample_rate, self.config.channels
        );

        Ok(audio_rx)
    }

    /// Stop capturing audio
    pub fn stop(&mut self) {
        self.stream = None;
        debug!("Mic capture stopped");
    }

    /// Check if capturing
    pub fn is_capturing(&self) -> bool {
        self.stream.is_some()
    }
}

/// Mic capture errors
#[derive(Debug, thiserror::Error)]
pub enum MicError {
    #[error("No input audio device")]
    NoInputDevice,

    #[error("Audio stream error: {0}")]
    Stream(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mic_config_default() {
        let config = MicConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
    }

    #[test]
    fn test_mic_capture_creation() {
        let running = Arc::new(AtomicBool::new(true));
        let config = MicConfig::default();
        let mic = MicCapture::new(config, running);

        assert!(!mic.is_capturing());
    }
}
