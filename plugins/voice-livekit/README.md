# Wakey Voice Plugin - LiveKit

A voice plugin for Wakey that uses LiveKit Agents for real-time voice interaction.

## Overview

This plugin runs as a subprocess spawned by Wakey's plugin host. It communicates via JSON lines over stdin/stdout while using LiveKit for audio processing.

### Architecture

```
┌─────────────────────────────────────────────────────┐
│                    WAKEY (Rust)                      │
│                                                     │
│  stdin ──────────► Plugin ───────────► stdout       │
│         ShouldSpeak     VoiceEvents                 │
│         Shutdown                                    │
│                                                     │
│  LiveKit Client ◄──────► LiveKit Room ◄──────► Plugin
│  (mic/speaker)              (audio bridge)    (agent)
└─────────────────────────────────────────────────────┘
```

## Configuration

### Environment Variables

Required credentials (passed from Wakey core):

| Variable | Description |
|----------|-------------|
| `LIVEKIT_URL` | LiveKit server URL (e.g., `wss://your-project.livekit.cloud`) |
| `LIVEKIT_API_KEY` | LiveKit API key |
| `LIVEKIT_API_SECRET` | LiveKit API secret |
| `DEEPGRAM_API_KEY` | Deepgram API key (for STT/TTS) |
| `GROQ_API_KEY` | Groq API key (for LLM) |

### Wakey Configuration

In `config/default.toml`:

```toml
[voice]
enabled = true
plugin = "voice-livekit"
plugin_path = "plugins/voice-livekit/main.py"
plugin_command = "python3"

[voice.env]
LIVEKIT_URL = "${LIVEKIT_URL}"
LIVEKIT_API_KEY = "${LIVEKIT_API_KEY}"
LIVEKIT_API_SECRET = "${LIVEKIT_API_SECRET}"
DEEPGRAM_API_KEY = "${DEEPGRAM_API_KEY}"
GROQ_API_KEY = "${GROQ_API_KEY}"
```

## Communication Protocol

### Wakey → Plugin Events

**ShouldSpeak**: Tell Wakey to say something
```json
{"event": "ShouldSpeak", "text": "Hi there!", "urgency": "low"}
```

- `text`: What Wakey should say
- `urgency`: `"low"`, `"medium"`, `"high"`, `"critical"` (affects speech priority)

**Shutdown**: Stop the plugin
```json
{"event": "Shutdown"}
```

### Plugin → Wakey Events

**VoiceListeningStarted**: Agent is ready and listening
```json
{"event": "VoiceListeningStarted"}
```

**VoiceUserSpeaking**: User speech detected (interim or final)
```json
{"event": "VoiceUserSpeaking", "text": "hello", "is_final": false}
{"event": "VoiceUserSpeaking", "text": "hello wakey", "is_final": true}
```

- `text`: Transcribed speech
- `is_final`: `false` for interim, `true` for final transcript

**VoiceWakeyThinking**: Agent is processing (LLM call started)
```json
{"event": "VoiceWakeyThinking"}
```

**VoiceWakeySpeaking**: Agent is speaking
```json
{"event": "VoiceWakeySpeaking", "text": "Hey! What's up?"}
```

- `text`: What the agent is saying

**VoiceSessionEnded**: Session terminated
```json
{"event": "VoiceSessionEnded"}
```

**VoiceError**: Error occurred
```json
{"event": "VoiceError", "message": "Connection failed"}
```

## Voice Pipeline

The plugin uses:

| Component | Provider | Model |
|-----------|----------|-------|
| **STT** | Deepgram | `nova-3` |
| **LLM** | Groq | `llama-3.3-70b-versatile` (via OpenAI-compatible API) |
| **TTS** | Deepgram | `aura-2-asteria-en` |
| **VAD** | Silero | Silero VAD model |
| **Turn Detection** | LiveKit | `MultilingualModel` (14 languages) |

### Features

- **Interruption handling**: Users can interrupt Wakey mid-speech
- **Multilingual turn detection**: Better end-of-turn detection across languages
- **Preemptive generation**: Starts thinking before user finishes speaking
- **VAD**: Fast voice activity detection for responsive interruptions

## Installation

```bash
# Install dependencies
cd plugins/voice-livekit
pip install -r requirements.txt
```

## Running Standalone (for testing)

```bash
# Set environment variables
export LIVEKIT_URL="wss://your-project.livekit.cloud"
export LIVEKIT_API_KEY="your-api-key"
export LIVEKIT_API_SECRET="your-api-secret"
export DEEPGRAM_API_KEY="your-deepgram-key"
export GROQ_API_KEY="your-groq-key"

# Run the plugin
python3 main.py
```

The plugin will connect to LiveKit and wait for stdin events. To test ShouldSpeak:

```bash
echo '{"event": "ShouldSpeak", "text": "Hello, how are you?", "urgency": "low"}' | python3 main.py
```

## LiveKit Setup

### Option 1: LiveKit Cloud

1. Create account at [livekit.cloud](https://livekit.cloud)
2. Create a project
3. Get credentials from project settings
4. Set environment variables

### Option 2: Self-hosted LiveKit

```bash
# Using Docker
docker run -d \
  --name livekit \
  -p 7880:7880 \
  -p 7881:7881 \
  -v $PWD/livekit.yaml:/livekit.yaml \
  livekit/livekit-server \
  --config /livekit.yaml
```

See [LiveKit docs](https://docs.livekit.io) for full self-hosting guide.

## Integration with Wakey

Wakey's plugin host (in `wakey-cortex/src/plugin_host.rs`) will:

1. Spawn this plugin as a subprocess
2. Read stdout events → emit as `WakeyEvent::Voice*` on spine
3. Listen for `WakeyEvent::ShouldSpeak` on spine → write to stdin
4. Restart plugin if it crashes

The plugin doesn't need to know about Wakey's internals—it just follows the JSON protocol.

## Troubleshooting

### Plugin won't start

Check stderr output for logs. Common issues:
- Missing environment variables
- Invalid LiveKit credentials
- Network connectivity issues

### No audio

Ensure Wakey is connected to the same LiveKit room as the agent. Audio flows through the room, not stdin/stdout.

### Latency issues

Try adjusting:
- Use a LiveKit server closer to your location
- Reduce TTS model complexity
- Enable preemptive generation

## License

Same as Wakey project.