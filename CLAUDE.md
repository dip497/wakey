# CLAUDE.md — Wakey Agent Engineering Protocol

## 1. Project Snapshot

Wakey is a Rust-first, event-driven AI companion that lives on your desktop as an always-on-top overlay. It perceives the screen, talks proactively, controls the machine when asked, remembers everything, and learns over time.

**Core philosophy**: Alive, not reactive. Lightweight, not bloated. Safe, not reckless. Growing, not static.

## 2. Architecture

### Workspace Structure

```
wakey/
├── crates/
│   ├── wakey-types/          # Foundation: shared types, events, config, errors
│   ├── wakey-spine/          # Event bus (tokio broadcast channels)
│   ├── wakey-context/        # Memory + skills storage (OpenViking L0/L1/L2 + ZeroClaw Memory trait)
│   ├── wakey-senses/         # Perception: a11y, screen, clipboard, system, git
│   ├── wakey-cortex/         # Brain: ZeroClaw agent loop + LLM client + heartbeat rhythms + decisions
│   ├── wakey-action/         # Hands + safety: input, terminal, browser + Cedar policies
│   ├── wakey-skills/         # Skills runtime: Hermes format + petgraph DAG + WASM sandbox + learning loop
│   ├── wakey-overlay/        # Face: egui, sprites, chat bubbles
│   └── wakey-app/            # Binary entry point
├── policies/                 # Cedar safety policy files
├── viking/                   # OpenViking context filesystem
├── skills/                   # Builtin + learned + community skills
├── assets/                   # Sprites, sounds
└── config/                   # TOML configuration files
```

### Dependency Flow (strict downward)

```
wakey-types          (depends on nothing)
    ↓
wakey-spine          (depends on types)
    ↓
wakey-context        (depends on types, spine)
wakey-senses         (depends on types, spine)
    ↓
wakey-cortex         (depends on types, spine, context, senses)
wakey-skills         (depends on types, spine, context)
    ↓
wakey-action         (depends on types, spine, cortex)
wakey-overlay        (depends on types, spine)
    ↓
wakey-app            (depends on everything, ties it all together)
```

## 3. Core Patterns

### Event-Driven Architecture
- ALL communication between crates goes through the event spine
- Never import one crate's internals into another — use events
- Every event is a typed enum variant in `wakey-types`
- Subsystems subscribe to events they care about, emit events for others

### Trait-Driven Extensibility (from ZeroClaw)
- Major subsystems defined as traits: `Provider`, `Sensor`, `Actor`, `MemoryBackend`, `Skill`
- Implementations are swappable via config
- New functionality = implement a trait + register in factory

### Heartbeat Protocol
- Wakey's heartbeat is NOT a cron/wake-sleep cycle (that's Paperclip's pattern)
- It's continuous consciousness: multiple rhythms running simultaneously
- Tick (2s), Breath (30s), Reflect (15min), Dream (daily)
- Each rhythm emits events that the cortex processes

### Safety-First Actions (from Sondera)
- Every action passes through Cedar policy evaluation BEFORE execution
- Denied actions return feedback to the cortex, which tries a different approach
- Safety is deterministic and auditable, not LLM-based

## 4. Engineering Rules

### Code Style
- Rust 2024 edition
- Use `thiserror` for error types, `anyhow` only in `wakey-app`
- Async runtime: `tokio` (multi-threaded)
- Serialization: `serde` + TOML for config, JSON for events
- No `unwrap()` in library crates — propagate errors
- Minimize allocations in hot paths (tick, event routing)

### Performance Constraints
- Idle RAM target: <20MB
- No background polling — use OS event APIs and async channels
- Screen capture only on-demand (triggered by heartbeat or events)
- LLM calls are expensive — cache aggressively, use local models when possible

### Testing
- Unit tests per crate
- Integration tests in `wakey-app`
- Property tests for event routing (use `proptest`)
- Benchmark heartbeat tick latency (must be <10ms)

### Dependencies
- Crate-specific deps go in that crate's Cargo.toml
- Minimize external dependencies — prefer std library
- No C dependencies unless absolutely necessary (FFI = last resort)
- WASM skills must be sandboxed with fuel metering

## 5. Key Decisions Log

| Decision | Choice | Why |
|---|---|---|
| Language | Rust | 24/7 running, must be ultra-lightweight |
| UI framework | iced or egui | No webview overhead, native GPU rendering |
| Event system | tokio broadcast | Lock-free, async, backpressure support |
| LLM client | OpenAI-compatible only | One HTTP client for all providers (Ollama, OpenRouter, vLLM, GLM, etc.) |
| Memory | OpenViking pattern | Tiered L0/L1/L2 reduces token consumption |
| Safety | Cedar policies | Deterministic, auditable, not prompt-based |
| Skills | Trait + WASM | Traits for core, WASM sandbox for community |
| Learning | Hermes pattern | Auto-skill extraction from experience |
| Screen understanding | Tiered (a11y→OCR→VLM) | Minimizes cost, escalates only when needed |
| Action execution | Platform-native | xdotool/Win32/CoreGraphics for lowest overhead |
