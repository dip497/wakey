# Hermes Agent Implementation Patterns

**Research Date:** April 2026  
**Repository:** https://github.com/NousResearch/hermes-agent  
**Purpose:** Understand implementation patterns for Wakey's Rust learning loop

---

## Executive Summary

Hermes Agent is a Python-based AI companion with sophisticated self-improving capabilities. This report analyzes 5 key focus areas with code-level details and adaptation notes for Wakey's Rust implementation.

**Key Insights:**
1. Skills are Markdown files with YAML frontmatter, stored in `~/.hermes/skills/`
2. Skill extraction is **LLM-driven** via tool calls (not automatic detection)
3. Memory is file-based (MEMORY.md/USER.md) with character limits
4. Honcho integration provides cross-session user modeling via external API
5. Context compression uses iterative summarization with structured templates

---

## 1. Learning Loop (Skill Extraction)

### How Skills Are Detected and Created

**Critical Finding:** Hermes does NOT automatically detect reusable tasks. Instead, it uses:

1. **Explicit LLM Tool Calls** - The `skill_manage` tool allows the agent to create/update/delete skills
2. **Periodic Nudging** - After N tool iterations (`_skill_nudge_interval`), a background review prompts skill creation
3. **Guidance in System Prompt** - Explicit instructions tell the agent when to save skills

#### System Prompt Guidance (from `agent/prompt_builder.py`)

```python
SKILLS_GUIDANCE = (
    "After completing a complex task (5+ tool calls), fixing a tricky error, "
    "or discovering a non-trivial workflow, save the approach as a "
    "skill with skill_manage so you can reuse it next time.\n"
    "When using a skill and finding it outdated, incomplete, or wrong, "
    "patch it immediately with skill_manage(action='patch') — don't wait to be asked. "
    "Skills that aren't maintained become liabilities."
)
```

#### Skill Nudge Logic (from `run_agent.py`)

```python
# After completing a turn, check if skill review is needed
_should_review_skills = False
if (self._skill_nudge_interval > 0
        and self._iters_since_skill >= self._skill_nudge_interval
        and "skill_manage" in self.valid_tool_names):
    _should_review_skills = True
    self._iters_since_skill = 0

# Background review runs AFTER response is delivered
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

### Skill Format

Skills are stored as Markdown files with YAML frontmatter:

```markdown
---
name: test-driven-development
description: Use when implementing any feature or bugfix, before writing implementation code. Enforces RED-GREEN-REFACTOR cycle with test-first approach.
version: 1.1.0
author: Hermes Agent (adapted from obra/superpowers)
license: MIT
metadata:
  hermes:
    tags: [testing, tdd, development, quality, red-green-refactor]
    related_skills: [systematic-debugging, writing-plans, subagent-driven-development]
---

# Test-Driven Development (TDD)

## Overview

Write the test first. Watch it fail. Write minimal code to pass.

**Core principle:** If you didn't watch the test fail, you don't know if it tests the right thing.

## When to Use

**Always:**
- New features
- Bug fixes
- Refactoring
...
```

### Skill Directory Structure

```
~/.hermes/skills/
├── software-development/
│   ├── test-driven-development/
│   │   ├── SKILL.md           # Main instructions (required)
│   │   ├── references/        # Supporting documentation
│   │   └── templates/         # Output templates
│   ├── systematic-debugging/
│   └── writing-plans/
├── mlops/
│   ├── training/
│   └── inference/
└── DESCRIPTION.md             # Category description
```

### Skill Loading and Matching

Skills are loaded progressively (from `tools/skills_tool.py`):

```python
def _find_all_skills(*, skip_disabled: bool = False) -> List[Dict[str, Any]]:
    """Recursively find all skills in ~/.hermes/skills/ and external dirs."""
    skills = []
    seen_names: set = set()
    disabled = set() if skip_disabled else _get_disabled_skill_names()
    
    for scan_dir in dirs_to_scan:
        for skill_md in scan_dir.rglob("SKILL.md"):
            # Parse frontmatter
            content = skill_md.read_text(encoding="utf-8")[:4000]
            frontmatter, body = _parse_frontmatter(content)
            
            # Check platform compatibility
            if not skill_matches_platform(frontmatter):
                continue
            
            name = frontmatter.get("name", skill_dir.name)[:MAX_NAME_LENGTH]
            if name in seen_names or name in disabled:
                continue
            
            description = frontmatter.get("description", "")
            # ... append to skills list
