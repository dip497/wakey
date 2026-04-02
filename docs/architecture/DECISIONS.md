# Architecture Decisions — Based on Deep Code Research

Every decision here is backed by actual code from proven projects. No assumptions.

## 1. Agent Loop → ZeroClaw Pattern

**Source**: `zeroclaw/src/agent/loop_.rs` (9000+ lines, battle-tested)

```
for iteration in 0..max_iterations {
    1. Cancellation check
    2. Budget check (shared atomic counter for subagents)
    3. Preemptive context trim (fast_trim_tool_results → prune_history)
    4. Tool filtering per turn
    5. Vision provider routing (if images present)
    6. LLM call (streaming or non-streaming)
    7. Parse tool calls (native JSON or XML fallback)
    8. If no tools → return final text
    9. Execute tools (parallel when safe)
   10. Loop detection (Warning → Block → Break)
   11. Credential scrubbing (regex, preserve first 4 chars)
}
```

**For Wakey**: Adopt this loop exactly. Add:
- Heartbeat event as trigger (not just user message)
- Cedar policy check at step 9 (before tool execution)
- Spine event emission after each step

## 2. Memory → ZeroClaw Trait + OpenViking Storage

**ZeroClaw Memory Trait** (`src/memory/traits.rs`):
```rust
trait Memory: Send + Sync {
    async fn store(key, content, category, session_id)
    async fn recall(query, limit, session_id, since, until) → Vec<MemoryEntry>
    async fn get(key) → Option<MemoryEntry>
    async fn forget(key) → bool
    async fn export(filter) → Vec<MemoryEntry>  // GDPR
}
```

Categories: Core, Daily, Conversation, Custom(String)
Hybrid search: vector_weight=0.7, keyword_weight=0.3
SQLite schema with FTS5 + embeddings BLOB

**OpenViking Storage** (`openviking/storage/viking_fs.py`):
```
viking://{scope}/{space}/{path}
Scopes: user, agent, session, resources, temp
Tiers: L0 (.abstract.md ~256 chars), L1 (.overview.md ~2000 chars), L2 (full)
L0/L1 auto-generated bottom-up by SemanticProcessor
L2 loaded on-demand only
Token savings: 99%+ for exploration
```

**For Wakey**: ZeroClaw's Memory trait as the Rust interface. OpenViking's filesystem paradigm for storage layout. SQLite for persistence (no external DB). L0/L1/L2 tiers for token efficiency.

## 3. Skills → Hermes Format + OpenViking Storage + petgraph DAG

**Hermes skill_manage** (`tools/skill_manager_tool.py`):
- CRUD: create/patch/edit/delete/write_file/remove_file
- Auto-creation triggers: 5+ tool calls succeeded, errors overcome, user correction
- Nudge system: every N iterations, background review thread checks if skill should be created
- Security scan on every write
- Fuzzy matching for patches

**Hermes Learning Loop** (`run_agent.py`):
```
_iters_since_skill counter → increments on each tool call
After N iterations (default 10) → trigger _should_review_skills
Background thread: _spawn_background_review()
  → Sends conversation snapshot to sub-agent
  → Sub-agent decides: create/update skill or "nothing to save"
  → Non-blocking, best-effort
```

**OpenViking Skill Storage**:
```
viking://agent/skills/{skill-name}/
├── .abstract.md     (L0: quick scan)
├── .overview.md     (L1: when to use)
└── SKILL.md         (L2: full instructions)

viking://agent/memories/skills/{skill-name}.md
  → Execution stats: total_executions, success_count, fail_count
  → Best practices, failure modes, guidelines
```

**For Wakey**: Hermes format (SKILL.md + YAML frontmatter). Hermes creation triggers + background review. OpenViking storage layout with tiers. petgraph for dependency resolution. Cedar for pre-execution safety check.

## 4. Session Management → OpenClaw Pattern

**OpenClaw Sessions** (`src/sessions/session-key-utils.ts`):
```
Key format: agent:{agentId}:{channel}:{kind}:{id}
Types: direct, group, channel, unknown
Persistence: execution-scoped, project-scoped, or custom path
Memory: session: (clean slate) vs resume: (load previous)
```

