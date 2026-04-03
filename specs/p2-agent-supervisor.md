# P2: Agent Supervisor Skill

## Goal
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

## Architecture (as a Wakey skill)

### Perception: How Wakey Watches Agents

Based on Composio's pattern — read agent logs directly, don't trust self-reporting:

1. **GSD sessions**: Read `.gsd/STATE.md` + `.gsd/runtime/` logs + `gsd headless query`
2. **Claude Code sessions**: Read JSONL event files from `~/.claude/` 
3. **Terminal sessions**: Watch terminal output via pty or log files
4. **Generic**: Watch any log file for patterns (errors, stuck loops, completion)

### Detection: How Wakey Knows Something Is Wrong

Based on Composio + ZeroClaw patterns:

1. **No activity timeout**: Agent hasn't produced output in N minutes → stuck
2. **Error pattern matching**: Regex patterns for common errors (compile errors, missing deps, auth failures, rate limits)
3. **Loop detection**: Same tool called with same args multiple times (ZeroClaw's Warning → Block → Break)
4. **State regression**: Agent's STATE.md hasn't progressed (like GSD stuck on "evaluating-gates")
5. **CI failure**: PR checks failing after agent pushed

### Response: What Wakey Does (Two Tiers)

**Tier 1 — Silent auto-fix** (no human needed):
- Missing dependency → install it
- Format/lint error → run cargo fmt/clippy
- Rate limit → wait and retry
- Known stuck pattern → restart with context

**Tier 2 — Tell the human** (judgment needed):
- Architectural question
- Merge conflict that can't auto-resolve  
- Agent keeps failing on same issue after 3 retries
- Task seems fundamentally wrong
- Agent requests user input

### Communication: How Wakey Tells You

Not a dashboard. Not Slack. Through the overlay:
- Chat bubble: "Your GSD worker hit a compile error in wakey-cortex. Want me to fix it?"
- Voice: Same message spoken via TTS
- Sprite animation: concerned expression while monitoring, happy when agent succeeds

## Implementation

### skill format (SKILL.md in skills/builtin/agent-supervisor/)

```yaml
---
name: agent-supervisor
description: Monitor and manage AI coding agents running in terminals or worktrees
version: 1.0.0
---
```

### Core Components (all in wakey-skills as a built-in skill)

1. **AgentWatcher** — subscribes to filesystem events + polls log files
   - Watches: .gsd/STATE.md, .gsd/runtime/, terminal output logs
   - Emits spine events: AgentStatus, AgentError, AgentStuck, AgentDone

2. **StuckDetector** — analyzes agent state over time
   - Sliding window of last N state snapshots
   - Compares: is state progressing? are errors repeating?
   - Uses ZeroClaw's loop detection pattern (same action → warn → block → break)

3. **AutoFixer** — handles Tier 1 fixes
   - Pattern-matched responses for known errors
   - Can: restart GSD, inject context, run commands, modify config
   - Cedar policy check before any action

4. **Reporter** — handles Tier 2 notifications
   - Emits ShouldSpeak events through spine
   - Includes: what failed, why, what options the user has
   - Conversational, not technical — "Your worker is stuck" not "Process 12345 SIGTERM"

### Spine Events (add to wakey-types/event.rs)

```rust
// Agent supervision events
AgentSpawned { agent_type: String, task: String, worktree: Option<String> },
AgentProgress { agent_type: String, phase: String, detail: String },
AgentStuck { agent_type: String, reason: String, duration_secs: u64 },
AgentError { agent_type: String, error: String, auto_fixable: bool },
AgentFixed { agent_type: String, fix: String },
AgentCompleted { agent_type: String, summary: String },
AgentFailed { agent_type: String, reason: String },
```

## Dependencies
- Filesystem watcher (notify crate) for log file changes
- GSD CLI (`gsd headless query`) for state inspection
- Existing LLM client for understanding errors
- Existing safety system for action gating

## Read first
- docs/research/zeroclaw-deep.md (loop detection, tool execution)
- docs/architecture/DECISIONS.md (agent loop, security)
- https://github.com/ComposioHQ/agent-orchestrator/blob/main/artifacts/architecture-design.md

## Acceptance criteria
- Wakey detects GSD stuck on gates (like our real issue yesterday)
- Wakey auto-fixes known issues (restart, inject context)
- Wakey tells user about unknown issues via chat bubble
- Works with GSD headless sessions
- Cedar policy check before any auto-fix action
- cargo check passes
