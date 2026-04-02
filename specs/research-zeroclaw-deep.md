# Deep Research: ZeroClaw Internals

## Goal
Read the ACTUAL ZeroClaw Rust codebase and extract implementation details for memory, agent loop, tool execution, cron/scheduler, and security.

## Repo
Located at /home/dipendra-sharma/projects/zeroclaw/
If not exists, clone https://github.com/zeroclaw-labs/zeroclaw

## Focus Areas (read actual .rs files, not wiki)

### 1. Agent Loop (run_tool_call_loop)
- Find the actual loop_.rs or equivalent
- How does it iterate tool calls?
- How does auto-compaction work?
- How is max_iterations enforced?

### 2. Memory System (SQLite + Vector)
- Find the Memory trait definition
- How is hybrid search implemented (vector 0.7 + keyword 0.3)?
- How are embeddings generated and stored?
- How does soul export to markdown work?
- Find the actual SQLite schema

### 3. Cron/Scheduler
- How does the daemon mode work?
- How are cron jobs stored and triggered?
- How does window-based scheduling prevent missed beats?
- Find the actual scheduler code

### 4. Security Policy
- Find SecurityPolicy::can_act() implementation
- How are tool calls gated?
- How does credential scrubbing work?

### 5. Provider Trait (LLM Client)
- Find the actual Provider trait
- How does OpenAI-compatible chat work?
- How is streaming parsed?
- How does model switching work at runtime?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/zeroclaw-deep.md
Include ACTUAL Rust code snippets with file paths.
