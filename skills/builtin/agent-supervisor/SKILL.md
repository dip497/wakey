---
name: agent-supervisor
description: "Monitor and manage AI coding agents running in terminals or worktrees. Detects when agents get stuck, auto-fixes known issues, and notifies user when intervention needed. Triggers when GSD sessions are running, agents are spawned, or monitoring is requested."
version: 1.0.0
tags: [agent, monitoring, supervision, gsd, auto-fix]
platforms: [linux]
dependencies: []
---

# Agent Supervisor

Wakey monitors your AI coding agents (GSD, Claude Code, Codex, OpenCode), detects when they're stuck, and either fixes them automatically or tells you what's wrong.

## The Experience

```
You: "Wakey, build me a REST API"
Wakey: spawns GSD in a worktree
  → GSD gets stuck on a dependency error
  → Wakey notices (reads GSD output logs)
  → Wakey: "Your GSD worker hit a missing crate error. I'm fixing it..."
  → Wakey injects the fix
  → GSD continues
  → GSD finishes
  → Wakey: "API is done! 4 endpoints, tests passing."
You: didn't touch anything
```

## Core Philosophy

**Push, not pull.** Based on Composio's pattern — the user never polls a dashboard. Wakey pushes notifications exactly when intervention is needed, and stays silent otherwise.

### Two-Tier Handling

**Tier 1 — Silent auto-fix** (no human needed):
- Missing dependency → install it
- Format/lint error → run cargo fmt/clippy
- Rate limit → wait and retry
- Known stuck pattern → restart with context
- CI failure → send fix to agent

**Tier 2 — Tell the human** (judgment needed):
- Architectural question
- Merge conflict that can't auto-resolve
- Agent keeps failing on same issue after 3 retries
- Task seems fundamentally wrong
- Agent requests user input

## When to Use

- GSD session is running in a worktree
- Agent is spawned for a task
- User asks Wakey to "watch" or "monitor" an agent
- User says "build me X" and Wakey spawns an agent
- Any terminal session with AI agent activity detected

## Components

### 1. AgentWatcher

Subscribes to filesystem events and polls log files:

- Watches `.gsd/STATE.md` for state transitions
- Watches `.gsd/runtime/` for execution logs
- Monitors terminal output via pty or log files
- Emits spine events: AgentStatus, AgentError, AgentStuck, AgentDone

Uses `notify` crate for filesystem events with debouncing.

### 2. StuckDetector

Analyzes agent state over time using ZeroClaw's loop detection pattern:

- Sliding window of last N state snapshots (default: 10)
- Compares: is state progressing? are errors repeating?
- Loop detection patterns:
  - **Exact repeat**: Same tool + args 3+ times → Warning → Block → Break
  - **Ping-pong**: A→B→A→B for 4+ cycles → Warning
  - **No progress**: Same tool, different args, same result 5+ times → Warning
  - **Timeout**: No output for N minutes → Stuck
- State regression: STATE.md stuck on same phase (e.g., "evaluating-gates")

Detection thresholds:
- Warning: inject nudge after 2 repeats
- Block: replace output after 4 repeats
- Break: terminate turn after 6+ repeats

### 3. AutoFixer

Handles Tier 1 fixes with Cedar policy check before action:

Pattern-matched responses:

| Pattern | Detection Regex | Auto-Fix Action |
|---------|----------------|-----------------|
| Missing crate | `error\[E0433\]: unresolved import` or `cannot find crate` | Run cargo add, restart agent |
| Compile error | `error\[E0\d+\]` | Inject error context, restart |
| Format issue | `cargo fmt --check failed` | Run cargo fmt |
| Lint issue | `cargo clippy -- -D warnings failed` | Run cargo clippy --fix |
| Rate limit | `429 Too Many Requests` or `rate limit exceeded` | Wait 60s, retry |
| Auth failure | `401 Unauthorized` or `invalid API key` | Check env vars via secure_env_collect |
| CI failure | `CI checks failed` or `test failed` | Send fix prompt to agent |
| Gates stuck | STATE.md stuck on `evaluating-gates` > 2min | Restart agent with context injection |

Each fix requires Cedar policy approval before execution.

### 4. Reporter

Handles Tier 2 notifications via spine events:

Emits `ShouldSpeak` events with:
- What failed (conversational, not technical)
- Why it matters
- What options the user has

Example messages:
- "Your worker got stuck on a compile error. Want me to try fixing it?"
- "GSD keeps hitting the same gate. I've restarted it twice. Your call."
- "The agent needs your input on which database to use."

Not a dashboard — through the overlay's chat bubble or voice.

## GSD Integration (MVP)

For the MVP, we focus on GSD headless sessions:

