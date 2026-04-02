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

## Reference Repos (for research)
- ZeroClaw: /home/dipendra-sharma/projects/zeroclaw/
- OpenFang: /home/dipendra-sharma/projects/openfang/
- Paperclip: /home/dipendra-sharma/projects/paperclip/
