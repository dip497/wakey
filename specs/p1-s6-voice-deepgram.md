# P1-S6: Voice Mode — Deepgram Real-Time STT + TTS

## Goal
Replace DashScope voice with Deepgram WebSocket streaming. Real-time conversation with Wakey.

## Flow
```
Mic (cpal) → WebSocket to Deepgram STT (streaming, live transcription)
  → Text → LLM (Qwen via existing OpenAiCompatible client)
  → Response text → Deepgram TTS WebSocket (streaming audio back)
  → Speaker (cpal)
```

## Crate
wakey-cortex (rewrite src/voice.rs)

## API Details

### Deepgram STT (Streaming)
- WebSocket: `wss://api.deepgram.com/v1/listen?encoding=linear16&sample_rate=16000&channels=1&model=nova-3&smart_format=true&endpointing=300`
- Auth header: `Authorization: Token DEEPGRAM_API_KEY`
- Send: raw PCM16 audio bytes (NOT base64, NOT JSON — just binary frames)
- Receive: JSON with `channel.alternatives[0].transcript` and `is_final` boolean
- Docs: https://developers.deepgram.com/docs/getting-started-with-live-streaming-audio

### Deepgram TTS (Streaming)
- WebSocket: `wss://api.deepgram.com/v1/speak?encoding=linear16&sample_rate=24000&model=aura-2-theia-en`
- Auth header: `Authorization: Token DEEPGRAM_API_KEY`
- Send: JSON `{"type": "Speak", "text": "Hello world"}` then `{"type": "Flush"}`
- Receive: raw PCM16 audio binary frames
- Docs: https://developers.deepgram.com/docs/text-to-speech-websocket

## What to implement

### Rewrite voice.rs completely:

1. **MicCapture** — cpal, 16kHz mono PCM16, send raw bytes via channel
2. **DeepgramStt** — WebSocket to Deepgram, send raw audio bytes, receive JSON transcripts
3. **DeepgramTts** — WebSocket to Deepgram, send text JSON, receive raw audio bytes
4. **AudioPlayer** — cpal output, play received PCM16 chunks
5. **VoiceSession** — orchestrates: mic → STT → LLM → TTS → speaker

### Key differences from DashScope version:
- Deepgram STT takes **raw binary PCM** not base64 JSON
- Deepgram TTS takes **JSON messages** not OpenAI realtime format
- Auth is `Token xxx` not `Bearer xxx`
- Much simpler protocol

### Config changes:
Update VoiceConfig in wakey-types/src/config.rs:
```rust
pub struct VoiceConfig {
    pub enabled: bool,
    pub provider: String,           // "deepgram"
    pub api_key_env: String,        // "DEEPGRAM_API_KEY"
    pub stt_model: String,          // "nova-3"
    pub tts_model: String,          // "aura-2-theia-en"
    pub asr_sample_rate: u32,       // 16000
    pub tts_sample_rate: u32,       // 24000
    pub language: String,           // "en"
}
```

Update config/default.toml:
```toml
[voice]
enabled = true
provider = "deepgram"
api_key_env = "DEEPGRAM_API_KEY"
stt_model = "nova-3"
tts_model = "aura-2-theia-en"
asr_sample_rate = 16000
tts_sample_rate = 24000
language = "en"
```

### Spine events (already exist):
- VoiceListeningStarted — when mic starts
- VoiceUserSpeaking { text, is_final } — streaming transcription
- VoiceWakeyThinking — waiting for LLM
- VoiceWakeySpeaking { text } — TTS playing
- VoiceSessionEnded
- VoiceError { message }

## Read first
- crates/wakey-cortex/src/voice.rs (current DashScope implementation to understand structure)
- crates/wakey-cortex/AGENTS.md
- https://developers.deepgram.com/docs/getting-started-with-live-streaming-audio
- https://developers.deepgram.com/docs/text-to-speech-websocket

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app  # speak, Wakey should respond
```

## Acceptance criteria
- Mic captures audio and streams to Deepgram STT
- Real-time transcription appears in logs
- After speech ends (endpointing), LLM generates response
- TTS streams audio back through speaker
- Overlay shows listening/thinking/speaking states
- No 401 errors
- cargo check passes