```

### Skill Management Tool (from `tools/skill_manager_tool.py`)

```python
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
    
    Actions: create, edit, patch, delete, write_file, remove_file
    """
```

---

### For Wakey (Rust) Adaptation

| Pattern | Hermes (Python) | Wakey (Rust) Recommendation |
|---------|-----------------|----------------------------|
| Skill detection | LLM-driven via tool calls + nudging | Keep LLM-driven but add event-based triggers (e.g., after successful complex action) |
| Skill format | Markdown + YAML frontmatter | Use TOML frontmatter (Rust-native) or keep YAML with `serde_yaml` |
| Skill storage | `~/.hermes/skills/` directory | Use XDG data dir: `~/.local/share/wakey/skills/` |
| Skill loading | Recursive glob + parse on startup | Use `notify` crate for hot-reloading |
| Skill matching | Manual string matching | Use `fuse-rs` for fuzzy matching |
| Skill registry | In-memory list + disk scan | Use `DashMap` for concurrent access |

**Key Adaptation:**
```rust
// skills/skill.rs
#[derive(Debug, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub tags: Vec<String>,
    pub triggers: Vec<SkillTrigger>,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum SkillTrigger {
    #[serde(rename = "tool_used")]
    ToolUsed { tool_name: String },
    #[serde(rename = "error_pattern")]
    ErrorPattern { pattern: String },
    #[serde(rename = "task_type")]
    TaskType { task_type: String },
}
```

---

## 2. User Modeling (Honcho Integration)

### Honcho Architecture

Honcho is an external AI-native memory service. Hermes integrates via the `honcho-ai` Python SDK.

**Configuration Resolution (from `honcho_integration/client.py`):**

```python
@dataclass
class HonchoClientConfig:
    host: str = HOST
    workspace_id: str = "hermes"
    api_key: str | None = None
    environment: str = "production"
    base_url: str | None = None  # Self-hosted option
    
    # Identity
    peer_name: str | None = None
    ai_peer: str = "hermes"
    
    # Memory modes
    memory_mode: str = "hybrid"  # "hybrid" | "honcho"
    peer_memory_modes: dict[str, str] = field(default_factory=dict)
    
    # Write frequency
    write_frequency: str | int = "async"  # "async" | "turn" | "session" | N
    
    # Recall mode
    recall_mode: str = "hybrid"  # "hybrid" | "context" | "tools"
```

### Session Management (from `honcho_integration/session.py`)

```python
@dataclass
class HonchoSession:
    key: str  # channel:chat_id
    user_peer_id: str
    assistant_peer_id: str
    honcho_session_id: str
    messages: list[dict[str, Any]] = field(default_factory=list)

class HonchoSessionManager:
    def get_or_create(self, key: str) -> HonchoSession:
        """Get an existing session or create a new one."""
        
    def dialectic_query(self, session_key: str, query: str) -> str:
        """Query Honcho's dialectic endpoint about a peer.
        
        Runs an LLM on Honcho's backend against the target peer's full
        representation. Higher latency than context() — call async.
        """
        
    def prefetch_dialectic(self, session_key: str, query: str) -> None:
        """Fire a dialectic_query in a background thread."""
        
    def create_conclusion(self, session_key: str, content: str) -> bool:
        """Write a conclusion about the user back to Honcho."""
```

### How User Modeling Influences Behavior

1. **Context Injection** - Honcho's `peer_representation` is injected into the system prompt
2. **Dialectic Queries** - Background LLM queries synthesize user insights
3. **Conclusions** - The agent can write facts back to Honcho's representation

**Memory Mode Decision (from `run_agent.py`):**

```python
# Gate local memory writes based on per-peer memory modes.
# "honcho" = Honcho only, disable local writes.
if self._honcho_config and self._honcho:
    _agent_mode = _hcfg.peer_memory_mode(_hcfg.ai_peer)
    _user_mode = _hcfg.peer_memory_mode(_hcfg.peer_name or "user")
    if _agent_mode == "honcho":
        self._memory_flush_min_turns = 0
        self._memory_enabled = False
    if _user_mode == "honcho":
        self._user_profile_enabled = False
```

---

### For Wakey (Rust) Adaptation

| Pattern | Hermes (Python) | Wakey (Rust) Recommendation |
|---------|-----------------|----------------------------|
| External service | Honcho API | Consider local-first: SQLite + optional sync |
| Session management | Python dataclass + async queue | Use `tokio::sync::RwLock<Session>` |
| Peer representation | Honcho's LLM-based | Build local user model with incremental updates |
| Write frequency | Async by default | Use `tokio::sync::mpsc` channel for async writes |
| Dialectic queries | Honcho's backend LLM | Use local embeddings + small model for synthesis |

**Key Adaptation:**
```rust
// user_model/mod.rs
pub struct UserModel {
    pub peer_id: PeerId,
    pub preferences: DashMap<String, String>,
    pub patterns: Vec<BehaviorPattern>,
    pub last_updated: Instant,
}

impl UserModel {
    pub async fn update_from_interaction(&self, event: &InteractionEvent) {
        // Incremental update without full LLM call
        match event {
            InteractionEvent::Correction { field, value } => {
                self.preferences.insert(field.clone(), value.clone());
            }
            InteractionEvent::Pattern { pattern } => {
                self.patterns.push(pattern.clone());
            }
            _ => {}
        }
    }
    
    pub fn format_for_prompt(&self) -> String {
        // Format for system prompt injection
    }
}
```

---

## 3. Memory System

### Memory Store (from `tools/memory_tool.py`)

```python
class MemoryStore:
    """
    Bounded curated memory with file persistence.
    
    Two stores:
      - MEMORY.md: agent's personal notes (environment facts, conventions)
      - USER.md: user profile (preferences, communication style)
    """
    
    def __init__(self, memory_char_limit: int = 2200, user_char_limit: int = 1375):
        self.memory_entries: List[str] = []
        self.user_entries: List[str] = []
        self._system_prompt_snapshot: Dict[str, str] = {"memory": "", "user": ""}
    
    def add(self, target: str, content: str) -> Dict[str, Any]:
        """Append a new entry. Returns error if char limit exceeded."""
        
    def replace(self, target: str, old_text: str, new_content: str) -> Dict[str, Any]:
        """Find entry containing old_text substring, replace it."""
        
    def remove(self, target: str, old_text: str) -> Dict[str, Any]:
        """Remove entry containing old_text substring."""
    
    def format_for_system_prompt(self, target: str) -> Optional[str]:
        """Return the frozen snapshot for system prompt injection."""
```

### Key Design Decisions

1. **Frozen Snapshot** - Memory is loaded once at session start and frozen. Mid-session writes update disk but don't affect the system prompt (preserves prefix cache).

2. **Character Limits** - Not tokens (model-agnostic):
   - MEMORY.md: 2200 chars
   - USER.md: 1375 chars

3. **Entry Delimiter** - Uses `§` (section sign) to separate entries

4. **Security Scanning** - Content is scanned for injection patterns before storage:

```python
_MEMORY_THREAT_PATTERNS = [
    (r'ignore\s+(previous|all|above|prior)\s+instructions', "prompt_injection"),
    (r'you\s+are\s+now\s+', "role_hijack"),
    (r'curl\s+[^\n]*\$\{?\w*(KEY|TOKEN|SECRET)', "exfil_curl"),
    # ... more patterns
]
```

### Memory File Format

```markdown
╔══════════════════════════════════════════════════════╗
MEMORY (your personal notes) [23% — 500/2200 chars]
╚══════════════════════════════════════════════════════╝
§
User prefers dark mode in all editors
§
Project uses Rust 2024 edition with tokio runtime
§
Terminal commands should use --non-interactive flags
```

---

### For Wakey (Rust) Adaptation

| Pattern | Hermes (Python) | Wakey (Rust) Recommendation |
|---------|-----------------|----------------------------|
| Storage | Plain text files | Keep text files (simple, debuggable) |
| Snapshot | Frozen at session start | Same pattern - preserve prefix cache |
| Limits | Character-based | Use token estimation with `tokenizers` crate |
| Security | Regex patterns | Use `regex` crate + add more patterns |
| Concurrency | File locks with `fcntl` | Use `parking_lot::Mutex` + atomic writes |

**Key Adaptation:**
```rust
// memory/store.rs
pub struct MemoryStore {
    memory_entries: Vec<String>,
    user_entries: Vec<String>,
    memory_char_limit: usize,
    user_char_limit: usize,
    frozen_snapshot: Option<MemorySnapshot>,
}

impl MemoryStore {
    pub fn load_from_disk(&mut self, path: &Path) -> Result<()> {
        let content = fs::read_to_string(path)?;
        self.memory_entries = content
            .split("§\n")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        // Freeze snapshot for system prompt
        self.frozen_snapshot = Some(self.render_snapshot());
        Ok(())
    }
    
    pub fn add(&mut self, target: MemoryTarget, content: &str) -> Result<()> {
        self.scan_for_injection(content)?;
        let entries = match target {
            MemoryTarget::Agent => &mut self.memory_entries,
            MemoryTarget::User => &mut self.user_entries,
        };
        
        // Check limit
        let new_total = self.calculate_total_chars(target) + content.len();
        let limit = match target {
            MemoryTarget::Agent => self.memory_char_limit,
            MemoryTarget::User => self.user_char_limit,
        };
        
        if new_total > limit {
            return Err(MemoryError::LimitExceeded { limit, actual: new_total });
        }
        
        entries.push(content.to_string());
        self.save_to_disk(target)?;
        Ok(())
    }
}
```

---

## 4. Agent Loop

### Core Orchestration (from `run_agent.py`)

```python
class AIAgent:
    def run_conversation(self, message: str, ...) -> Dict[str, Any]:
        """Main conversation loop with tool calling."""
        
        messages = [{"role": "user", "content": message}]
        
        while True:
            # 1. Check iteration budget
            if not self.iteration_budget.consume():
                break
            
            # 2. Track skill nudge counter
            if self._skill_nudge_interval > 0:
                self._iters_since_skill += 1
            
            # 3. Build API messages
            api_messages = self._prepare_messages(messages)
            
            # 4. Apply prompt caching (Anthropic)
            if self._use_prompt_caching:
                api_messages = apply_anthropic_cache_control(api_messages)
            
            # 5. Make API call (with retries)
            response = self._make_api_call(api_messages)
            
            # 6. Check for tool calls
            if response.tool_calls:
                # Execute tools
                for tool_call in response.tool_calls:
                    result = handle_function_call(tool_call)
                    messages.append({
                        "role": "tool",
                        "content": result,
                        "tool_call_id": tool_call.id,
                    })
            else:
                # Final response
                break
        
        # 7. Background skill/memory review
        if _should_review_skills or _should_review_memory:
            self._spawn_background_review(messages, ...)
        
        return {"final_response": response.content, ...}
```

### Tool Execution

```python
# Tool registration (from model_tools.py)
def get_tool_definitions(enabled_toolsets, disabled_toolsets) -> List[Dict]:
    """Get OpenAI-compatible tool definitions."""
    
def handle_function_call(tool_call, ...) -> str:
    """Execute a tool call and return the result."""
    tool_name = tool_call.function.name
    args = json.loads(tool_call.function.arguments)
    
    # Dispatch to registered handler
    handler = registry.get_handler(tool_name)
    result = handler(args, task_id=task_id)
    
    return result
```

### Prompt Building (from `agent/prompt_builder.py`)

```python
def build_skills_system_prompt() -> str:
    """Build the skills portion of the system prompt."""
    
    # Load snapshot if available
    snapshot = _load_skills_snapshot(skills_dir)
    if snapshot:
        return _render_skills_from_snapshot(snapshot)
    
    # Otherwise, scan all skills
    skill_entries = []
    for skill_md in skills_dir.rglob("SKILL.md"):
        frontmatter, body = parse_frontmatter(content)
        
        # Check platform
        if not skill_matches_platform(frontmatter):
            continue
        
        # Check disabled
        if name in get_disabled_skill_names():
            continue
        
        skill_entries.append({
            "name": frontmatter.get("name"),
            "description": frontmatter.get("description"),
            "category": category,
        })
    
    return _render_skills_prompt(skill_entries)
```

### Context Compression (from `agent/context_compressor.py`)

```python
class ContextCompressor:
    def should_compress(self, prompt_tokens: int) -> bool:
        """Check if context exceeds threshold (default 50%)."""
        return prompt_tokens >= self.threshold_tokens
    
    def compress(self, messages: List[Dict]) -> List[Dict]:
        """Compress conversation by summarizing middle turns."""
        
        # 1. Prune old tool results (cheap, no LLM)
        messages, pruned = self._prune_old_tool_results(messages)
        
        # 2. Protect head messages (system + first exchange)
        head = messages[:self.protect_first_n]
        
        # 3. Protect tail by token budget
        tail = self._extract_protected_tail(messages)
        
        # 4. Summarize middle
        middle = messages[len(head):-len(tail)]
        summary = self._generate_summary(middle)
        
        # 5. Build compressed messages
        return [
            *head,
            {"role": "user", "content": f"[CONTEXT SUMMARY]\n{summary}"},
            *tail,
        ]
    
    def _generate_summary(self, turns: List[Dict]) -> str:
        """Generate structured summary with LLM."""
        
        # Use iterative update if previous summary exists
        if self._previous_summary:
            prompt = f"""Update the previous summary with new turns...
            
