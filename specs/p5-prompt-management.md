# P5: Prompt Management — SOUL.md, USER.md, MEMORY.md Integration

## Goal
Replace hardcoded personality with file-based prompt layers. Follows OpenClaw/ZeroClaw/Hermes pattern. Integrates with existing wakey-context memory and wakey-skills registry.

## Current State
- `decision.rs` has `build_system_prompt()` that hardcodes "You are {name}, a friendly AI companion"
- Memory recall works (SqliteMemory)
- Skill registry works (SkillRegistry.find())
- But no SOUL.md, USER.md, or MEMORY.md files

## Prompt Assembly Order (every LLM call)

```
┌─────────────────────────────────────────────┐
│ 1. SOUL.md (WHO Wakey is)                   │ ← Loaded from ~/.wakey/SOUL.md
│    Personality, tone, boundaries, mood rules │    Falls back to default if missing
├─────────────────────────────────────────────┤
│ 2. USER.md (WHO the user is)                │ ← Loaded from ~/.wakey/USER.md
│    Name, preferences, style, patterns       │    Auto-updated by learning system
├─────────────────────────────────────────────┤
│ 3. Recalled memories (WHAT happened)        │ ← SqliteMemory.recall(query, 5)
│    Recent relevant memories from context DB │    Already implemented
├─────────────────────────────────────────────┤
│ 4. Active skill (HOW to do something)       │ ← SkillRegistry.find(query)
│    Matched skill instructions               │    Already implemented
├─────────────────────────────────────────────┤
│ 5. Current context (WHAT's happening now)   │ ← From heartbeat events
│    Active window, time, system state        │    Already implemented
├─────────────────────────────────────────────┤
│ 6. Conversation history                     │ ← Last 10 messages
│    Already implemented in DecisionContext   │
├─────────────────────────────────────────────┤
│ 7. Current input                            │ ← User message or heartbeat trigger
└─────────────────────────────────────────────┘
```

## Files to Create

### ~/.wakey/SOUL.md (default, created on first run)

```markdown
# Soul

You are Wakey, a friendly AI companion that lives on the user's desktop.

## Personality
- Warm, curious, a little playful
- Casual, not robotic or formal
- Concise — this is conversation, not an essay
- You notice what the user is doing and comment naturally
- You know when to be quiet (deep focus = silence)

## Voice
- Short sentences. Natural rhythm.
- Use humor sparingly but genuinely
- Never announce you're an AI. Just be Wakey.
- Match the user's energy — if they're tired, be gentle

## Boundaries
- Don't nag about productivity
- Don't give unsolicited life advice
- Don't be a manager — be a friend
- If unsure, ask. Don't assume.

## Expression
- End every response with MOOD:<mood> on a new line
- Available moods: neutral, happy, excited, concerned, thinking, empathetic, sleepy, surprised, playful, focused
- Match the mood to your response's emotional tone
- For long responses, use multiple MOOD: tags (one per paragraph)
```

### ~/.wakey/USER.md (default, auto-populated over time)

```markdown
# User

## Profile
- Name: (learned from conversation)
- Preferred style: (detected from interactions)

## Patterns
- (auto-populated: "works late", "uses VS Code", etc.)

## Preferences
- (auto-populated: "likes short responses", "prefers humor", etc.)
```

### ~/.wakey/MEMORY.md (curated, auto-maintained)

```markdown
# Memory

## Key Facts
- (auto-populated from memory system)

## Recent Sessions
- (auto-populated from reflect cycle summaries)
```

## What to Implement

### 1. Prompt file loader (crates/wakey-cortex/src/prompt_loader.rs — NEW)

```rust
pub struct PromptFiles {
    pub soul: String,
    pub user: String,
    pub memory_curated: String,
}

impl PromptFiles {
    /// Load from ~/.wakey/ directory. Create defaults if missing.
    pub fn load(data_dir: &Path) -> WakeyResult<Self> { ... }
    
    /// Create default SOUL.md if it doesn't exist
    fn ensure_soul(data_dir: &Path) -> WakeyResult<String> { ... }
    
    /// Create default USER.md if it doesn't exist
    fn ensure_user(data_dir: &Path) -> WakeyResult<String> { ... }
    
    /// Reload files (call periodically or on file change)
    pub fn reload(&mut self, data_dir: &Path) -> WakeyResult<()> { ... }
}
```

