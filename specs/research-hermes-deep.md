# Deep Research: Hermes Agent Internals

## Goal
Read the ACTUAL Hermes Agent codebase and extract implementation details for the learning loop, skill_manage tool, memory system, and user modeling.

## Repo
Clone https://github.com/NousResearch/hermes-agent to /tmp/hermes-research/ if not exists.

## Focus Areas (read actual Python code)

### 1. skill_manage Tool
- Find the actual implementation of create/patch/edit/delete
- How does it decide WHEN to create a skill?
- What triggers skill creation vs update?
- How is SKILL.md generated from experience?

### 2. Learning Loop
- Find the actual learning loop code
- How does it track task success/failure?
- How does skill refinement work on reuse?
- What's the feedback loop between execution and skill update?

### 3. Memory System
- How does SQLite FTS5 work in practice?
- How does LLM summarization compress memories?
- How are MEMORY.md and USER.md maintained?
- What triggers a memory write vs read?

### 4. User Modeling (Honcho)
- How is the user model structured?
- What data points are tracked?
- How does it influence agent behavior?

### 5. Prompt Building
- How is context assembled before LLM call?
- How does prompt caching work?
- How does context compression work?
- What's the prompt structure?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/hermes-deep.md
Include ACTUAL code snippets with file paths.