PREVIOUS SUMMARY:
{self._previous_summary}

NEW TURNS:
{self._serialize_for_summary(turns)}
"""
        else:
            prompt = f"""Summarize these conversation turns...

{self._serialize_for_summary(turns)}

Use this structure:
## Goal
## Constraints & Preferences
## Progress
### Done
### In Progress
## Key Decisions
## Relevant Files
## Next Steps
## Critical Context
"""
        
        return call_llm(prompt, model=self.summary_model)
```

---

### For Wakey (Rust) Adaptation

| Pattern | Hermes (Python) | Wakey (Rust) Recommendation |
|---------|-----------------|----------------------------|
| Agent loop | Synchronous while loop | Async with `tokio::select!` for interruptibility |
| Tool registry | Dict-based | Use `DashMap<String, ToolHandler>` |
| Prompt caching | Anthropic-specific | Support multiple providers via trait |
| Context compression | LLM-based summarization | Use local model (Phi-3, Gemma) for summary |
| Tool execution | Sequential by default | Parallel where safe (read-only tools) |

**Key Adaptation:**
```rust
// agent/loop.rs
pub async fn run_conversation(
    &mut self,
    message: String,
) -> Result<ConversationResult> {
    let mut messages = vec![Message::user(message)];
    
    loop {
        // Check budget
        if !self.iteration_budget.try_consume()? {
            break;
        }
        
        // Build API request
        let request = self.build_request(&messages)?;
        
        // Make API call with timeout
        let response = tokio::select! {
            result = self.client.chat(request) => result?,
            _ = self.interrupt.recv() => {
                return Ok(ConversationResult::Interrupted);
            }
        };
        
        // Handle tool calls
        if let Some(tool_calls) = response.tool_calls {
            // Execute tools in parallel where safe
            let results = self.execute_tools_parallel(tool_calls).await;
            messages.extend(results);
        } else {
            return Ok(ConversationResult::Complete {
                response: response.content,
            });
        }
    }
    
    Ok(ConversationResult::BudgetExhausted)
}
```

---

## 5. Skill Format & Registry

### Complete Skill Example

```yaml
---
name: test-driven-development
description: Use when implementing any feature or bugfix, before writing implementation code.
version: 1.1.0
author: Hermes Agent
license: MIT
platforms: [macos, linux]  # Optional platform restriction
metadata:
  hermes:
    tags: [testing, tdd, development]
    related_skills: [systematic-debugging]
    fallback_for_toolsets: []  # Auto-load when these toolsets are missing
    requires_toolsets: [terminal, file]  # Required toolsets
---

# Test-Driven Development (TDD)

## Overview
Write the test first. Watch it fail. Write minimal code to pass.

## When to Use
- New features
- Bug fixes
- Refactoring

## Steps
1. Write failing test
2. Run test to verify failure
3. Write minimal code to pass
4. Run test to verify pass
5. Refactor if needed

## Pitfalls
- Skipping the "watch it fail" step
- Writing tests after implementation
- Over-mocking
```

### Skill Registry (from `tools/registry.py`)

```python
class ToolRegistry:
    """Registry for all tools with schemas and handlers."""
    
    def register(
        self,
        name: str,
        toolset: str,
        schema: Dict[str, Any],
        handler: Callable,
        check_fn: Callable = None,
        emoji: str = "",
    ):
        """Register a tool with its schema and handler."""
        
    def get_handler(self, name: str) -> Callable:
        """Get the handler for a tool."""
        
    def get_toolset_for_tool(self, name: str) -> str:
        """Get the toolset a tool belongs to."""
```

### Skill Discovery Flow

```
1. Startup: Scan ~/.hermes/skills/**/*.md
2. Parse: Extract frontmatter + body
3. Filter: Platform check, disabled check
4. Cache: Store in memory + optional disk snapshot
5. Inject: Add to system prompt at conversation start
6. Access: skill_view() loads full content on demand
```

---

### For Wakey (Rust) Adaptation

| Pattern | Hermes (Python) | Wakey (Rust) Recommendation |
|---------|-----------------|----------------------------|
| Frontmatter | YAML | TOML (Rust-native) or YAML with `serde_yaml` |
| Registry | Python dict | `DashMap` for concurrent access |
| Discovery | Glob scan on startup | `notify` crate for hot-reload |
| Schema | OpenAI function calling format | Same format (standard) |
| Handler | Python callable | `async fn(Context, Args) -> Result<String>` |

**Key Adaptation:**
```rust
// skills/registry.rs
use dashmap::DashMap;

pub struct SkillRegistry {
    skills: DashMap<String, Skill>,
    categories: DashMap<String, CategoryInfo>,
    disabled: Arc<RwLock<HashSet<String>>>,
}

impl SkillRegistry {
    pub fn discover(&self, skills_dir: &Path) -> Result<()> {
        for entry in walkdir::WalkDir::new(skills_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_name() == "SKILL.md" {
                let skill = Skill::from_file(entry.path())?;
                
                // Check platform
                if !skill.matches_platform(PLATFORM) {
                    continue;
                }
                
                // Check disabled
                if self.disabled.read().contains(&skill.name) {
                    continue;
                }
                
                self.skills.insert(skill.name.clone(), skill);
            }
        }
        Ok(())
    }
    
    pub fn get(&self, name: &str) -> Option<Skill> {
        self.skills.get(name).map(|r| r.clone())
    }
    
    pub fn search(&self, query: &str) -> Vec<SkillMatch> {
        // Fuzzy search using fuse-rs or similar
    }
}
```

---

## Summary: Key Patterns for Wakey

### 1. Learning Loop
- **Hermes**: LLM-driven via `skill_manage` tool + periodic nudging
- **Wakey**: Keep LLM-driven, add event-based triggers for proactive skill extraction

### 2. User Modeling
- **Hermes**: External Honcho API for cross-session memory
- **Wakey**: Local-first with SQLite, optional sync to external service

### 3. Memory System
- **Hermes**: File-based MEMORY.md/USER.md with frozen snapshots
- **Wakey**: Keep file-based, add structured events for better querying

### 4. Agent Loop
- **Hermes**: Synchronous with tool-calling loop
- **Wakey**: Async with `tokio::select!` for interruptibility

### 5. Skill Format
- **Hermes**: Markdown + YAML frontmatter
- **Wakey**: Markdown + TOML frontmatter (or keep YAML for compatibility)

---

## References

- Hermes Agent Repo: https://github.com/NousResearch/hermes-agent
- Honcho Documentation: https://honcho.dev
- agentskills.io Standard: https://agentskills.io