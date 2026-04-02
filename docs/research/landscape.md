# Competitive Landscape & Research

## Projects Studied

### OpenClaw (TypeScript, ~247K stars)
- **What**: Multi-platform AI agent gateway (WhatsApp, Telegram, Slack, Discord, etc.)
- **Architecture**: pnpm monorepo, plugin system with SKILL.md folders, ClawHub marketplace
- **Takeaway**: Skill ecosystem pattern, community marketplace model
- **Limitation**: Heavy (Node.js), not a companion, not alive

### ZeroClaw (Rust, ~17K stars)
- **What**: Ultra-lightweight AI agent runtime, single binary
- **Architecture**: Trait-driven (`Provider`, `Channel`, `Tool`, `Memory`, `RuntimeAdapter`), <5MB RAM
- **Takeaway**: Rust trait-driven design, zero-overhead philosophy, memory backends
- **Limitation**: CLI agent, no eyes, no face, no desktop presence

### Hermes Agent (Python, ~22.5K stars, by Nous Research)
- **What**: Self-improving AI agent with learning loop
- **Architecture**: Three-tier (interface → core → execution), 40+ tools, SQLite FTS5 memory
- **Takeaway**: Self-improving skill loop (experience → skill → refinement), user modeling via Honcho, RL trajectory generation
- **Limitation**: Python (heavy), server-side, no companion personality

### Paperclip (Python)
- **What**: Agent orchestration platform for autonomous AI companies
- **Architecture**: Heartbeat protocol (agents wake for short execution windows, do work, sleep)
- **Takeaway**: Heartbeat concept (upgraded for Wakey: continuous, not wake/sleep)
- **Limitation**: Heartbeat = cron job, not consciousness

### Sondera Harness (Python)
- **What**: Deterministic guardrails for AI agents using Cedar policy language
- **Architecture**: Policy evaluation before every action, steering pattern (denied → feedback → retry)
- **Takeaway**: Cedar policy engine for action safety
- **Limitation**: Just a guardrail library

### OpenViking (Python, by Volcengine/ByteDance)
- **What**: Context database for AI agents using filesystem paradigm
- **Architecture**: `viking://` URIs, tiered loading (L0 abstract/L1 overview/L2 detail)
- **Takeaway**: Tiered memory reduces token consumption, semantic search, auto session management
- **Limitation**: Just a database

### OpenFang (Rust)
- **What**: Agent Operating System, 14-crate Cargo workspace
- **Architecture**: Multi-crate with strict downward deps, WASM sandboxed skills, 16 security layers
- **Takeaway**: Multi-crate Rust workspace pattern, WASM skill sandbox
- **Limitation**: Server-side agent OS, no companion

### Computer Use (Claude, Agent-S, Gemini, Open Computer Use)
- **Claude**: Screenshot → vision LLM → pixel coords → action. Pure VLM, no OCR.
- **Agent-S**: UI-TARS grounding model + OCR (72.6% OSWorld). Most accurate.
- **Gemini**: Normalized 1000x1000 coords, safety service layer.
- **Open Computer Use**: Multi-agent (browser/terminal/desktop agents). 82% OSWorld.
- **Takeaway**: Tiered approach — a11y (free) → OCR (cheap) → VLM (expensive)

### Tabbie (Hardware + Software)
- **What**: Physical desk robot with AMOLED display, mounts on monitor
- **Features**: Tasks, Pomodoro, habits, activity insights, 50+ animations
- **Takeaway**: Companion personality, visual presence, emotional reactions
- **Limitation**: Hardware-dependent, limited AI, $70-90

### Project AIRI (TypeScript/Rust/Python, ~1.3K stars)
- **What**: Self-hosted AI virtual companion with Live2D/VRM models
- **Architecture**: pnpm + Turbo monorepo, Electron/Tauri/Capacitor targets
- **Takeaway**: Live2D companion rendering, multi-platform targets
- **Limitation**: Heavy (multiple runtimes), complex stack
