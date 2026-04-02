# Wakey — Full Project Brief

## What Is Wakey?
Wakey is an open-source AI companion that lives on your desktop as an always-on-top overlay. It perceives your screen, talks proactively, controls the machine when asked, remembers everything, and grows with you. Domain: wakey.to.

## Tech Stack
- **Language**: Rust 2024 edition, single binary
- **UI**: iced or egui (no webview, no Electron)
- **Async**: tokio
- **LLM**: OpenAI-compatible HTTP client only (works with Ollama, OpenRouter, GLM, vLLM)
- **Memory**: OpenViking-inspired tiered L0/L1/L2 context database
- **Safety**: Cedar policy engine for action guardrails

## Architecture
16-crate Cargo workspace. All inter-crate communication via typed Event Spine (tokio broadcast).

### Crates (in dependency order):
1. `wakey-types` — events, errors, config (depends on nothing)
2. `wakey-spine` — event bus (depends on types)
3. `wakey-senses` — vision, a11y, clipboard, fs, system vitals
4. `wakey-memory` — tiered context database
5. `wakey-heartbeat` — tick/breath/reflect/dream consciousness cycles
6. `wakey-safety` — Cedar policy engine
7. `wakey-cortex` — decision engine + LLM client
8. `wakey-user-model` — user preference tracking
9. `wakey-learning` — self-improving skill loop
10. `wakey-action` — mouse, keyboard, terminal, browser control
11. `wakey-persona` — mood, communication style, evolution
12. `wakey-voice` — TTS/STT
13. `wakey-overlay` — always-on-top window, sprites, chat bubbles
14. `wakey-skills` — trait-based + WASM sandboxed skills
15. `wakey-sdk` — SDK for community skill developers
16. `wakey-app` — binary entry point

## Performance Targets
- Idle RAM: <20MB
- Binary size: <15MB
- Tick latency: <10ms
- Startup: <500ms

## Existing Code
- Cargo workspace is scaffolded (all 16 crates compile with skeleton code)
- `wakey-types` has full event enum, config structs, error types
- `wakey-spine` has working broadcast-based event bus
- `wakey-app` has basic main.rs that initializes spine and waits for Ctrl+C
- Config file exists at `config/default.toml`
- Cedar policies exist at `policies/default.cedar`

## Research Completed
- `docs/research/zeroclaw-impl.md` — ZeroClaw trait system, providers, memory, events
- `docs/research/hermes-impl.md` — Hermes learning loop, skill extraction, user modeling
- `docs/research/openfang-impl.md` — OpenFang multi-crate workspace, WASM, security

## Key Context Files
- `AGENTS.md` — Rules for all AI agents (auto-loaded by GSD)
- `CLAUDE.md` — Full architecture and dependency graph
- `PROJECT.md` — Vision and differentiation
- `RUNTIME.md` — Build commands and environment
- `docs/CODING_STANDARDS.md` — Code patterns with examples
- Per-crate `AGENTS.md` files in critical crates

## Milestones

### M001: First Breath (MVP)
Make Wakey alive on screen — a sprite that knows what you're doing and can talk to you.
- Config loader (parse default.toml)
- Heartbeat tick (active window tracking every 2s)
- OpenAI-compatible LLM client (one provider, streaming)
- Overlay window (always-on-top, transparent, sprite, chat bubble)
- Wire it all together (first conversation about what you're doing)

### M002: Open Eyes
Screen understanding — tiered vision with a11y, OCR, and VLM.

### M003: Grow Hands
Computer use — mouse/keyboard control with Cedar safety policies.

### M004: Remember
Memory system — OpenViking-inspired tiered context, episodic memory, user model.

### M005: Evolve
Personality, learning loop, skill extraction, voice.

## Constraints
- Read `AGENTS.md` and `docs/CODING_STANDARDS.md` before writing any code
- Read relevant `docs/research/*-impl.md` before implementing a subsystem
- Read per-crate `AGENTS.md` when working inside a crate
- All code must pass: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo check`
- No `unwrap()` in library crates, no `unsafe`, no C FFI unless unavoidable
- OpenAI-compatible only for LLM — no vendor SDKs
