# Deep Research: OpenClaw Internals

## Goal
Read the ACTUAL OpenClaw codebase and extract implementation details for memory, sessions, gateway, agent loop, and heartbeat.

## Repo
Clone https://github.com/openclaw/openclaw to /tmp/openclaw-research/ if not exists.

## Focus Areas (read actual code, not just docs)

### 1. Gateway Architecture
- How does message routing work between channels and agents?
- How are sessions created, maintained, and destroyed?
- Find the actual gateway entry point code.

### 2. Agent Memory
- How is memory.md structured?
- How does session/resume work at code level?
- How is memory compacted?
- What's the memory persistence format?

### 3. Heartbeat Implementation
- How does the heartbeat timer work?
- What context does HEARTBEAT.md contain?
- How does lightContext mode reduce cost?
- How does isolatedSession work?

### 4. Agent Loop (the actual code)
- Find runEmbeddedPiAgent or equivalent
- How is context assembled before LLM call?
- How are tools executed?
- How is streaming handled?

### 5. Skills Snapshot
- How are skills loaded at session start?
- How does skill precedence work?
- How are skills matched to tasks?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/openclaw-deep.md
Include ACTUAL code snippets with file paths.