**For Wakey**: Sessions are continuous (always-on). Working memory persists per heartbeat cycle. Reflect (15min) compacts to short-term. Dream (daily) compacts to long-term. No clean slate — Wakey always remembers.

## 5. Heartbeat → OpenClaw Timer + ZeroClaw Cron + Our Multi-Rhythm

**OpenClaw** (`docs/gateway/heartbeat.md`):
```json5
heartbeat: {
  every: "30m",
  lightContext: false,
  isolatedSession: false,
  prompt: "Read HEARTBEAT.md...",
}
```

**ZeroClaw** (`docs/sop/connectivity.md`):
```toml
[[triggers]]
type = "cron"
expression = "0 0 8 * * *"
```
Window-based check, at-most-once per tick.

**For Wakey**: Multi-rhythm (not single frequency):
- Tick (2s): Local only, no LLM. Active window + vitals.
- Breath (30s): May call VLM. Screen understanding.
- Reflect (15min): LLM call. Summarize, compact memory.
- Dream (daily): Heavy. Pattern learning, memory compression.

Each rhythm = a cron-like trigger that emits spine events. Cortex decides what to do with each event.

## 6. Security → ZeroClaw can_act() + Sondera Cedar

**ZeroClaw** (`src/agent/loop_.rs`):
```rust
SecurityPolicy::can_act() // Called BEFORE every tool execution
// Three autonomy levels: full, semi, ask
// Per-sender rate limiting
// Command risk scoring
```

**Sondera Cedar**:
```cedar
forbid(principal, action, resource)
when { context.command like "*rm -rf*" };
```

**For Wakey**: ZeroClaw's SecurityPolicy trait as the interface. Cedar for policy files (declarative, auditable). Check before EVERY action, not just terminal commands.

## 7. Inter-Crate Communication → OpenFang Kernel Pattern

**OpenFang** (`crates/openfang-kernel/`):
- Kernel assembles all subsystems
- Types crate at the bottom (depends on nothing)
- Runtime handles agent loop + tools + WASM
- Kernel wires runtime + memory + channels + skills
- API layer on top for external access

**For Wakey**: Same pattern but with event spine instead of direct kernel wiring. Types → Spine → Subsystems → App.

## 8. WASM Skills Sandbox → OpenFang Wasmtime

**OpenFang** (`crates/openfang-runtime/src/sandbox.rs`):
- Uses wasmtime v41
- Fuel metering (prevent infinite loops)
- Capability-based security (skills declare what they need)
- Host functions exposed to WASM guests

**For Wakey**: Adopt wasmtime for community skills. Built-in skills can be native Rust (no sandbox overhead). Only community/third-party skills run in WASM sandbox.

## 9. Context Assembly → ZeroClaw Pre-Turn Pipeline

```
Before each LLM call:
1. memory.recall(query) → inject relevant past context
2. Build system prompt (persona + skills + instructions)
3. Append recent history (with auto-compaction if too long)
4. Add current turn (user message or heartbeat event)
5. Estimate tokens → trim if over budget
6. Send to LLM
```

**For Wakey**: Same pipeline. Add screen context from senses (a11y data, OCR text) as part of step 4.

## 10. Desktop Overlay → OpenFang Tauri (reference only)

**OpenFang** uses Tauri 2.0 for desktop. But Tauri adds a webview.

**For Wakey**: Skip Tauri. Use egui/eframe directly for <20MB target. Reference OpenFang's IPC pattern if we ever need frontend-backend separation.

---

## Summary: What We Adopt From Each

| From | What | How |
|---|---|---|
| ZeroClaw | Agent loop, Memory trait, Security, Context assembly | Translate Rust patterns directly |
| OpenClaw | Session management, Heartbeat config, Gateway routing | Adapt for continuous (not message-triggered) |
| OpenViking | Filesystem storage, L0/L1/L2 tiers, Self-evolution | Implement in Rust with SQLite backend |
| Hermes | Skill format, Creation triggers, Learning loop, Background review | Port Python patterns to Rust |
| OpenFang | Multi-crate workspace, WASM sandbox, Dependency graph | Direct adoption (already Rust) |
| Sondera | Cedar policy engine | Integrate cedar-policy crate |
