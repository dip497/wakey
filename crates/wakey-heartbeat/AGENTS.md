# wakey-heartbeat — Agent Instructions

This is the **consciousness cycle**. Wakey's heartbeat is NOT a cron job — it's continuous awareness.

## Rhythms
- **Tick** (2s): Active window name, system vitals. MUST be <10ms.
- **Breath** (30s): Screenshot + understanding. Can take up to 2s.
- **Reflect** (15min): Summarize recent activity. LLM call OK.
- **Dream** (daily at configured hour): Memory compression. Heavy work OK.

## Rules
- Each rhythm is a separate tokio task
- All rhythms emit events through the spine
- Must respect `WakeyEvent::Shutdown` — cancel all tasks gracefully
- Tick must NEVER call an LLM — it must be pure local computation
- Breath may call VLM (async, non-blocking to tick)
- Use `tokio::select!` for clean cancellation
