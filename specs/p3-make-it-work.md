# P3: Make It Actually Work — Integration + Testing

## Goal
Wire everything end-to-end so Wakey is actually usable. Fix all broken connections.

## Fixes needed

### Fix 1: Wakey should talk on startup
Current: decision loop waits for 5 unique window changes before speaking
Problem: Wakey overlay window keeps being the active window, so focus_count barely increments

Fix in wakey-app/src/main.rs:
- On startup (after 5 seconds): send first greeting via ShouldSpeak
- Change decision loop: trigger on TICK count (every 30s = 15 ticks) instead of window changes
- Filter out "wakey" from window detection (ignore own window)
- Add: on first Breath event, ask LLM to introduce itself

### Fix 2: Chat bubble should show LLM responses
Current: ShouldSpeak events emitted but need to verify overlay receives them
Fix: Add logging in overlay spine handler to confirm ShouldSpeak events arrive and bubble shows text

### Fix 3: Memory should persist
Current: SqliteMemory initialized but nothing stored
Fix in decision loop:
- After each LLM response: store conversation in memory (category: "conversation")
- On Reflect event: recall recent memories and summarize
- On startup: recall last session summary and mention it ("Welcome back! Last time you were working on...")

### Fix 4: Skill registry should load builtin skills
Current: skills directory scanned but agent-supervisor SKILL.md may not be found
Fix:
- Ensure skills/builtin/agent-supervisor/ path is scanned
- Log which skills are loaded on startup
- Test: skill_registry.find("stuck agent") should return agent-supervisor

### Fix 5: Agent supervisor should watch GSD
Current: code exists but not started
Fix in wakey-app/src/main.rs:
- On startup: if .gsd/ directory exists, start AgentWatcher
- AgentWatcher monitors .gsd/STATE.md for changes
- When state changes: emit AgentProgress event
- When stuck detected: emit AgentStuck → Reporter emits ShouldSpeak
- Test: start GSD headless, watch Wakey report progress

### Fix 6: Filter own window from detection
Current: xdotool returns "wakey" as active window when overlay has focus
Fix in wakey-senses/src/window.rs or wakey-cortex/src/heartbeat.rs:
- If active window title is "Wakey" or app is "wakey", skip emission
- Only emit WindowFocusChanged for non-Wakey windows

## Read first
- crates/wakey-app/src/main.rs (current wiring)
- crates/wakey-overlay/src/window.rs (spine handler, bubble display)
- crates/wakey-cortex/src/decision.rs (decision loop)
- crates/wakey-skills/src/agent_supervisor/ (supervisor code)

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app
# Expected:
# 1. Overlay appears
# 2. Within 5 seconds: Wakey says hello in chat bubble
# 3. Every ~30 seconds: Wakey comments on what you're doing
# 4. Memory persists — restart and Wakey remembers
# 5. If .gsd/ exists: Wakey reports GSD status
```

## Acceptance criteria
- Wakey greets you on startup
- Wakey speaks every ~30s about your active window (not its own window)
- Chat bubble actually shows text
- Memory persists across restarts
- Agent supervisor starts if .gsd/ exists
- No crash, no infinite loops, no panic
- cargo check passes
