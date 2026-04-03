# Research: Compare Memory + Skill Systems — OpenViking vs OpenSpace vs Hermes

## Goal
Side-by-side code-level comparison of the three systems. We need to decide which patterns to adopt for Wakey's context + skill system.

## Repos
- OpenViking: clone https://github.com/volcengine/OpenViking to /tmp/openviking-compare/
- OpenSpace: clone https://github.com/HKUDS/OpenSpace to /tmp/openspace-compare/
- Hermes: clone https://github.com/NousResearch/hermes-agent to /tmp/hermes-compare/

## Compare on these dimensions (read actual code, show snippets)

### 1. Memory Storage
- OpenViking: filesystem (viking://) + AGFS backend
- OpenSpace: SQLite skill registry
- Hermes: SQLite FTS5 + MEMORY.md + USER.md
- **Which is lightest? Which fits Rust best?**

### 2. Skill Format
- OpenViking: .abstract.md / .overview.md / SKILL.md per directory
- OpenSpace: Markdown with embedded code + SQLite metadata
- Hermes: SKILL.md + YAML frontmatter + scripts/references/assets dirs
- **Which is most extensible?**

### 3. Skill Evolution
- OpenViking: session-end extraction updates agent/skills/
- OpenSpace: CAPTURED → DERIVED → FIX versioning with quality metrics
- Hermes: background review thread, patch/edit actions
- **Which actually works best in practice?**

### 4. Retrieval
- OpenViking: L0/L1/L2 tiers + directory-recursive search
- OpenSpace: SQLite query + task-specific search
- Hermes: flat directory scan + FTS5 keyword search
- **Which is most token-efficient?**

### 5. Token Savings
- OpenViking: claims 80% reduction
- OpenSpace: claims 46% reduction with 4.2x quality improvement
- Hermes: no specific claims
- **Are these claims verifiable from code?**

### 6. MCP Compatibility
- OpenViking: no MCP
- OpenSpace: full MCP server
- Hermes: no MCP (custom skill_manage tool)
- **Does MCP matter for Wakey?**

### 7. Maturity
- Stars, commits, contributors, test coverage, docs quality
- Is it a research prototype or production-ready?
- Active maintenance?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/memory-systems-comparison.md

End with a clear RECOMMENDATION: which patterns to adopt for each dimension.
