//! Speaker Output — Audio playback via cpal.
//!
//! Plays PCM16 audio from a buffer. Interruptible: can stop mid-playback.
//!
//! Used for TTS audio output. The pipeline can stop playback when
//! user interrupts (VAD detects speech while TTS is playing).

use cpal::Stream;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info};

/// Maximum buffer size in bytes (10 seconds of 24kHz mono 16-bit)
const MAX_BUFFER_SIZE: usize = 480_000;

/// Speaker configuration
#[derive(Debug, Clone)]
pub struct SpeakerConfig {
    /// Sample rate (Hz) - 24000 for Deepgram TTS
    pub sample_rate: u32,
    /// Channels - 1 for mono
    pub channels: u16,
}

impl Default for SpeakerConfig {
    fn default() -> Self {
        Self {
            sample_rate: 24000,
            channels: 1,
        }
    }
}

impl SpeakerConfig {
    pub fn from_voice_config(config: &wakey_types::config::VoiceConfig) -> Self {
        Self {
            sample_rate: config.tts_sample_rate,
            channels: 1,
        }
    }
}

/// Audio output player
///
/// Thread-safe. Can be stopped mid-playback from any thread.
pub struct AudioPlayer {
    config: SpeakerConfig,
    stream: Option<Stream>,
    audio_buffer: Arc<Mutex<VecDeque<Vec<u8>>>>,
    playing: Arc<AtomicBool>,
}

impl AudioPlayer {
    pub fn new(config: SpeakerConfig) -> Self {
        Self {
            config,
            stream: None,
            audio_buffer: Arc::new(Mutex::new(VecDeque::new())),
            playing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start audio output stream
    ///
    /// Call this before pushing audio data.
    pub fn start(&mut self) -> Result<(), SpeakerError> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or(SpeakerError::NoOutputDevice)?;

        let cpal_config = cpal::StreamConfig {
            channels: self.config.channels,
            sample_rate: cpal::SampleRate(self.config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let audio_buffer = self.audio_buffer.clone();
        let playing = self.playing.clone();

        let stream = device
            .build_output_stream(
                &cpal_config,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    if !playing.load(Ordering::SeqCst) {
                        // Not playing - output silence
                        for sample in data.iter_mut() {
                            *sample = 0;
                        }
                        return;
                    }

                    let mut buf = audio_buffer.lock().unwrap();

                    for sample in data.iter_mut() {
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
                |err| error!("Speaker error: {}", err),
                None,
            )
            .map_err(|e| SpeakerError::Stream(e.to_string()))?;

        stream
            .play()
            .map_err(|e| SpeakerError::Stream(e.to_string()))?;

        self.stream = Some(stream);

        info!(
            "Speaker started ({}Hz, {} channels)",
            self.config.sample_rate, self.config.channels
        );

        Ok(())
    }

    /// Stop audio output and clear buffer
    pub fn stop(&mut self) {
        self.playing.store(false, Ordering::SeqCst);
        self.stream = None;
        self.audio_buffer.lock().unwrap().clear();
        debug!("Speaker stopped");
    }

    /// Push audio data to play
    ///
    /// Call after start(). Audio is played in order received.
    pub fn push(&self, audio: Vec<u8>) {
        let mut buf = self.audio_buffer.lock().unwrap();
        if buf.len() < MAX_BUFFER_SIZE {
            buf.push_back(audio);
        }
    }

    /// Clear audio buffer
    pub fn clear(&self) {
        self.audio_buffer.lock().unwrap().clear();
    }

    /// Set playing state
    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::SeqCst);
    }

    /// Check if playing
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    /// Check if buffer is empty
    pub fn is_buffer_empty(&self) -> bool {
        self.audio_buffer.lock().unwrap().is_empty()
    }

    /// Wait for buffer to be fully played
    pub fn wait_for_playback(&self, max_wait_ms: u64) {
        let start = std::time::Instant::now();

        while start.elapsed().as_millis() < max_wait_ms as u128
            && self.playing.load(Ordering::SeqCst)
            && !self.is_buffer_empty()
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Get playing flag (for interruption check)
    pub fn playing_flag(&self) -> Arc<AtomicBool> {
        self.playing.clone()
    }
}

/// Speaker errors
#[derive(Debug, thiserror::Error)]
pub enum SpeakerError {
    #[error("No output audio device")]
    NoOutputDevice,

    #[error("Audio stream error: {0}")]
    Stream(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speaker_config_default() {
        let config = SpeakerConfig::default();
        assert_eq!(config.sample_rate, 24000);
        assert_eq!(config.channels, 1);
    }

    #[test]
    fn test_speaker_creation() {
        let config = SpeakerConfig::default();
        let speaker = AudioPlayer::new(config);

        assert!(!speaker.is_playing());
        assert!(speaker.is_buffer_empty());
    }

    #[test]
    fn test_speaker_push_and_clear() {
        let config = SpeakerConfig::default();
        let speaker = AudioPlayer::new(config);

        speaker.push(vec![0, 1, 2, 3]);
        assert!(!speaker.is_buffer_empty());

        speaker.clear();
        assert!(speaker.is_buffer_empty());
    }

    #[test]
    fn test_speaker_playing_flag() {
        let config = SpeakerConfig::default();
        let speaker = AudioPlayer::new(config);

        speaker.set_playing(true);
        assert!(speaker.is_playing());

        speaker.set_playing(false);
        assert!(!speaker.is_playing());
    }
}