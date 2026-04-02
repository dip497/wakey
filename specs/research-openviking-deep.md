# Deep Research: OpenViking Internals

## Goal
Read the ACTUAL OpenViking codebase and extract implementation details for the viking:// filesystem, tiered loading, skill management, memory extraction, and self-evolution.

## Repo
Clone https://github.com/volcengine/OpenViking to /tmp/openviking-research/ if not exists.

## Focus Areas (read actual Python code)

### 1. Viking Filesystem
- How is viking:// protocol implemented?
- How are directories and files structured?
- How does URI resolution work?
- Find the actual filesystem abstraction code

### 2. Tiered Loading (L0/L1/L2)
- How are .abstract.md (L0) files generated?
- How are .overview.md (L1) files generated?
- When does L2 (full content) get loaded?
- What's the token savings in practice?

### 3. Directory Recursive Retrieval
- How does the 3-phase retrieval work (intent → position → drill)?
- How does vector search combine with filesystem navigation?
- Find the actual retrieval pipeline code

### 4. Memory Extraction
- How does end-of-session extraction work?
- How are user preferences auto-detected?
- How are skill execution stats tracked?
- Find the extraction pipeline code

### 5. Skill Self-Evolution
- How does viking://agent/skills/ update itself?
- How are success/fail stats used to improve skills?
- How does the agent decide to create vs update a skill?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/openviking-deep.md
Include ACTUAL code snippets with file paths.
