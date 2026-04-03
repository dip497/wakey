//! Voice Activity Detection — Simple energy-based VAD.
//!
//! No ONNX dependency for MVP — uses RMS energy threshold.
//! ~1ms processing time, fully local.
//!
//! States:
//! - Silence: RMS below threshold, waiting for speech
//! - Speech: RMS above threshold, confirmed after debounce
//! - Endpointing: Speech ended after min_silence_frames

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// VAD event emitted when state changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    /// User started speaking (debounced)
    SpeechStarted,
    /// User stopped speaking (endpointed)
    SpeechEnded,
    /// Still in speech (continuous)
    SpeechActive,
    /// Still in silence (continuous)
    SilenceActive,
}

/// Simple energy-based VAD configuration
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// RMS threshold for speech detection
    pub energy_threshold: f32,

    /// Min consecutive speech frames to confirm speech started (debounce)
    pub min_speech_frames: u32,

    /// Min consecutive silence frames to confirm speech ended (endpointing)
    pub min_silence_frames: u32,

    /// Sample rate (Hz) - for frame timing
    pub sample_rate: u32,

    /// Frame size in samples
    pub frame_samples: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            // 0.01 RMS ≈ -40dB, good for typical mic levels
            energy_threshold: 0.01,
            // ~100ms of speech to confirm (10 frames @ 10ms each)
            min_speech_frames: 10,
            // ~500ms of silence to endpoint (50 frames @ 10ms each)
            min_silence_frames: 50,
            sample_rate: 16000,
            // 160 samples = 10ms frame at 16kHz
            frame_samples: 160,
        }
    }
}

impl VadConfig {
    /// Create config from voice config
    pub fn from_voice_config(config: &wakey_types::config::VoiceConfig) -> Self {
        Self {
            energy_threshold: 0.01,
            min_speech_frames: 10,
            // Use endpointing_ms from config (convert to frames)
            min_silence_frames: config.endpointing_ms / 10,
            sample_rate: config.asr_sample_rate,
            frame_samples: (config.asr_sample_rate / 100) as usize, // 10ms frames
        }
    }
}

/// Simple energy-based VAD state machine
pub struct SimpleVad {
    config: VadConfig,

    /// Consecutive frames with speech (RMS > threshold)
    speech_frames: u32,

    /// Consecutive frames with silence (RMS < threshold)
    silence_frames: u32,

    /// Current state: are we in speech mode?
    in_speech: bool,

    /// Has speech_started been emitted?
    speech_started_emitted: bool,
}

impl SimpleVad {
    /// Create VAD with configuration
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            speech_frames: 0,
            silence_frames: 0,
            in_speech: false,
            speech_started_emitted: false,
        }
    }

    /// Process an audio frame and return the VAD event
    ///
    /// Audio is PCM16 samples (i16). Frame size should match config.frame_samples.
    pub fn process(&mut self, audio: &[i16]) -> VadEvent {
        let rms = calculate_rms(audio);

        if rms > self.config.energy_threshold {
            // Speech detected
            self.speech_frames += 1;
            self.silence_frames = 0;

            if self.in_speech {
                // Already in speech mode, continue
                VadEvent::SpeechActive
            } else if self.speech_frames >= self.config.min_speech_frames {
                // Debounce satisfied — confirm speech started
                self.in_speech = true;
                self.speech_started_emitted = true;
                debug!(
                    rms = rms,
                    frames = self.speech_frames,
                    "VAD: Speech started"
                );
                VadEvent::SpeechStarted
            } else {
                // Still debouncing
                VadEvent::SilenceActive
            }
        } else {
            // Silence detected
            self.silence_frames += 1;
            self.speech_frames = 0;

            if self.in_speech {
                // In speech mode, check for endpointing
                if self.silence_frames >= self.config.min_silence_frames {
                    // Endpointing satisfied — speech ended
                    self.in_speech = false;
                    self.speech_started_emitted = false;
                    debug!(rms = rms, frames = self.silence_frames, "VAD: Speech ended");
                    VadEvent::SpeechEnded
                } else {
                    // Still in speech, waiting for endpoint
                    VadEvent::SpeechActive
                }
            } else {
                // Silence mode
                VadEvent::SilenceActive
            }
        }
    }

    /// Check if currently in speech mode
    pub fn is_speaking(&self) -> bool {
        self.in_speech
    }

    /// Reset VAD state
    pub fn reset(&mut self) {
        self.speech_frames = 0;
        self.silence_frames = 0;
        self.in_speech = false;
        self.speech_started_emitted = false;
    }
}

