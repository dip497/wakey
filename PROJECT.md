# Wakey — Your Laptop, Alive

> An open-source AI companion that lives on your screen, sees what you see, talks to you, acts for you, and grows with you.

**Domain**: wakey.to
**License**: MIT
**Language**: Rust (100% core)

---

## Vision

Your laptop has a soul. Wakey is not a tool you open — it's always there. It wakes up with you, watches what you're doing, comments when it matters, stays silent when you're in flow, and learns who you are over weeks and months.

Every other AI agent today is **dead** — it responds when called, then disappears. Wakey is **alive** — it has a heartbeat, a nervous system, eyes, hands, memory, and a personality that evolves.

## What Makes Wakey Different

| Dimension | Everyone Else | Wakey |
|---|---|---|
| Alive? | No (request/response) | Yes (heartbeat + event spine) |
| Has eyes? | Some (computer use) | Yes (tiered vision: a11y → OCR → VLM) |
| Has hands? | Some (computer use) | Yes (action + Cedar safety policies) |
| Has memory? | Basic | Yes (OpenViking L0/L1/L2 tiered) |
| Knows you? | No | Yes (user model, evolves daily) |
| Learns? | Hermes only | Yes (auto-skill creation from experience) |
| Has personality? | Tabbie/AIRI only | Yes (mood, style, evolution) |
| Lightweight? | ZeroClaw only | Yes (Rust, <20MB idle) |
| Safe? | Sondera only | Yes (Cedar deterministic policies) |
| Extensible? | OpenClaw/OpenFang | Yes (WASM + traits + marketplace) |
| Desktop presence? | Tabbie only | Yes (overlay, sprites, voice) |

## Architectural Inspirations

| Source | What We Take |
|---|---|
| **ZeroClaw** | Trait-driven Rust core, single binary, <20MB idle, zero-overhead |
| **OpenClaw** | Skill ecosystem pattern, community marketplace model |
| **Hermes Agent** | Self-improving learning loop, user modeling, skill extraction |
| **Paperclip** | Heartbeat protocol (upgraded to continuous consciousness) |
| **Sondera Harness** | Cedar policy engine for deterministic action safety |
| **OpenViking** | Tiered memory (L0/L1/L2), filesystem paradigm |
| **OpenFang** | Multi-crate Cargo workspace, WASM sandboxed skills |
| **Computer Use** (Claude/Agent-S) | Vision + screen control pipeline |
| **Tabbie/AIRI** | Visual companion presence, personality system |

## Core Systems

### 1. Event Spine
Central nervous system. Every subsystem emits and consumes typed events. Nothing polls. Nothing sleeps. Pure async event-driven architecture.

### 2. Heartbeat (Consciousness Cycles)
- **Tick** (2s): Active window, system vitals, cursor position
- **Breath** (30s): Screenshot → OCR/VLM → "What is user doing?"
- **Reflect** (15min): Summarize activity, update working memory
- **Dream** (daily): Compress memories, learn patterns, evolve personality

### 3. Tiered Vision (Perception)
- **Layer 0** (always-on): OS accessibility APIs — window focus, app name, text
- **Layer 1** (on-change): Screenshot + local OCR — all visible text
- **Layer 2** (periodic): Cloud vision LLM — deep semantic understanding

### 4. Computer Use (Action)
- Platform-native input injection (xdotool/Win32/CoreGraphics)
- Terminal command execution
- File operations, app launching, browser automation
- Grounding: VLM coordinates or accessibility tree
- Cedar policy safety layer — ask before risky actions

### 5. Memory (OpenViking)
- Working memory (current session)
- Short-term (today's context, L0/L1 tier)
- Long-term (user patterns, preferences, L2 tier)
- Episodic (specific remembered moments)
- User model (who is this human, how do they work)

### 6. Learning Loop (from Hermes)
- Experience → extract pattern → create skill
- Reuse → refine skill → get faster
- Over time: LLM calls decrease, speed increases, cost drops to zero for learned tasks

### 7. Personality
- Mood system (adapts to user's emotional state)
- Communication style (learns user preferences)
- Proactive triggers (knows when to speak and when to shut up)
- Evolution (personality weights shift over weeks/months)

### 8. Safety (Cedar Policies)
- Deterministic, auditable action guardrails
- Per-app permission levels
- Steering pattern: denied actions get feedback, agent tries differently
- User trust levels: auto-approve safe actions, confirm risky ones

## Target Performance

| Metric | Target |
|---|---|
| Idle RAM | <20MB |
| Active RAM (during vision spike) | <80MB, drops back |
| Binary size | <15MB |
| Startup time | <500ms |
| Tick latency | <10ms |
| Breath latency | <2s |
