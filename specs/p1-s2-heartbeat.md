# P1-S2: Heartbeat Tick + Active Window

## Goal
Emit WakeyEvent::Tick every 2s with active window info. Also emit SystemVitals.

## Crate
wakey-cortex (src/heartbeat.rs) + wakey-senses (src/window.rs, src/system.rs)

## What to implement

### wakey-senses/src/window.rs
1. `pub fn get_active_window() -> Option<(String, String)>` — returns (app_name, window_title)
2. On Linux: shell out to `xdotool getactivewindow getwindowname` or use x11rb crate
3. Keep it simple — xdotool first, can optimize later

### wakey-senses/src/system.rs
1. `pub fn get_system_vitals() -> SystemVitals` — battery %, CPU %, RAM MB
2. Read from /proc/stat, /proc/meminfo, /sys/class/power_supply/
3. Keep it lightweight — no heavy crate, just file reads

### wakey-cortex/src/heartbeat.rs
1. `pub struct HeartbeatRunner { spine, config }`
2. `pub async fn run(&self, shutdown: broadcast::Receiver<WakeyEvent>)`
3. tokio::select! loop: sleep(tick_interval) → get_active_window → emit WindowFocusChanged + SystemVitals
4. Respect WakeyEvent::Shutdown for clean cancellation
5. Must be <10ms per tick (no LLM calls in tick)

## Read first
- crates/wakey-cortex/AGENTS.md (heartbeat rules)
- crates/wakey-senses/AGENTS.md
- docs/architecture/DECISIONS.md #5 (heartbeat section)

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app  # should print tick events to console
```

## Acceptance criteria
- Tick events emitted every 2s
- Active window app name + title captured correctly on Linux
- SystemVitals with CPU/RAM populated
- Graceful shutdown on Ctrl+C
- cargo check passes