/// Calculate RMS energy of PCM16 audio samples
///
/// RMS = sqrt(sum(sample^2) / n)
/// Normalized to 0.0-1.0 range (assuming i16 max = 32767)
fn calculate_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples
        .iter()
        .map(|s| {
            let s_f = *s as f64;
            s_f * s_f
        })
        .sum();

    let n = samples.len() as f64;
    let rms = (sum_sq / n).sqrt();

    // Normalize to 0-1 range (max i16 = 32767) and cast to f32
    (rms / 32767.0) as f32
}

/// VAD processor task
///
/// Receives audio frames from mic, runs VAD, emits events.
/// Stops when running flag is false or channel closes.
pub struct VadProcessor {
    config: VadConfig,
    running: Arc<AtomicBool>,
}

impl VadProcessor {
    pub fn new(config: VadConfig, running: Arc<AtomicBool>) -> Self {
        Self { config, running }
    }

    /// Run VAD processing loop
    ///
    /// Returns a sender to send audio frames, and a receiver for VAD events.
    pub fn run(self) -> (mpsc::Sender<Vec<i16>>, mpsc::Receiver<VadEvent>) {
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<i16>>(100);
        let (event_tx, event_rx) = mpsc::channel::<VadEvent>(32);

        let running = self.running;

        tokio::spawn(async move {
            let mut vad = SimpleVad::new(self.config);
            info!("VAD processor started");

            while running.load(Ordering::SeqCst) {
                match audio_rx.recv().await {
                    Some(audio_frame) => {
                        let event = vad.process(&audio_frame);

                        // Only emit meaningful state changes
                        match event {
                            VadEvent::SpeechStarted | VadEvent::SpeechEnded => {
                                if event_tx.send(event).await.is_err() {
                                    debug!("VAD event channel closed");
                                    break;
                                }
                            }
                            VadEvent::SpeechActive | VadEvent::SilenceActive => {
                                // Continuous state, don't spam events
                            }
                        }
                    }
                    None => {
                        debug!("VAD audio channel closed");
                        break;
                    }
                }
            }

            info!("VAD processor stopped");
        });

        (audio_tx, event_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_rms_silence() {
        // All zeros = silence
        let silence: Vec<i16> = vec![0, 0, 0, 0, 0];
        let rms = calculate_rms(&silence);
        assert_eq!(rms, 0.0);
    }

    #[test]
    fn test_calculate_rms_max() {
        // Max amplitude
        let max: Vec<i16> = vec![32767, 32767, 32767];
        let rms = calculate_rms(&max);
        assert!((rms - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_rms_mid() {
        // Mid amplitude
        let mid: Vec<i16> = vec![16000, 16000, 16000];
        let rms = calculate_rms(&mid);
        assert!((rms - 0.5).abs() < 0.02);
    }

    #[test]
    fn test_vad_speech_detection() {
        let config = VadConfig {
            energy_threshold: 0.01,
            min_speech_frames: 3,
            min_silence_frames: 5,
            sample_rate: 16000,
            frame_samples: 160,
        };

        let mut vad = SimpleVad::new(config);

        // Start with silence
        let silence: Vec<i16> = vec![0; 160];
        for _ in 0..10 {
            let event = vad.process(&silence);
            assert_eq!(event, VadEvent::SilenceActive);
        }
        assert!(!vad.is_speaking());

        // Speech frames (above threshold)
        let speech: Vec<i16> = vec![5000; 160]; // RMS ~0.15
        let event1 = vad.process(&speech);
        assert_eq!(event1, VadEvent::SilenceActive); // debounce not satisfied

        let event2 = vad.process(&speech);
        assert_eq!(event2, VadEvent::SilenceActive);

        let event3 = vad.process(&speech);
        assert_eq!(event3, VadEvent::SpeechStarted); // debounce satisfied!
        assert!(vad.is_speaking());

        // Continue speech
        let event4 = vad.process(&speech);
        assert_eq!(event4, VadEvent::SpeechActive);

        // Back to silence - endpointing
        for i in 0..5 {
            let event = vad.process(&silence);
            if i < 4 {
                assert_eq!(event, VadEvent::SpeechActive); // still in speech
            } else {
                assert_eq!(event, VadEvent::SpeechEnded); // endpointed
            }
        }
        assert!(!vad.is_speaking());
    }

    #[test]
    fn test_vad_reset() {
        let config = VadConfig::default();
        let mut vad = SimpleVad::new(config);

        // Force into speech state
        let speech: Vec<i16> = vec![5000; 160];
        for _ in 0..20 {
            vad.process(&speech);
        }
        assert!(vad.is_speaking());

        // Reset
        vad.reset();
        assert!(!vad.is_speaking());
        assert_eq!(vad.speech_frames, 0);
        assert_eq!(vad.silence_frames, 0);
    }
}
