# P4: Voice as a Plugin — Pluggable Voice Architecture

## Problem
Voice is currently hardcoded in wakey-cortex. If we want to switch providers (LiveKit, Deepgram, local whisper, or none), we have to rewrite core code. That's wrong.

## Solution
Voice becomes a PLUGIN that connects to Wakey's spine via events. Core doesn't know or care HOW voice works — it just sees spine events.

## Architecture

```
┌──────────────────────────────────────────────┐
│              WAKEY CORE (Rust)                │
│                                              │
│  Spine ← emits/consumes WakeyEvents         │
│  Cortex ← decisions, LLM                    │
│  Overlay ← sprite, animations               │
│  Heartbeat ← tick, breath                    │
│  Memory ← context, skills                    │
│                                              │
│  Core ONLY knows these voice events:         │
│    VoiceListeningStarted                     │
│    VoiceUserSpeaking { text, is_final }      │
│    VoiceWakeyThinking                        │
│    VoiceWakeySpeaking { text }               │
│    VoiceSessionEnded                         │
│    ShouldSpeak { text } → voice plugin picks │
│                           this up and speaks  │
└─────────────────┬────────────────────────────┘
                  │ Spine (events)
                  │
┌─────────────────▼────────────────────────────┐
│          VOICE PLUGIN (any language)          │
│                                              │
│  Connects to spine via:                      │
│    Option A: Unix socket / TCP               │
│    Option B: Subprocess with stdin/stdout     │
│    Option C: Shared memory                   │
│                                              │
│  Plugin implementations:                     │
│    plugins/voice-livekit/  (Python, best)    │
│    plugins/voice-deepgram/ (Rust, current)   │
│    plugins/voice-local/    (Rust, offline)    │
│    plugins/voice-none/     (disabled)        │
└──────────────────────────────────────────────┘
```

## Plugin Protocol (simple JSON over stdin/stdout)

Wakey spawns the voice plugin as a subprocess. Communication via JSON lines:

```
Wakey → Plugin:
  {"event": "ShouldSpeak", "text": "Hi there!", "urgency": "low"}
  {"event": "Shutdown"}

Plugin → Wakey:
  {"event": "VoiceListeningStarted"}
  {"event": "VoiceUserSpeaking", "text": "hello", "is_final": false}
  {"event": "VoiceUserSpeaking", "text": "hello wakey", "is_final": true}
  {"event": "VoiceWakeyThinking"}
  {"event": "VoiceWakeySpeaking", "text": "Hey! What's up?"}
  {"event": "VoiceSessionEnded"}
```

## Implementation

### 1. Plugin host (crates/wakey-cortex/src/plugin_host.rs)
- Spawns plugin subprocess
- Reads JSON lines from stdout → emits as WakeyEvents on spine
- Listens for ShouldSpeak events on spine → writes JSON to stdin
- Restarts plugin if it crashes
- Config: which plugin to use, plugin path, env vars to pass

### 2. Plugin config (config/default.toml)
```toml
[voice]
enabled = true
plugin = "voice-livekit"      # or "voice-deepgram", "voice-local"
plugin_path = "plugins/voice-livekit/main.py"
plugin_command = "python3"     # or "node", or path to binary

[voice.env]
# These env vars are passed to the plugin process
LIVEKIT_URL = "${LIVEKIT_URL}"
LIVEKIT_API_KEY = "${LIVEKIT_API_KEY}"
LIVEKIT_API_SECRET = "${LIVEKIT_API_SECRET}"
DEEPGRAM_API_KEY = "${DEEPGRAM_API_KEY}"
GROQ_API_KEY = "${GROQ_API_KEY}"
```

### 3. LiveKit voice plugin (plugins/voice-livekit/main.py)
- Full LiveKit agent with interruption, VAD, turn detection
- Uses Deepgram STT + Groq LLM + Deepgram TTS (or any combo)
- Reads JSON from stdin (ShouldSpeak events)
- Writes JSON to stdout (voice events)
- All LiveKit complexity hidden inside plugin

### 4. Remove hardcoded voice from cortex
- Delete crates/wakey-cortex/src/voice/ directory
- Delete crates/wakey-cortex/src/tts.rs
- Remove cpal, tokio-tungstenite, base64, futures-util deps from cortex
- Voice feature flag removed — voice is always a plugin now

### 5. Wakey core stays clean
- Core only emits ShouldSpeak and consumes Voice* events
- Doesn't know about Deepgram, LiveKit, cpal, or any voice provider
- Overlay reacts to VoiceWakeySpeaking for animation
- Decision loop checks VoiceListeningStarted for overlap prevention

## Benefits
- Switch voice provider by changing one config line
- Add new voice providers without touching Rust code
- Voice bugs don't crash core
- Core binary gets SMALLER (no audio deps)
- Community can write voice plugins in any language
- No voice? Just don't configure a plugin

## Read first
- docs/architecture/DECISIONS.md
- Current voice code (to understand events)
- LiveKit agent quickstart docs

## Acceptance criteria
- Plugin host spawns subprocess and communicates via JSON lines
- ShouldSpeak → plugin → spoken through speaker
- User speech → plugin → VoiceUserSpeaking event on spine
- LiveKit plugin works with existing creds
- Core compiles WITHOUT any audio dependencies
- cargo check passes
