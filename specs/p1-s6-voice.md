# P1-S6: Voice Mode — Talk to Wakey

## Goal
Real-time voice conversation with Wakey using Qwen DashScope APIs.
User speaks → Wakey hears (STT) → Wakey thinks (LLM) → Wakey speaks back (TTS).

## Architecture

```
Mic capture (cpal crate)
  → WebSocket to Qwen ASR (qwen3-asr-flash-realtime)
  → Text
  → LLM chat (existing OpenAiCompatible client)
  → Response text
  → Qwen TTS (qwen3-tts-flash-realtime) via WebSocket
  → Audio stream
  → Speaker output (cpal crate)
```

## Crate
wakey-cortex (add src/voice.rs)

## API Details

### STT (Speech-to-Text)
- Endpoint: wss://dashscope-intl.aliyuncs.com/api-ws/v1/inference
- Model: qwen3-asr-flash-realtime
- Protocol: WebSocket, send PCM audio chunks, receive text events
- API Key: same DASHSCOPE_API_KEY from .env
- Docs: https://www.alibabacloud.com/help/en/model-studio/qwen-real-time-speech-recognition

### TTS (Text-to-Speech)  
- Endpoint: wss://dashscope-intl.aliyuncs.com/api-ws/v1/inference
- Model: qwen3-tts-flash-realtime
- Protocol: WebSocket, send text, receive PCM audio chunks
- API Key: same DASHSCOPE_API_KEY
- Docs: https://www.alibabacloud.com/help/en/model-studio/qwen-tts-realtime

## What to implement

### 1. Mic capture (src/voice.rs)
- Use `cpal` crate for cross-platform audio input
- Capture PCM audio from default input device
- Push-to-talk: activate on hotkey (e.g., hold Space when Wakey window focused)
- OR: voice activity detection (VAD) — detect when user starts/stops speaking
- Start simple: push-to-talk first

### 2. STT client
- WebSocket connection to Qwen ASR endpoint
- Stream PCM audio chunks as user speaks
- Receive transcribed text events
- Emit text as a spine event when utterance complete

### 3. LLM processing
- Use existing OpenAiCompatible::chat() with the transcribed text
- Add conversation history (last 5 messages) for context
- System prompt: "You are Wakey, a friendly voice companion..."

### 4. TTS client
- WebSocket connection to Qwen TTS endpoint
- Send LLM response text
- Receive PCM audio chunks
- Play audio through default output device via cpal

### 5. Integration with overlay
- When user is speaking: sprite shows "listening" state (ear animation or glow change)
- When Wakey is thinking: sprite shows "thinking" state
- When Wakey is speaking: sprite shows "talking" state (mouth animation)
- Show transcribed text in chat bubble too

## Dependencies to add
```toml
# In workspace Cargo.toml
cpal = "0.15"                    # Cross-platform audio I/O
tokio-tungstenite = { version = "0.26", features = ["rustls-tls-webpki-roots"] }  # WebSocket client
```

## Read first
- docs/architecture/DECISIONS.md
- crates/wakey-cortex/AGENTS.md
- crates/wakey-cortex/src/llm.rs (existing LLM client)
- https://www.alibabacloud.com/help/en/model-studio/qwen-real-time-speech-recognition
- https://www.alibabacloud.com/help/en/model-studio/qwen-tts-realtime

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app  # hold space, speak, hear Wakey respond
```

## Acceptance criteria
- User can speak to Wakey via microphone
- Wakey transcribes speech in real-time
- Wakey responds via TTS (audio output)
- Chat bubble shows both user text and Wakey response
- Sprite animation changes for listening/thinking/speaking states
- cargo check passes
