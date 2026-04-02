# Hermes Agent Deep Research

> Implementation analysis of Hermes Agent's learning loop, skill management, memory system, and user modeling. All code snippets are from the actual codebase.

**Repository:** https://github.com/NousResearch/hermes-agent  
**Analyzed:** April 2026

---

## Table of Contents

1. [Skill Management System](#1-skill-management-system)
2. [Learning Loop](#2-learning-loop)
3. [Memory System](#3-memory-system)
4. [User Modeling (Honcho)](#4-user-modeling-honcho)
5. [Prompt Building & Context Assembly](#5-prompt-building--context-assembly)
6. [Context Compression](#6-context-compression)
7. [Key Patterns for Wakey](#7-key-patterns-for-wakey)

---

## 1. Skill Management System

### 1.1 Skill CRUD Implementation

**File:** `tools/skill_manager_tool.py`

Hermes implements a complete skill CRUD system with security scanning:

```python
# skill_manager_tool.py:67-94
def skill_manage(
    action: str,
    name: str,
    content: str = None,
    category: str = None,
    file_path: str = None,
    file_content: str = None,
    old_string: str = None,
    new_string: str = None,
    replace_all: bool = False,
) -> str:
    """
    Manage user-created skills. Dispatches to the appropriate action handler.

    Actions: create, edit, patch, delete, write_file, remove_file.
    """
```

**Actions:**
- `create` - Full SKILL.md + optional category
- `patch` - Find-and-replace (preferred for fixes)
- `edit` - Full SKILL.md rewrite (major overhauls only)
- `delete` - Remove skill entirely
- `write_file` / `remove_file` - Supporting files

### 1.2 When Skills Are Created

The tool schema encodes creation triggers:

```python
# skill_manager_tool.py:467-480
SKILL_MANAGE_SCHEMA = {
    "name": "skill_manage",
    "description": (
        # ...
        "Create when: complex task succeeded (5+ calls), errors overcome, "
        "user-corrected approach worked, non-trivial workflow discovered, "
        "or user asks you to remember a procedure.\n"
        "Update when: instructions stale/wrong, OS-specific failures, "
        "missing steps or pitfalls found during use. "
        "If you used a skill and hit issues not covered by it, patch it immediately.\n"
    ),
}
```

**Key insight:** Skill creation is triggered by:
1. Complex task success (5+ tool calls)
2. Errors overcome through iteration
3. User corrections to agent approach
4. Non-trivial workflow discovery
5. Explicit user request ("remember this")

### 1.3 Skill Directory Structure

```python
# skill_manager_tool.py:29-43
# Directory layout for user skills:
#     ~/.hermes/skills/
#     ├── my-skill/
#     │   ├── SKILL.md
#     │   ├── references/
#     │   ├── templates/
#     │   ├── scripts/
#     │   └── assets/
```

### 1.4 SKILL.md Format

```yaml
# skill_manager_tool.py:136-165 validation
---
name: skill-name              # Required, max 64 chars
description: Brief description # Required, max 1024 chars
version: 1.0.0                # Optional
platforms: [macos, linux]     # Optional — restrict to specific OS
prerequisites:                # Optional
  env_vars: [API_KEY]
  commands: [curl, jq]
---

# Skill Title

Full instructions and content here...
```

### 1.5 Security Scanning

All skills get scanned before write:

```python
# skill_manager_tool.py:39-56
def _security_scan_skill(skill_dir: Path) -> Optional[str]:
    """Scan a skill directory after write. Returns error string if blocked, else None."""
    if not _GUARD_AVAILABLE:
        return None
    try:
        result = scan_skill(skill_dir, source="agent-created")
        allowed, reason = should_allow_install(result)
        if allowed is False:
            report = format_scan_report(result)
            return f"Security scan blocked this skill ({reason}):\n{report}"
```

### 1.6 Skill Patch with Fuzzy Matching

Uses the same fuzzy matching engine as file patch tool:

```python
# skill_manager_tool.py:298-310
from tools.fuzzy_match import fuzzy_find_and_replace

new_content, match_count, match_error = fuzzy_find_and_replace(
    content, old_string, new_string, replace_all
)
if match_error:
    # Show a short preview of the file so the model can self-correct
    preview = content[:500] + ("..." if len(content) > 500 else "")
    return {
        "success": False,
        "error": match_error,
        "file_preview": preview,
    }
```

---

## 2. Learning Loop

### 2.1 Iteration Tracking

Hermes tracks tool iterations to trigger skill review:

```python
# run_agent.py:1055
self._iters_since_skill = 0

# run_agent.py:1144-1147
self._skill_nudge_interval = 10  # Default
# Can be configured via config.yaml skills.creation_nudge_interval
```

### 2.2 Increment on Each Tool Call

```python
# run_agent.py:6973-6976
# Track tool-calling iterations for skill nudge.
# Counter resets whenever skill_manage is actually used.
if (self._skill_nudge_interval > 0
        and "skill_manage" in self.valid_tool_names):
    self._iters_since_skill += 1
```

### 2.3 Reset on Skill Tool Use

```python
# run_agent.py:5947-5948 and 6146-6147
elif function_name == "skill_manage":
    # ...
    self._iters_since_skill = 0  # Reset after successful skill use
```

### 2.4 Trigger Check After Turn Completion

```python
# run_agent.py:8804-8810
# Check skill trigger NOW — based on how many tool iterations THIS turn used.
_should_review_skills = False
if (self._skill_nudge_interval > 0
        and self._iters_since_skill >= self._skill_nudge_interval
        and "skill_manage" in self.valid_tool_names):
    _should_review_skills = True
    self._iters_since_skill = 0
```

### 2.5 Background Review (Non-Blocking)

When triggered, spawns a background thread with a sub-agent:

```python
# run_agent.py:8810-8818
# Background memory/skill review — runs AFTER the response is delivered
# so it never competes with the user's task for model attention.
if final_response and not interrupted and (_should_review_memory or _should_review_skills):
    try:
        self._spawn_background_review(
            messages_snapshot=list(messages),
            review_memory=_should_review_memory,
            review_skills=_should_review_skills,
        )
    except Exception:
        pass  # Background review is best-effort
```

### 2.6 Review Prompts

**Memory Review:**
```python
# run_agent.py:1654-1664
_MEMORY_REVIEW_PROMPT = (
    "Review the conversation above and consider saving to memory if appropriate.\n\n"
    "Focus on:\n"
    "1. Has the user revealed things about themselves — their persona, desires, "
    "preferences, or personal details worth remembering?\n"
    "2. Has the user expressed expectations about how you should behave, their work "
    "style, or ways they want you to operate?\n\n"
    "If something stands out, save it using the memory tool. "
    "If nothing is worth saving, just say 'Nothing to save.' and stop."
)
```

**Skill Review:**
```python
# run_agent.py:1665-1675
_SKILL_REVIEW_PROMPT = (
    "Review the conversation above and consider saving or updating a skill if appropriate.\n\n"
    "Focus on: was a non-trivial approach used to complete a task that required trial "
    "and error, or changing course due to experiential findings along the way, or did "
    "the user expect or desire a different method or outcome?\n\n"
    "If a relevant skill already exists, update it with what you learned. "
    "Otherwise, create a new skill if the approach is reusable.\n"
    "If nothing is worth saving, just say 'Nothing to save.' and stop."
)
```

### 2.7 Sub-Agent Fork for Review

```python
# run_agent.py:1712-1730
def _run_review():
    review_agent = AIAgent(
        model=self.model,
        max_iterations=8,
        quiet_mode=True,
        platform=self.platform,
        provider=self.provider,
    )
    review_agent._memory_store = self._memory_store
    review_agent._memory_enabled = self._memory_enabled
    review_agent._user_profile_enabled = self._user_profile_enabled
    review_agent._memory_nudge_interval = 0
    review_agent._skill_nudge_interval = 0  # Prevent recursive review

    review_agent.run_conversation(
        user_message=prompt,
        conversation_history=messages_snapshot,
    )
```

### 2.8 Action Summary for User

```python
# run_agent.py:1736-1766
# Scan the review agent's messages for successful tool actions
# and surface a compact summary to the user.
actions = []
for msg in getattr(review_agent, "_session_messages", []):
    if not isinstance(msg, dict) or msg.get("role") != "tool":
        continue
    data = json.loads(msg.get("content", "{}"))
    if not data.get("success"):
        continue
    message = data.get("message", "")
    # ... categorize actions ...
    
if actions:
    summary = " · ".join(dict.fromkeys(actions))
    self._safe_print(f"  💾 {summary}")
```

---

## 3. Memory System

### 3.1 File-Backed Storage

Hermes uses simple markdown files with a delimiter:

```python
# tools/memory_tool.py:38-41
MEMORY_DIR = get_hermes_home() / "memories"
ENTRY_DELIMITER = "\n§\n"  # Section sign delimiter
```

**Two stores:**
- `MEMORY.md` - Agent's personal notes (environment, tool quirks, conventions)
- `USER.md` - User profile (preferences, communication style, habits)

### 3.2 Frozen Snapshot Pattern

Memory is loaded once at session start and frozen for prefix caching:

```python
# tools/memory_tool.py:60-68
class MemoryStore:
    def __init__(self, memory_char_limit: int = 2200, user_char_limit: int = 1375):
        self.memory_entries: List[str] = []
        self.user_entries: List[str] = []
        # Frozen snapshot for system prompt -- set once at load_from_disk()
        self._system_prompt_snapshot: Dict[str, str] = {"memory": "", "user": ""}
```

```python
# tools/memory_tool.py:200-211
def format_for_system_prompt(self, target: str) -> Optional[str]:
    """
    Return the frozen snapshot for system prompt injection.

    This returns the state captured at load_from_disk() time, NOT the live
    state. Mid-session writes do not affect this. This keeps the system
    prompt stable across all turns, preserving the prefix cache.
    """
    block = self._system_prompt_snapshot.get(target, "")
    return block if block else None
```

### 3.3 Character Limits

```python
# tools/memory_tool.py:64-65
memory_char_limit: int = 2200
user_char_limit: int = 1375
```

### 3.4 Security Scanning

Memory content is scanned for injection patterns:

```python
# tools/memory_tool.py:40-66
_MEMORY_THREAT_PATTERNS = [
    (r'ignore\s+(previous|all|above|prior)\s+instructions', "prompt_injection"),
    (r'you\s+are\s+now\s+', "role_hijack"),
    (r'do\s+not\s+tell\s+the\s+user', "deception_hide"),
    (r'system\s+prompt\s+override', "sys_prompt_override"),
    # ... more patterns
]

def _scan_memory_content(content: str) -> Optional[str]:
    """Scan memory content for injection/exfil patterns. Returns error string if blocked."""
    for char in _INVISIBLE_CHARS:
        if char in content:
            return f"Blocked: content contains invisible unicode character U+{ord(char):04X}"
    for pattern, pid in _MEMORY_THREAT_PATTERNS:
        if re.search(pattern, content, re.IGNORECASE):
            return f"Blocked: content matches threat pattern '{pid}'"
```

### 3.5 Atomic Write with File Locking

```python
# tools/memory_tool.py:83-93
@staticmethod
@contextmanager
def _file_lock(path: Path):
    """Acquire an exclusive file lock for read-modify-write safety."""
    lock_path = path.with_suffix(path.suffix + ".lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    fd = open(lock_path, "w")
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield
    finally:
        fcntl.flock(fd, fcntl.LOCK_UN)
        fd.close()
```

### 3.6 SQLite FTS5 for Session Search

The session_search tool provides full-text search over past conversations:

```python
# tools/session_search_tool.py:20-27
"""
Session Search Tool - Long-Term Conversation Recall

Searches past session transcripts in SQLite via FTS5, then summarizes the top
matching sessions using a cheap/fast model (same pattern as web_extract).

Flow:
  1. FTS5 search finds matching messages ranked by relevance
  2. Groups by session, takes the top N unique sessions (default 3)
  3. Loads each session's conversation, truncates to ~100k chars centered on matches
  4. Sends to Gemini Flash with a focused summarization prompt
  5. Returns per-session summaries with metadata
"""
```

### 3.7 SQLite Schema

```python
# hermes_state.py:45-87
SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    user_id TEXT,
    model TEXT,
    system_prompt TEXT,
    parent_session_id TEXT,
    started_at REAL NOT NULL,
    ended_at REAL,
    message_count INTEGER DEFAULT 0,
    -- ... token counts, billing, etc.
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT,
    tool_call_id TEXT,
    tool_calls TEXT,
    tool_name TEXT,
    timestamp REAL NOT NULL,
    -- ... reasoning fields
);
"""

FTS_SQL = """
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    content=messages,
    content_rowid=id
);
"""
```

---

## 4. User Modeling (Honcho)

### 4.1 Honcho Client Configuration

```python
# honcho_integration/client.py:71-112
@dataclass
class HonchoClientConfig:
    """Configuration for Honcho client, resolved for a specific host."""
    host: str = HOST
    workspace_id: str = "hermes"
    api_key: str | None = None
    environment: str = "production"
    
    # Identity
    peer_name: str | None = None
    ai_peer: str = "hermes"
    
    # Toggles
    enabled: bool = False
    save_messages: bool = True
    
    # Memory mode: "hybrid" / "honcho"
    memory_mode: str = "hybrid"
    
    # Write frequency: "async" | "turn" | "session" | int
    write_frequency: str | int = "async"
    
    # Recall mode: "hybrid" | "context" | "tools"
    recall_mode: str = "hybrid"
```

### 4.2 Recall Modes

```python
# honcho_integration/client.py:28-32
# Recall mode: how memory retrieval works when Honcho is active.
# "hybrid"  — auto-injected context + Honcho tools available (model decides)
# "context" — auto-injected context only, Honcho tools removed
# "tools"   — Honcho tools only, no auto-injected context
recall_mode: str = "hybrid"
```

### 4.3 Session Management

```python
# honcho_integration/session.py:25-50
@dataclass
class HonchoSession:
    """
    A conversation session backed by Honcho.
    
    Provides a local message cache that syncs to Honcho's
    AI-native memory system for user modeling.
    """
    key: str  # channel:chat_id
    user_peer_id: str  # Honcho peer ID for the user
    assistant_peer_id: str  # Honcho peer ID for the assistant
    honcho_session_id: str  # Honcho session ID
    messages: list[dict[str, Any]] = field(default_factory=list)
```

### 4.4 Peer Representation

```python
# honcho_integration/session.py:565-590
def get_prefetch_context(self, session_key: str, user_message: str | None = None) -> dict[str, str]:
    """
    Pre-fetch user and AI peer context from Honcho.
    
    Returns:
        Dictionary with 'representation', 'card', 'ai_representation',
        and 'ai_card' keys.
    """
    ctx = honcho_session.context(
        summary=False,
        tokens=self._context_tokens,
        peer_target=session.user_peer_id,
        peer_perspective=session.assistant_peer_id,
    )
    card = ctx.peer_card or []
    result["representation"] = ctx.peer_representation or ""
    result["card"] = "\n".join(card) if isinstance(card, list) else str(card)
```

### 4.5 Dialectic Queries

```python
# honcho_integration/session.py:436-470
def dialectic_query(
    self, session_key: str, query: str,
    reasoning_level: str | None = None,
    peer: str = "user",
) -> str:
    """
    Query Honcho's dialectic endpoint about a peer.
    
    Runs an LLM on Honcho's backend against the target peer's full
    representation. Higher latency than context() — call async via
    prefetch_dialectic() to avoid blocking the response.
    """
    result = target_peer.chat(query, reasoning_level=level) or ""
    # Apply Hermes-side char cap before caching
    if result and self._dialectic_max_chars and len(result) > self._dialectic_max_chars:
        result = result[:self._dialectic_max_chars].rsplit(" ", 1)[0] + " …"
    return result
```

### 4.6 Conclusions (User Facts)

```python
# honcho_integration/session.py:604-628
def create_conclusion(self, session_key: str, content: str) -> bool:
    """Write a conclusion about the user back to Honcho.

    Conclusions are facts the AI peer observes about the user —
    preferences, corrections, clarifications, project context.
    They feed into the user's peer card and representation.
    """
    conclusions_scope = assistant_peer.conclusions_of(session.user_peer_id)
    conclusions_scope.create([{
        "content": content.strip(),
        "session_id": session.honcho_session_id,
    }])
```

---

## 5. Prompt Building & Context Assembly

### 5.1 System Prompt Layers

```python
# run_agent.py:2646-2658
def _build_system_prompt(self, system_message: str = None) -> str:
    """
    Assemble the full system prompt from all layers.
    
    Layers (in order):
      1. Agent identity — SOUL.md when available, else DEFAULT_AGENT_IDENTITY
      2. User / gateway system prompt (if provided)
      3. Persistent memory (frozen snapshot)
      4. Skills guidance (if skills tools are loaded)
      5. Context files (AGENTS.md, .cursorrules — SOUL.md excluded here)
      6. Current date & time (frozen at build time)
      7. Platform-specific formatting hint
    """
```

### 5.2 Tool-Aware Guidance Injection

```python
# run_agent.py:2683-2692
# Tool-aware behavioral guidance: only inject when the tools are loaded
tool_guidance = []
if "memory" in self.valid_tool_names:
    tool_guidance.append(MEMORY_GUIDANCE)
if "session_search" in self.valid_tool_names:
    tool_guidance.append(SESSION_SEARCH_GUIDANCE)
if "skill_manage" in self.valid_tool_names:
    tool_guidance.append(SKILLS_GUIDANCE)
if tool_guidance:
    prompt_parts.append(" ".join(tool_guidance))
```

### 5.3 Skills Index in System Prompt

```python
# agent/prompt_builder.py:319-340
def build_skills_system_prompt(
    available_tools: "set[str] | None" = None,
    available_toolsets: "set[str] | None" = None,
) -> str:
    """Build a compact skill index for the system prompt.

    Two-layer cache:
      1. In-process LRU dict keyed by (skills_dir, tools, toolsets)
      2. Disk snapshot (``.skills_prompt_snapshot.json``) validated by
         mtime/size manifest — survives process restarts
    """
```

### 5.4 Skills Snapshot Cache

```python
# agent/prompt_builder.py:167-180
def _load_skills_snapshot(skills_dir: Path) -> Optional[dict]:
    """Load the disk snapshot if it exists and its manifest still matches."""
    snapshot_path = _skills_prompt_snapshot_path()
    if not snapshot_path.exists():
        return None
    snapshot = json.loads(snapshot_path.read_text(encoding="utf-8"))
    if snapshot.get("version") != _SKILLS_SNAPSHOT_VERSION:
        return None
    if snapshot.get("manifest") != _build_skills_manifest(skills_dir):
        return None
    return snapshot
```

### 5.5 Context Files Priority

```python
# agent/prompt_builder.py:449-455
def build_context_files_prompt(cwd: Optional[str] = None, skip_soul: bool = False) -> str:
    """Priority (first found wins — only ONE project context type is loaded):
      1. .hermes.md / HERMES.md  (walk to git root)
      2. AGENTS.md / agents.md   (cwd only)
      3. CLAUDE.md / claude.md   (cwd only)
      4. .cursorrules / .cursor/rules/*.mdc  (cwd only)
    """
```

### 5.6 Context Truncation

```python
# agent/prompt_builder.py:56-62
CONTEXT_FILE_MAX_CHARS = 20_000
CONTEXT_TRUNCATE_HEAD_RATIO = 0.7
CONTEXT_TRUNCATE_TAIL_RATIO = 0.2

def _truncate_content(content: str, filename: str, max_chars: int = CONTEXT_FILE_MAX_CHARS) -> str:
    """Head/tail truncation with a marker in the middle."""
```

---

## 6. Context Compression

### 6.1 Compression Algorithm

```python
# trajectory_compressor.py:52-64
"""
Compression Strategy:
1. Protect first turns (system, human, first gpt, first tool)
2. Protect last N turns (final actions and conclusions)
3. Compress MIDDLE turns only, starting from 2nd tool response
4. Compress only as much as needed to fit under target
5. Replace compressed region with a single human summary message
6. Keep remaining tool calls intact (model continues working after summary)
"""
```

### 6.2 ContextCompressor Class

```python
# agent/context_compressor.py:57-89
class ContextCompressor:
    """Compresses conversation context when approaching the model's context limit.

    Algorithm:
      1. Prune old tool results (cheap, no LLM call)
      2. Protect head messages (system prompt + first exchange)
      3. Protect tail messages by token budget (most recent ~20K tokens)
      4. Summarize middle turns with structured LLM prompt
      5. On subsequent compactions, iteratively update the previous summary
    """
    
    def __init__(
        self,
        model: str,
        threshold_percent: float = 0.50,
        protect_first_n: int = 3,
        protect_last_n: int = 20,
        summary_target_ratio: float = 0.20,
    ):
```

### 6.3 Structured Summary Template

```python
# agent/context_compressor.py:190-220
prompt = f"""Create a structured handoff summary for a later assistant that will continue this conversation after earlier turns are compacted.

TURNS TO SUMMARIZE:
{content_to_summarize}

Use this exact structure:

## Goal
[What the user is trying to accomplish]

## Constraints & Preferences
[User preferences, coding style, constraints, important decisions]

## Progress
### Done
[Completed work — include specific file paths, commands run, results obtained]
### In Progress
[Work currently underway]
### Blocked
[Any blockers or issues encountered]

## Key Decisions
[Important technical decisions and why they were made]

## Relevant Files
[Files read, modified, or created — with brief note on each]

## Next Steps
[What needs to happen next to continue the work]

## Critical Context
[Any specific values, error messages, configuration details, or data that would be lost without explicit preservation]
"""
```

### 6.4 Iterative Summary Updates

```python
# agent/context_compressor.py:155-185
if self._previous_summary:
    # Iterative update: preserve existing info, add new progress
    prompt = f"""You are updating a context compaction summary. A previous compaction produced the summary below. New conversation turns have occurred since then and need to be incorporated.

PREVIOUS SUMMARY:
{self._previous_summary}

NEW TURNS TO INCORPORATE:
{content_to_summarize}

Update the summary using this exact structure. PRESERVE all existing information that is still relevant. ADD new progress. Move items from "In Progress" to "Done" when completed.
"""
```

### 6.5 Tool Result Pruning

```python
# agent/context_compressor.py:101-130
def _prune_old_tool_results(
    self, messages: List[Dict[str, Any]], protect_tail_count: int,
) -> tuple[List[Dict[str, Any]], int]:
    """Replace old tool result contents with a short placeholder."""
    _PRUNED_TOOL_PLACEHOLDER = "[Old tool output cleared to save context space]"
    
    for i in range(prune_boundary):
        msg = result[i]
        if msg.get("role") != "tool":
            continue
        if len(content) > 200:
            result[i] = {**msg, "content": _PRUNED_TOOL_PLACEHOLDER}
            pruned += 1
```

---

## 7. Key Patterns for Wakey

### 7.1 Skill Creation Triggers

Hermes uses iteration counting to trigger skill review:

```rust
// Wakey adaptation
struct SkillNudge {
    iters_since_skill: u32,
    nudge_interval: u32, // Default 10
}

impl SkillNudge {
    fn on_tool_call(&mut self) {
        self.iters_since_skill += 1;
    }
    
    fn on_skill_use(&mut self) {
        self.iters_since_skill = 0;
    }
    
    fn should_nudge(&self) -> bool {
        self.iters_since_skill >= self.nudge_interval
    }
}
```

### 7.2 Frozen Memory Snapshot

For prefix caching, memory is loaded once and frozen:

```rust
// Wakey adaptation
struct MemoryStore {
    entries: Vec<String>,
    char_limit: usize,
    frozen_snapshot: Option<String>, // Set at load, never mutated
}

impl MemoryStore {
    fn load_from_disk(&mut self) {
        self.entries = self.read_entries_from_file();
        self.frozen_snapshot = Some(self.render_block());
    }
    
    fn format_for_system_prompt(&self) -> Option<&str> {
        self.frozen_snapshot.as_deref()
    }
}
```

### 7.3 Background Review Pattern

Spawn a sub-agent to review after task completion:

```rust
// Wakey adaptation (conceptual)
fn spawn_background_review(
    messages: Vec<Message>,
    review_type: ReviewType,
) {
    thread::spawn(move || {
        let prompt = match review_type {
            ReviewType::Memory => MEMORY_REVIEW_PROMPT,
            ReviewType::Skills => SKILL_REVIEW_PROMPT,
            ReviewType::Combined => COMBINED_REVIEW_PROMPT,
        };
        
        let review_agent = AIAgent::new(/* same config */);
        review_agent.run_conversation(prompt, messages);
        // Review agent writes directly to shared stores
    });
}
```

### 7.4 Structured Context Summary

When compressing, use a structured handoff format:

```markdown
## Goal
[User's objective]

## Constraints & Preferences
[User preferences, constraints, decisions]

## Progress
### Done
[Completed work with file paths and results]
### In Progress
[Current work]
### Blocked
[Blockers]

## Key Decisions
[Technical decisions with rationale]

## Relevant Files
[Files with brief notes]

## Next Steps
[What to do next]

## Critical Context
[Values, errors, config details to preserve]
```

### 7.5 Tool-Aware System Prompt

Only inject guidance for tools that are actually loaded:

```rust
fn build_system_prompt(&self) -> String {
    let mut parts = vec![self.identity.clone()];
    
    if self.tools.contains("memory") {
        parts.push(MEMORY_GUIDANCE.to_string());
    }
    if self.tools.contains("session_search") {
        parts.push(SESSION_SEARCH_GUIDANCE.to_string());
    }
    if self.tools.contains("skill_manage") {
        parts.push(SKILLS_GUIDANCE.to_string());
    }
    
    parts.join("\n\n")
}
```

### 7.6 FTS5 Search for Session Recall

SQLite FTS5 provides fast full-text search:

```sql
CREATE VIRTUAL TABLE messages_fts USING fts5(
    content,
    content=messages,
    content_rowid=id
);

-- Triggers for automatic index maintenance
CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content) VALUES (new.id, new.content);
END;
```

---

## Appendix: File Reference

| Component | File | Lines |
|-----------|------|-------|
| Skill CRUD | `tools/skill_manager_tool.py` | ~500 |
| Memory Store | `tools/memory_tool.py` | ~400 |
| Session Search | `tools/session_search_tool.py` | ~350 |
| Prompt Builder | `agent/prompt_builder.py` | ~500 |
| Context Compressor | `agent/context_compressor.py` | ~450 |
| Main Agent Loop | `run_agent.py` | ~9000 |
| Honcho Client | `honcho_integration/client.py` | ~350 |
| Honcho Session | `honcho_integration/session.py` | ~700 |
| SQLite State | `hermes_state.py` | ~1100 |
| Skills Tool | `tools/skills_tool.py` | ~1200 |