### 2. Update build_system_prompt (crates/wakey-cortex/src/decision.rs)

Replace hardcoded personality with:

```rust
pub async fn build_system_prompt(&self, context_query: &str) -> WakeyResult<String> {
    let mut prompt = String::new();
    
    // 1. SOUL.md — identity (truncate at 4000 chars like ZeroClaw)
    prompt.push_str(&self.prompt_files.soul);
    prompt.push_str("\n\n");
    
    // 2. USER.md — user info
    if !self.prompt_files.user.is_empty() {
        prompt.push_str(&self.prompt_files.user);
        prompt.push_str("\n\n");
    }
    
    // 3. MEMORY.md — curated long-term
    if !self.prompt_files.memory_curated.is_empty() {
        prompt.push_str("## Curated Memory\n");
        prompt.push_str(&self.prompt_files.memory_curated);
        prompt.push_str("\n\n");
    }
    
    // 4. Recalled memories (already implemented — from SqliteMemory)
    let memories = self.memory.recall(context_query, 5).await?;
    if !memories.is_empty() {
        prompt.push_str("## Recent Memories\n");
        for mem in &memories {
            prompt.push_str(&format!("- {}\n", mem.l0()));
        }
        prompt.push('\n');
    }
    
    // 5. Active skill (already implemented — from SkillRegistry)
    if let Some(ref registry) = self.skill_registry
        && let Ok(matches) = registry.find(context_query, 1)
        && let Some(skill) = matches.first()
    {
        prompt.push_str(&format!("## Active Skill: {}\n{}\n\n", skill.name, skill.overview));
    }
    
    // 6. Current context
    prompt.push_str(&format!("## Now\nSession started: {}\n", self.session_start.format("%H:%M")));
    
    Ok(prompt)
}
```

### 3. Auto-update USER.md (in reflect cycle)

During the 15-min reflect cycle, analyze recent conversations and update USER.md:
- Detect user name from conversations
- Detect patterns (working hours, preferred apps)
- Detect preferences (response length, humor level)

Use existing `handle_reflect()` in decision.rs — add USER.md update step.

### 4. Auto-update MEMORY.md (in dream cycle)

During daily dream cycle, curate top memories into MEMORY.md:
- Summarize key facts from SqliteMemory
- Remove stale entries
- Keep under 2000 chars (ZeroClaw uses 20000 max, but we're token-conscious)

### 5. Store SOUL.md in wakey-context filesystem

SOUL.md should also be indexed in the context system:
```
wakey://agent/soul → SOUL.md content
wakey://user/profile → USER.md content  
wakey://agent/memories/curated → MEMORY.md content
```

This way the retrieval system can find personality info when relevant.

### 6. Wire into wakey-app/src/main.rs

On startup:
```rust
// Load prompt files (creates defaults if missing)
let prompt_files = PromptFiles::load(&config.general.data_dir)?;

// Pass to DecisionContext
let decision_ctx = DecisionContext::new(
    persona,
    memory,
    skill_registry,
    prompt_files,  // NEW
    spine,
);
```

### 7. Allow user to edit SOUL.md

SOUL.md is a plain markdown file. User can edit it to customize Wakey's personality:
```bash
# User wants a sassy Wakey:
vim ~/.wakey/SOUL.md
# Change "Warm, curious" to "Sassy, sarcastic but caring"
# Wakey reloads on next session
```

## Read first
- crates/wakey-cortex/src/decision.rs (current prompt assembly)
- crates/wakey-context/src/memory.rs (SqliteMemory interface)
- crates/wakey-context/src/filesystem.rs (wakey:// URIs)
- ZeroClaw pattern: bootstrap files with per-file max chars
- Hermes pattern: SOUL.md as durable identity, project context is local

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app
# Expected: ~/.wakey/SOUL.md created on first run
# LLM responses reflect SOUL.md personality
# MOOD: tags appear in LLM output
```

## Acceptance criteria
- SOUL.md created with default content on first run
- USER.md created with empty template on first run
- build_system_prompt() loads from files, not hardcoded
- Memory recall still works (integrated, not replaced)
- Skill matching still works (integrated, not replaced)
- SOUL.md editable by user, takes effect on next session
- Prompt truncated if too long (max 4000 chars per file)
- cargo check passes
