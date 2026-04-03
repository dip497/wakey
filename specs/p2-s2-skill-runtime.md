# P2-S2: Skill Runtime (wakey-skills)

## Goal
Load, run, track, and evolve skills. Hermes SKILL.md format + OpenSpace quality metrics + petgraph DAG.

## Architecture

```
~/.wakey/context/agent/skills/
├── deploy-app/
│   ├── SKILL.md              # Hermes format (YAML frontmatter + markdown)
│   ├── scripts/              # Optional executable scripts
│   └── references/           # Optional reference docs
├── fix-lint/
│   └── SKILL.md
└── agent-supervisor/
    └── SKILL.md
```

## What to implement

### 1. Skill format parser (src/format.rs)
Parse SKILL.md with YAML frontmatter:
```yaml
---
name: deploy-app
description: Deploy application to production
version: 1.0.0
dependencies: [fix-lint]        # Other skills this depends on
tags: [devops, deployment]
platforms: [linux]
---

# Deploy App

## When to Use
When user asks to deploy...

## Procedure
1. Run tests
2. Build release
3. Deploy

## Pitfalls
- Check .env exists before deploy

## Verification
curl the health endpoint
```

- `SkillManifest` struct from frontmatter
- `SkillContent` struct for full parsed skill
- `parse_skill(path: &Path)` → WakeyResult<SkillContent>
- Validate required fields (name, description)

### 2. Skill registry (src/registry.rs)
- `SkillRegistry::new(skills_dir: &Path, index_db: &Path)`
- `scan()` — walk skills directory, parse all SKILL.md, index into SQLite
- `find(query: &str)` → Vec<SkillMatch> — FTS5 search by name/description/tags
- `get(name: &str)` → Option<SkillContent> — load specific skill
- `list()` → Vec<SkillManifest> — all registered skills (L0 only)
- Auto-rescan when files change (check mtime)

### 3. Skill DAG (src/dag.rs) — petgraph
- Build dependency graph from `dependencies` field in frontmatter
- `SkillDag::build(skills: &[SkillManifest])` → DiGraph
- `resolve_order(skill_name: &str)` → Vec<String> — topological sort
- `detect_cycles()` → Vec<Vec<String>> — Tarjan's SCC
- `find_orphans()` → Vec<String> — skills with broken deps

### 4. Quality tracking (src/quality.rs) — OpenSpace pattern
- `record_selection(skill_id)` — skill was chosen by LLM
- `record_applied(skill_id)` — skill was actually used  
- `record_completion(skill_id)` — task completed with this skill
- `record_fallback(skill_id)` — skill failed, had to fall back
- `get_metrics(skill_id)` → SkillMetrics
- `get_degraded()` → Vec<String> — skills with <50% completion rate
- All writes go to skill_metrics table in index.db

### 5. Skill evolution (src/evolution.rs) — OpenSpace FIX/DERIVED/CAPTURED
- `EvolutionType::Fix` — repair broken skill (same name, new version)
- `EvolutionType::Derived` — enhanced version of existing skill
- `EvolutionType::Captured` — brand new skill from execution pattern
- `evolve(skill_id, evolution_type, new_content)` — create new version
- Track lineage in skill_lineage table
- Deactivate old version (is_active=false)

### 6. Learning triggers (src/learning.rs) — Hermes pattern
- `LearningTracker::new()`
- Count tool iterations since last skill creation
- After N iterations (configurable, default 10): trigger skill review
- Review prompt: "Should we create/update a skill from this conversation?"
- Review runs in background (non-blocking, best-effort)
- Triggers: complex task success (5+ tools), errors overcome, user correction

## Dependencies
```toml
# Already in workspace
petgraph = { workspace = true }
wakey-context = { workspace = true }  # Uses SQLite index + filesystem
```

## Read first
- docs/research/hermes-deep.md #1 (skill_manage, SKILL.md format)
- docs/research/openspace-deep.md #1-2 (evolution engine, SQLite schema)
- docs/research/memory-systems-comparison.md (final recommendations)
- crates/wakey-skills/AGENTS.md

## Verify
```bash
cargo check --workspace
cargo test --package wakey-skills
```

## Acceptance criteria
- SKILL.md parser extracts frontmatter + content
- Registry scans directory and indexes skills
- FTS5 search finds skills by name/description/tags
- DAG builds from dependencies, detects cycles
- Quality metrics track selections/applied/completions/fallbacks
- Evolution creates new versions with lineage
- Learning tracker counts iterations and triggers review
- cargo check + cargo test pass
