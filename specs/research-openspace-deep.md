# Deep Research: OpenSpace Internals

## Goal
Read the ACTUAL OpenSpace codebase. Evaluate if it's production-ready, what patterns we can adopt, and how it compares to OpenViking + Hermes at code level.

## Repo
Clone https://github.com/HKUDS/OpenSpace to /tmp/openspace-research/

## Focus Areas (read actual Python code)

### 1. Skill Evolution Engine
- How does CAPTURED → DERIVED → FIX versioning work?
- How are quality metrics tracked (success rate, error count)?
- How does auto-fix work when a skill fails?
- Find the actual evolution loop code

### 2. SQLite Skill Registry
- What's the schema?
- How are skills indexed and searched?
- How does version lineage work in the DB?
- Compare with OpenViking's filesystem approach

### 3. MCP Integration
- How does it expose skills as MCP tools?
- How does skill discovery work via MCP?
- What's the MCP server implementation?

### 4. Collective Intelligence (Cloud)
- How does skill sharing work?
- How are access controls implemented?
- Is the cloud part needed or is local-only viable?

### 5. GDPVal Benchmark
- How are the 4.2x income and 46% token savings measured?
- Is the benchmark reproducible?
- What tasks were tested?

### 6. Code Quality Assessment
- How many lines of code? How well structured?
- Test coverage?
- Documentation quality?
- Is it production-ready or research prototype?
- Dependencies — heavy or light?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/openspace-deep.md

Include: actual code snippets, schema dumps, honest quality assessment.
Compare with OpenViking and Hermes on each point.
