# P4: LiveKit-Quality Voice — Interruption, Turn Detection, Smooth Audio

## Goal
Replace our janky voice with LiveKit-quality conversation. Natural interruptions, proper turn detection, smooth audio. Still using Deepgram STT + Groq LLM + Deepgram TTS.

## What to change

### 1. Remove old voice.rs completely
Delete crates/wakey-cortex/src/voice.rs — it's turn-based, no interruption, cycles every few seconds.

### 2. New voice pipeline (crates/wakey-cortex/src/voice/)
Create a module directory with proper pipeline architecture:

```
voice/
├── mod.rs          # Pipeline orchestrator
├── mic.rs          # Mic capture via cpal (already working, reuse)
├── vad.rs          # Voice Activity Detection (Silero ONNX or simple energy-based)
├── stt.rs          # Deepgram STT WebSocket (reuse existing, keep connection open)
├── tts.rs          # Deepgram TTS WebSocket (reuse from tts.rs)
├── speaker.rs      # Audio output via cpal
└── pipeline.rs     # The flow: mic → vad → stt → llm → tts → speaker with interruption
```

### 3. Pipeline flow with interruption

```
ALWAYS RUNNING:
  Mic → VAD (local, ~1ms) → is someone speaking?

WHEN USER STARTS SPEAKING:
  VAD detects voice → 
    IF TTS is playing → STOP TTS immediately (interruption!)
    Start/continue streaming audio to Deepgram STT

WHEN USER STOPS SPEAKING:
  VAD detects silence for 500ms →
    Get final transcript from Deepgram
    Send to Groq LLM (0.3s response)
    Stream LLM response to Deepgram TTS
    Play audio from TTS

WHEN WAKEY IS SPEAKING:
  TTS audio playing → speaker
  VAD still listening → if user speaks, CANCEL TTS and restart pipeline
```

### 4. VAD — Simple energy-based (no ONNX dependency for MVP)
```rust
struct SimpleVad {
    energy_threshold: f32,  // RMS threshold for speech
    silence_frames: u32,     // Consecutive silent frames
    speech_frames: u32,      // Consecutive speech frames
    min_speech_frames: u32,  // Min frames to confirm speech (debounce)
    min_silence_frames: u32, // Min frames to confirm silence (endpointing)
}

impl SimpleVad {
    fn process(&mut self, audio: &[i16]) -> VadEvent {
        let rms = calculate_rms(audio);
        if rms > self.energy_threshold {
            self.speech_frames += 1;
            self.silence_frames = 0;
            if self.speech_frames >= self.min_speech_frames {
                VadEvent::SpeechStarted
            }
        } else {
            self.silence_frames += 1;
            self.speech_frames = 0;
            if self.silence_frames >= self.min_silence_frames {
                VadEvent::SpeechEnded
            }
        }
    }
}
```

### 5. STT — Keep Deepgram connection alive
Current: connect → listen → disconnect → reconnect (cycling)
New: single persistent WebSocket, always listening. Send audio when VAD says speech. Use Deepgram's `is_final` for turn completion.

### 6. Interruption handling
```rust
struct VoicePipeline {
    vad: SimpleVad,
    stt: DeepgramStt,       // Persistent WebSocket
    tts: DeepgramTts,       // On-demand WebSocket
    speaker: AudioPlayer,
    is_tts_playing: Arc<AtomicBool>,
}

impl VoicePipeline {
    async fn on_vad_speech_started(&mut self) {
        // User started talking
        if self.is_tts_playing.load(Ordering::SeqCst) {
            // INTERRUPTION — stop TTS immediately
            self.speaker.stop();
            self.tts.cancel();
            self.is_tts_playing.store(false, Ordering::SeqCst);
            self.spine.emit(WakeyEvent::VoiceListeningStopped); // TTS cancelled
        }
        self.spine.emit(WakeyEvent::VoiceListeningStarted);
    }
    
    async fn on_vad_speech_ended(&mut self) {
        // User stopped talking — get final transcript and respond
        let text = self.stt.get_final_transcript().await;
        if text.is_empty() { return; }
        
        self.spine.emit(WakeyEvent::VoiceUserSpeaking { text: text.clone(), is_final: true });
        
        // LLM response
        let response = self.llm.chat(&messages).await;
        
        // TTS playback
        self.is_tts_playing.store(true, Ordering::SeqCst);
        self.tts.speak(&response).await; // Can be interrupted by next on_vad_speech_started
        self.is_tts_playing.store(false, Ordering::SeqCst);
    }
}
```

### 7. Keep existing TTS listener for proactive speech
The proactive speech (startup greeting, periodic comments) still goes through ShouldSpeak → TTS.
The voice pipeline handles conversational speech separately.
Both respect is_speaking flag.

## Dependencies
No new deps — reuse cpal, tokio-tungstenite, existing Deepgram code.

## Read first
- crates/wakey-cortex/src/voice.rs (what to replace)
- crates/wakey-cortex/src/tts.rs (TTS code to reuse)
- LiveKit agent pipeline pattern (STT→LLM→TTS with interruption)
- Pipecat interruption handling pattern

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app
# Speak while Wakey is talking → TTS should stop immediately
# Natural conversation flow with ~1s response time
```

## Acceptance criteria
- VAD detects speech start/end locally (no network latency)
- Deepgram STT connection stays open (no cycling)
- User can interrupt Wakey mid-speech — TTS cancels immediately
- Full conversation loop: speak → transcript → LLM → TTS → hear response
- Proactive speech still works (startup greeting, periodic comments)
- No voice overlap
- cargo check passes
