# P3: Voice Only — No Chat Bubble, Wakey Speaks Through Speaker

## Goal
When Wakey wants to say something, it speaks through the speaker via Deepgram TTS. No chat bubble.

## Current flow (broken)
```
Decision loop → ShouldSpeak event → Overlay shows chat bubble (text)
Voice session → separate, only for push-to-talk STT→LLM→TTS
```

## New flow
```
Decision loop → ShouldSpeak event → TTS engine speaks it through speaker
Overlay → shows sprite animation (talking state) while speaking, NO text bubble
```

## What to implement

### 1. TTS speaker module (crates/wakey-cortex/src/tts.rs)
- New module: standalone TTS that speaks text through Deepgram
- `pub struct TtsSpeaker { api_key, sample_rate }`
- `pub async fn speak(&self, text: &str) -> WakeyResult<()>`
- Connect to Deepgram TTS WebSocket: `wss://api.deepgram.com/v1/speak?encoding=linear16&sample_rate=24000&model=aura-2-theia-en`
- Auth: `Authorization: Token DEEPGRAM_API_KEY`
- Send: `{"type": "Speak", "text": "..."}`  then `{"type": "Flush"}`
- Receive: raw PCM16 audio binary frames
- Play through cpal output device
- Read DEEPGRAM_API_KEY from env

### 2. Wire ShouldSpeak → TTS (crates/wakey-app/src/main.rs)
- In the background thread, spawn a TTS listener task
- Subscribe to spine events
- On ShouldSpeak with suggested_text: call tts_speaker.speak(text)
- Before speaking: emit VoiceWakeySpeaking event (sprite shows talking animation)
- After speaking: emit VoiceSessionEnded

### 3. Remove chat bubble from overlay
- In overlay spine handler: do NOT show bubble on ShouldSpeak
- Instead: on VoiceWakeySpeaking → sprite changes to talking animation
- On VoiceSessionEnded → sprite returns to idle
- Keep the bubble code but disable it (we might want optional text later)

### 4. Startup greeting through speaker
- The 5-second startup greeting should go through TTS, not bubble
- Same flow: emit ShouldSpeak → TTS listener picks it up → speaks

## Dependencies
Already have: cpal, tokio-tungstenite, futures-util (from voice feature)

## Read first
- crates/wakey-cortex/src/voice.rs (existing Deepgram WebSocket code — reuse TTS parts)
- crates/wakey-app/src/main.rs (current ShouldSpeak flow)
- crates/wakey-overlay/src/window.rs (current bubble handling)

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app
# Expected: Wakey speaks "Hi there!" through speaker after 5s. No text bubble.
```

## Acceptance criteria
- ShouldSpeak events trigger TTS audio output
- Audio plays through default speaker
- No chat bubble shown (overlay only shows sprite animation)
- Sprite animates during speech (talking state)
- Startup greeting spoken through speaker
- Periodic comments spoken through speaker
- cargo check passes
