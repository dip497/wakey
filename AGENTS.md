# Wakey — Agent Instructions

> GSD and all AI agents working on this project MUST read this file first.

## What Is Wakey?

Wakey is your laptop, alive. An open-source AI companion that lives as an always-on-top desktop overlay. It perceives your screen, talks proactively, controls the machine when asked, remembers everything, and grows with you.

**Not a tool. Not a bot. A living presence.**

## Architecture (Non-Negotiable)

- **Language**: Rust 2024 edition. No Python. No TypeScript. No exceptions.
- **Binary**: Single binary. Must compile to one executable.
- **Memory target**: <20MB idle RAM. Every MB must be justified.
- **LLM client**: OpenAI-compatible HTTP only. No vendor SDKs. `POST /v1/chat/completions` is the only endpoint we need.
- **Workspace**: 9-crate Cargo workspace. See Cargo.toml for members.
- **Event system**: All inter-crate communication through the Event Spine (`wakey-spine`, tokio broadcast channels). Never import one crate's internals into another.
- **Dependency flow**: Strict downward. See CLAUDE.md for the full graph.

## Code Rules

### DO
- Use `thiserror` for error types in library crates
- Use `anyhow` only in `wakey-app` (the binary)
- Propagate errors with `?` — never `unwrap()` in library crates
- Use `tracing` for logging (not `println!` or `log`)
- Use `serde` + TOML for config, JSON for events
- Write doc comments on public traits and their methods
- Keep functions short — if it scrolls, split it
- Use `Arc<dyn Trait>` for runtime polymorphism (provider swapping)

### DON'T
- Don't add C/C++ FFI dependencies unless absolutely unavoidable
- Don't use `unsafe` unless there is no safe alternative and you document why
- Don't pull in heavy frameworks (no `axum`, no `actix` — this is a desktop app)
- Don't add features "just in case" — build what's needed now
- Don't create new crates — the 9 crates are defined. Put code in the right one.
- Don't import between peer crates directly — communicate through the spine
- Don't use `tokio::spawn` without a cancellation mechanism (respect Shutdown events)

## Naming Conventions

```
Crates:       wakey-{name}     (kebab-case)
Modules:      snake_case.rs
Types:        PascalCase
Functions:    snake_case
Constants:    SCREAMING_SNAKE
Events:       WakeyEvent::{PascalCase}
Config keys:  snake_case in TOML
```

## Testing

- Unit tests in each crate: `#[cfg(test)] mod tests`
- Integration tests in `wakey-app/tests/`
- Test names: `test_{what}_{scenario}_{expected}` (e.g., `test_tick_emits_event_every_2s`)
- Use `tokio::test` for async tests
- Mock the spine for unit tests (create a test Spine, subscribe, assert events)

## Git Conventions

- Branch names: `feat/{slice-name}`, `fix/{description}`, `research/{topic}`
- Commit messages: imperative mood, <72 chars first line
- One commit per task (squash if needed before merge)
- Always run `cargo check && cargo clippy` before committing

## Key Files — READ BEFORE CODING

| File | Purpose | When to Read |
|---|---|---|
| `PROJECT.md` | Vision, differentiation, performance targets | Always |
| `CLAUDE.md` | Architecture, dependency graph, decisions log | Always |
| `AGENTS.md` | This file — rules for all AI agents | Always (auto-loaded) |
| `RUNTIME.md` | Build commands, dependencies, environment | Before running/building |
| `docs/CODING_STANDARDS.md` | Code style, patterns, anti-patterns with examples | Before writing any code |
| `TASKS.md` | Current implementation tasks | Before starting work |
| `config/default.toml` | Default configuration | When touching config |
| `policies/default.cedar` | Safety policies for action execution | When touching action crate |
| `specs/` | Spec files for GSD milestones | When starting a milestone |
| `docs/research/` | Research reports on ZeroClaw, Hermes, OpenFang | Before implementing a subsystem |
| `docs/architecture/` | Architecture documentation | For reference |
| `.rustfmt.toml` | Formatting rules | Auto-applied by `cargo fmt` |
| `clippy.toml` | Lint thresholds | Auto-applied by `cargo clippy` |
| `deny.toml` | Banned dependencies, license checks | Auto-applied by `cargo deny` |

## Per-Crate Instructions

Each critical crate has its own `AGENTS.md` with specific rules. GSD auto-reads these when working inside a crate directory. Currently:
- `crates/wakey-types/AGENTS.md` — Foundation types, events, config, errors
- `crates/wakey-spine/AGENTS.md` — Event bus rules
- `crates/wakey-context/AGENTS.md` — Memory + skills storage (OpenViking L0/L1/L2 + ZeroClaw Memory trait)
- `crates/wakey-senses/AGENTS.md` — Perception (a11y, screen, clipboard, system, git)
- `crates/wakey-cortex/AGENTS.md` — Brain (ZeroClaw agent loop + LLM client + heartbeat rhythms + decisions)
- `crates/wakey-action/AGENTS.md` — Hands + safety (input, terminal, browser + Cedar policies)
- `crates/wakey-skills/AGENTS.md` — Skills runtime (Hermes format + petgraph DAG + WASM sandbox + learning loop)
- `crates/wakey-overlay/AGENTS.md` — Face (egui, sprites, chat bubbles)
- `crates/wakey-app/AGENTS.md` — Binary entry point

## Reference Repos (Read Actual Code, Not Summaries)

When implementing a subsystem, read the ACTUAL source code from these local repos — not just our research summaries in `docs/research/`.

| Pattern Needed | Read From | Key Files |
|---|---|---|
| Trait system, LLM provider | `/home/dipendra-sharma/projects/zeroclaw/` | Look for `trait Provider`, `trait Channel`, `trait Tool` |
| Multi-crate workspace | `/home/dipendra-sharma/projects/openfang/` | Look at `Cargo.toml`, crate boundaries |
| WASM skills sandbox | `/home/dipendra-sharma/projects/openfang/` | Look for wasmtime/wasmer usage |

The `docs/research/*.md` files are quick summaries for human reference. For implementation, always read the real code.

## When In Doubt

1. Read PROJECT.md for the vision
2. Read CLAUDE.md for the architecture
3. Read actual source from reference repos (not just summaries)
4. Check if the feature passes the Five Gates (see skills/wakey-vision-guard/)
5. Ask: "Does this make Wakey more alive, or more like a tool?"
6. If still unsure, write the simplest version that compiles and revisit later
