# Runtime Context

## Development Environment
- OS: Ubuntu Linux (6.17.0)
- Rust: stable (2024 edition)
- Cargo workspace at: /home/dipendra-sharma/projects/wakey/
- Shell: bash

## Build Commands
```bash
# Check all crates compile
cargo check --workspace

# Lint (must be 0 warnings)
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all -- --check

# Test
cargo test --workspace

# Build release (optimized for size)
cargo build --release
```

## Runtime Dependencies
- No external services required for core functionality
- Optional: Ollama at localhost:11434 for local LLM
- Optional: OpenRouter API for cloud LLM (needs OPENROUTER_API_KEY env var)
- Optional: GLM API (needs GLM_API_KEY env var)

## Linux Desktop Dependencies
- X11 or Wayland for overlay window
- xdotool for input injection (Linux)
- AT-SPI2 for accessibility APIs (usually pre-installed on GNOME/KDE)

## Voice Mode Dependencies (Optional)
Voice mode enables real-time speech conversation with Wakey.

To enable voice mode:
1. Install ALSA development libraries:
   ```bash
   sudo apt-get install libasound2-dev pkg-config
   ```
2. Build with the `voice` feature:
   ```bash
   cargo build --features voice
   ```
3. Set the DASHSCOPE_API_KEY environment variable:
   ```bash
   export DASHSCOPE_API_KEY="sk-xxx"
   ```

Voice mode uses Qwen DashScope APIs:
- ASR: qwen3-asr-flash-realtime (16kHz PCM input)
- TTS: qwen3-tts-flash-realtime (24kHz PCM output)

## Reference Repos (for research)
- ZeroClaw: /home/dipendra-sharma/projects/zeroclaw/
- OpenFang: /home/dipendra-sharma/projects/openfang/
- Paperclip: /home/dipendra-sharma/projects/paperclip/