### State File Monitoring

Read `.gsd/STATE.md` for:
- Current phase: planning, executing, evaluating-gates, completing
- Active milestone/slice/task
- Last update timestamp

### Runtime Log Monitoring

Read `.gsd/runtime/` JSONL logs for:
- Tool calls and results
- Errors and warnings
- Time gaps between actions

### Query Interface

Use `gsd headless query` (if available) for:
- Current task status
- Active agent process
- Blocker details

## Detection Heuristics

### No Activity Timeout

If `STATE.md` hasn't been modified in N minutes (default: 5), check:
- Is the agent process alive?
- Are there recent log entries?
- If both stale → AgentStuck event

### Error Pattern Matching

Scan recent logs for error patterns:
- Compile errors → AgentError { auto_fixable: true }
- Auth failures → AgentError { auto_fixable: true }
- Unknown errors → AgentError { auto_fixable: false }

### Loop Detection

Track last 10 tool call signatures:
- `(tool_name, args_hash)`
- If 3+ consecutive identical → Warning
- If 5+ consecutive identical → Block
- If 7+ or escalation → Break

### State Regression

Track STATE.md phase:
- If phase unchanged for > 2min while agent active → Warning
- If > 5min → Stuck event

## Fix Execution Flow

```
1. Detect issue → emit AgentError/AgentStuck
2. Classify: Tier 1 (auto-fixable) or Tier 2 (needs user)
3. If Tier 1:
   a. Identify fix pattern
   b. Cedar policy check: can_act()?
   c. If approved → execute fix
   d. Emit AgentFixed event
   e. Monitor for recovery (30s window)
   f. If not recovered → escalate to Tier 2
4. If Tier 2:
   a. Emit ShouldSpeak with options
   b. Wait for user response
   c. Execute user's choice
```

## Verification

After each fix:
- Wait 30 seconds
- Check if agent has new activity
- Check if STATE.md has progressed
- If yes → emit AgentProgress
- If no → escalate to Tier 2 after 3 retries

## Pitfalls

- **False positives**: Don't flag long-running operations (compiles, tests) as stuck
- **Restart loops**: Don't restart more than 3 times without user approval
- **Cascading failures**: Don't auto-fix if previous fix failed
- **State file race**: Debounce filesystem events (500ms)
- **Permission issues**: Always check Cedar before action
- **Process detection**: Agent might be child of terminal, check full process tree

## Example Scenarios

### Scenario 1: Missing Crate

```
Detection: error[E0433]: cannot find crate `serde_json`
Classification: Tier 1
Fix: cargo add serde_json
Policy: Cedar check (medium risk: dependency addition)
Execute: run cargo add, emit AgentFixed
Monitor: wait for recompilation
Result: Agent continues, emit AgentProgress
```

### Scenario 2: Gates Loop

```
Detection: STATE.md stuck on "evaluating-gates" for 3min
           + logs show "Q5: flag" repeated 4 times
Classification: Tier 1 initially
Fix 1: Restart agent with injected context "skip Q5 for now"
Retry: still stuck after 2min
Fix 2: Restart with different context injection
Retry: still stuck
Escalate: Tier 2 after 2 retries
Message: "GSD keeps getting stuck on quality gates. Want me to skip them or should we investigate?"
```

### Scenario 3: Unknown Error

```
Detection: new error pattern not in fix table
Classification: Tier 2 (auto_fixable: false)
Message: "Your agent hit an error I don't recognize: [summary]. Should I restart it?"
```

## Spine Events

See `wakey-types/src/event.rs` for:
- `AgentSpawned`: New agent session started
- `AgentProgress`: Agent made progress (phase change)
- `AgentStuck`: Agent detected as stuck
- `AgentError`: Agent encountered error
- `AgentFixed`: Auto-fix was applied
- `AgentCompleted`: Agent finished successfully
- `AgentFailed`: Agent failed (after retries)

## Configuration

Default thresholds (can be tuned via Wakey config):
- `activity_timeout_secs`: 300 (5 minutes)
- `loop_warning_threshold`: 2
- `loop_block_threshold`: 4
- `loop_break_threshold`: 6
- `max_auto_retries`: 3
- `fix_monitor_window_secs`: 30
- `debounce_ms`: 500

## Dependencies

- `notify` crate: filesystem watching
- `wakey-spine`: event emission
- `wakey-action`: Cedar policy checks (can_act)
- GSD CLI: headless query (optional)

## Future Extensions

- Claude Code sessions: read `~/.claude/` JSONL logs
- Codex sessions: read `.codex/` state
- Generic terminal monitoring: pty capture
- Multi-agent coordination: worktree conflict detection