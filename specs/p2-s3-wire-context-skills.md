# P2-S3: Wire Context + Skills Into Cortex

## Goal
Connect wakey-context and wakey-skills to the cortex so Wakey actually remembers, uses skills, and learns from conversations.

## What to implement

### 1. Cortex uses Memory (wakey-cortex/src/decision.rs)
- On startup: initialize SqliteMemory from config
- Before each LLM call: `memory.recall(context_query)` → inject relevant memories into system prompt
- After each conversation: `memory.store()` key facts (user preferences, what happened)
- On Reflect event (15min): summarize recent activity → store as memory

### 2. Cortex uses Skills (wakey-cortex/src/agent_loop.rs)
- On startup: initialize SkillRegistry, scan skills directory
- When user asks something: `registry.find(query)` → check if a skill matches
- If skill found: load SKILL.md content → inject into LLM prompt as instructions
- Track skill usage: `quality.record_selection()`, `quality.record_completion()`
- After complex tasks (5+ LLM turns): trigger learning check → maybe create new skill

### 3. Update main.rs (wakey-app)
- Create SqliteMemory instance at startup
- Create SkillRegistry instance at startup
- Pass both to the decision loop
- Ensure ~/.wakey/context/ directory structure exists on first run

### 4. System prompt enhancement
Current prompt: "You are Wakey, a friendly companion..."
New prompt should include:
- Persona config (name, style)
- Relevant memories from recall
- Active skill instructions (if skill matched)
- Current context (active window, recent events)

### 5. Conversation history
- Keep last 10 messages in working memory
- On Reflect (15min): compress older messages via LLM summarization
- Store compressed summary in memory

## Read first
- crates/wakey-context/src/lib.rs (Memory trait, SqliteMemory)
- crates/wakey-skills/src/lib.rs (SkillRegistry, format)
- crates/wakey-cortex/src/llm.rs (existing LLM client)
- crates/wakey-app/src/main.rs (current wiring)
- docs/architecture/DECISIONS.md #9 (context assembly pipeline)

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app  # Wakey should remember things across restarts
```

## Acceptance criteria
- Memory initialized from SQLite on startup
- Skills directory scanned and indexed
- LLM prompts include relevant memories
- Skills matched and injected when relevant
- Conversation history maintained
- cargo check passes
