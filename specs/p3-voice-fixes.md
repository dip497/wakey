# P3: Voice Fixes — No Overlap, Smooth Audio

## Problem 1: Voices overlapping
The proactive decision loop (every 30s) sends ShouldSpeak events even when:
- Wakey is already speaking via TTS
- User is currently talking (STT active)
- A voice conversation is in progress

Fix: Add a global "speaking" flag. Decision loop checks it before triggering LLM.

### Implementation
In wakey-app/src/main.rs:
- Add `Arc<AtomicBool>` called `is_speaking` shared between TTS listener and decision loop
- TTS listener sets `is_speaking = true` before speaking, `false` after playback
- Decision loop skips LLM call if `is_speaking == true`
- Also skip if VoiceListeningStarted was recently received (user is talking)
- Add `Arc<AtomicBool>` called `user_talking` — set true on VoiceListeningStarted, false on VoiceListeningStopped

In decision loop:
```rust
if is_speaking.load(Ordering::SeqCst) || user_talking.load(Ordering::SeqCst) {
    // Don't interrupt — skip this cycle
    continue;
}
```

## Problem 2: Audio breaking/stuttering
TTS audio chunks arrive via WebSocket and play through cpal. If chunks arrive faster than playback or with gaps, audio stutters.

Fix: Buffer more audio before starting playback.

### Implementation
In wakey-cortex/src/tts.rs:
- Collect first 3-5 audio chunks before starting playback (pre-buffer ~200ms)
- Use a larger ring buffer for audio output
- Ensure cpal callback feeds silence (zeros) when buffer is empty instead of stopping

## Read first
- crates/wakey-app/src/main.rs (decision loop, TTS listener)
- crates/wakey-cortex/src/tts.rs (TTS playback)

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app
# Speak while Wakey is talking — should NOT overlap
# Listen to TTS — should be smooth, no breaks
```

## Acceptance criteria
- Proactive speech does NOT trigger while TTS is playing
- Proactive speech does NOT trigger while user is speaking
- TTS audio plays smoothly without breaks
- Voice conversation still works end-to-end
- cargo check passes
