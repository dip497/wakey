# Memory + Skill Systems Comparison: OpenViking vs OpenSpace vs Hermes

**Research Date:** 2026-04-03  
**Purpose:** Evaluate patterns for Wakey's context + skill system

## Executive Summary

| Dimension | OpenViking | OpenSpace | Hermes |
|-----------|------------|-----------|--------|
| **Memory Storage** | AGFS (Go) + VikingDB (vector) | SQLite + filesystem | SQLite FTS5 + MEMORY.md/USER.md |
| **Skill Format** | SKILL.md + .abstract.md/.overview.md | SKILL.md + SQLite metadata | SKILL.md + YAML frontmatter + subdirs |
| **Skill Evolution** | Session-end extraction → agent/skills/ | FIX / DERIVED / CAPTURED with quality metrics | Agent-driven create/patch/edit |
| **Retrieval** | L0/L1/L2 tiers + hierarchical directory search | BM25 + embedding hybrid | FTS5 keyword + flat directory scan |
| **Token Claims** | 80% reduction | 46% reduction, 4.2x quality | No specific claims |
| **MCP Support** | No (AGFS) | Yes (full MCP server) | No (custom skill_manage tool) |
| **Maturity** | Production (Volcano Engine) | Research (HKUDS) | Production (Nous Research) |
| **Language** | Python + Rust + Go | Python | Python |

---

## 1. Memory Storage

### OpenViking: Virtual Filesystem + Vector DB

**Architecture:**
```
viking://user/{user_space}/memories/
viking://agent/{agent_space}/memories/
viking://agent/{agent_space}/skills/
viking://resources/
```

**Storage layers:**
- **AGFS (Agent Global File System):** Go-based virtual filesystem, implements `viking://` URI scheme
- **VikingDB:** Vector database for L0/L1/L2 embeddings
- **QueueFS:** Async embedding pipeline

**Key code from `directories.py`:**
```python
class DirectoryDefinition:
    path: str          # Relative path, e.g., "memory/identity"
    abstract: str      # L0 summary
    overview: str      # L1 description
    children: List["DirectoryDefinition"]

PRESET_DIRECTORIES = {
    "user": DirectoryDefinition(
        path="",
        children=[
            DirectoryDefinition(path="memories", children=[
                DirectoryDefinition(path="preferences", ...),
                DirectoryDefinition(path="entities", ...),
                DirectoryDefinition(path="events", ...),
            ]),
        ],
    ),
    "agent": DirectoryDefinition(
        children=[
            DirectoryDefinition(path="memories", children=[
                DirectoryDefinition(path="cases", ...),
                DirectoryDefinition(path="patterns", ...),
            ]),
            DirectoryDefinition(path="instructions", ...),
            DirectoryDefinition(path="skills", ...),
        ],
    ),
}
```

**Pros:**
- Clean hierarchical model
- L0/L1/L2 tiered storage (abstract → overview → detail)
- Filesystem metaphor is intuitive

**Cons:**
- Requires Go + Rust components (C++ compiler for extensions)
- Heavy dependency stack
- Not pure Rust compatible

---

### OpenSpace: SQLite + Filesystem

**Architecture:**
- SQLite `skills` table with metadata
- SkillStore class manages quality metrics
- Filesystem for SKILL.md files

**Key code from `store.py`:**
```python
class SkillStore:
    """SQLite-backed skill metadata and quality tracking."""
    
    def save_record(self, record: SkillRecord) -> None:
        """Persist skill record with quality metrics."""
        
    def load_active(self) -> Dict[str, SkillRecord]:
        """Load all active skills with metrics."""
```

**Quality metrics tracked:**
```python
@dataclass
class SkillRecord:
    skill_id: str
    name: str
    total_selections: int
    total_applied: int
    total_completions: int
    total_fallbacks: int
    
    @property
    def applied_rate(self) -> float:
        return self.total_applied / max(1, self.total_selections)
    
    @property
    def completion_rate(self) -> float:
        return self.total_completions / max(1, self.total_applied)
```

**Pros:**
- Single SQLite file, no external dependencies
- Rich quality metrics built-in
- Easy to query and introspect

**Cons:**
- SQLite concurrency model (WAL mode needed)
- No L0/L1/L2 tiering

---

### Hermes: SQLite FTS5 + Markdown Files

**Architecture:**
- `~/.hermes/state.db` with FTS5 for session search
- `MEMORY.md` and `USER.md` for long-term memory
- `~/.hermes/skills/` for skill storage

**Key code from `session_search_tool.py`:**
```python
def session_search(query: str, db, limit: int = 3) -> str:
    """Search past sessions via FTS5, summarize with LLM."""
    raw_results = db.search_messages(
        query=query,
        role_filter=role_list,
        limit=50,
    )
    # ... LLM summarization of top sessions
```

**Memory files:**
- `MEMORY.md` - Agent's accumulated knowledge
- `USER.md` - User profile and preferences
- Session transcripts in SQLite FTS5

**Pros:**
- Human-readable markdown files
- FTS5 provides fast keyword search
- Simple, minimal dependencies

**Cons:**
- No semantic search (keyword only)
- No tiered abstraction
- Memory files can grow unbounded

---

### Recommendation: Memory Storage

**For Wakey (Rust, <20MB RAM):**

1. **Adopt OpenSpace's SQLite pattern** for skill metadata and quality metrics
2. **Add OpenViking's L0/L1 tiering** as virtual columns (not separate files)
3. **Use Hermes's human-readable markdown** for skill content

**Proposed schema:**
```sql
CREATE TABLE skills (
    skill_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    content TEXT,           -- Full SKILL.md content
    abstract TEXT,          -- L0: ~100 char summary
    overview TEXT,          -- L1: ~500 char summary
    generation INTEGER DEFAULT 0,
    parent_skill_ids TEXT,  -- JSON array
    quality_score REAL,
    created_at INTEGER,
    updated_at INTEGER
);

CREATE VIRTUAL TABLE skills_fts USING fts5(
    skill_id, name, description, abstract, overview,
    content='skills',
    content_rowid='rowid'
);
```

---

## 2. Skill Format

### OpenViking: SKILL.md + Abstract/Overview Files

**Structure:**
```
skills/
├── my-skill/
│   ├── SKILL.md           # Full skill content
│   ├── .abstract.md       # L0 summary (generated)
│   └── .overview.md       # L1 overview (generated)
```

**Frontmatter parsing from `skill_loader.py`:**
```python
FRONTMATTER_PATTERN = re.compile(r"^---\s*\n(.*?)\n---\s*\n(.*)$", re.DOTALL)

@classmethod
def parse(cls, content: str, source_path: str = "") -> Dict[str, Any]:
    frontmatter, body = cls._split_frontmatter(content)
    meta = yaml.safe_load(frontmatter)
    return {
        "name": meta["name"],
        "description": meta["description"],
        "content": body.strip(),
        "allowed_tools": meta.get("allowed-tools", []),
        "tags": meta.get("tags", []),
    }
```

**Pros:**
- Clean separation of L0/L1/L2
- Generated abstract/overview files

**Cons:**
- Multiple files per skill
- Sync issues between content and summaries

---

### OpenSpace: SKILL.md + SQLite Metadata

**Structure:**
```
skills/
├── my-skill/
│   ├── SKILL.md
│   └── .skill_id          # Persistent unique ID
```

**Key from `registry.py`:**
```python
@dataclass
class SkillMeta:
    skill_id: str          # Unique — persisted in .skill_id sidecar
    name: str              # Human-readable name
    description: str
    path: Path             # Absolute path to SKILL.md

def _read_or_create_skill_id(name: str, skill_dir: Path) -> str:
    """Read or create persistent skill_id from .skill_id sidecar."""
    id_file = skill_dir / SKILL_ID_FILENAME
    if id_file.exists():
        return id_file.read_text().strip()
    new_id = f"{name}__imp_{uuid.uuid4().hex[:8]}"
    id_file.write_text(new_id + "\n")
    return new_id
```

**Pros:**
- Persistent skill_id survives moves
- Single SKILL.md file
- SQLite for rich metadata

**Cons:**
- Sidecar file to manage
- No L0/L1 tiering in files

---

### Hermes: SKILL.md + Subdirectories

**Structure:**
```
~/.hermes/skills/
├── my-skill/
│   ├── SKILL.md
│   ├── references/
│   ├── templates/
│   ├── scripts/
│   └── assets/
```

**Example from `subagent-driven-development/SKILL.md`:**
```yaml
---
name: subagent-driven-development
description: Use when executing implementation plans with independent tasks.
version: 1.1.0
author: Hermes Agent
license: MIT
metadata:
  hermes:
    tags: [delegation, subagent, implementation]
    related_skills: [writing-plans, requesting-code-review]
---

# Subagent-Driven Development

## Overview
Execute implementation plans by dispatching fresh subagents...

## When to Use
...

## The Process
### 1. Read and Parse Plan
...

## Red Flags — Never Do These
...
```

**Pros:**
- Rich subdirectory support (scripts, templates, references)
- Extended metadata in frontmatter
- Self-contained skill packages

**Cons:**
- Larger disk footprint
- More complex loading

---

### Recommendation: Skill Format

**For Wakey:**

1. **Adopt Hermes's extended frontmatter** with name, description, tags, version
2. **Use OpenSpace's .skill_id sidecar** for persistent identity
3. **Generate L0/L1 abstractions** on load (not as separate files)
4. **Support optional subdirectories** (scripts/, references/)

**Proposed format:**
```yaml
---
name: my-skill
description: Brief description for selection
version: 1.0.0
tags: [category, domain]
wakey:
  tier: L2           # L0/L1/L2 detail level
  triggers: ["keyword", "pattern"]
  quality_score: 0.85
---

# Skill Title

## When to Use
...

## Steps
...
```

---

## 3. Skill Evolution

### OpenViking: Session-End Extraction

**Flow:**
1. Session completes
2. LLM extracts patterns from conversation
3. New skills written to `agent/skills/`

**No explicit evolution types** - just extraction and creation.

---

### OpenSpace: FIX / DERIVED / CAPTURED

**Evolution types from `evolver.py`:**

```python
class EvolutionType(str, Enum):
    FIX = "fix"           # In-place repair of broken skill
    DERIVED = "derived"   # New enhanced version from existing
    CAPTURED = "captured" # Brand new skill from observed pattern
```

**Trigger sources:**
```python
class EvolutionTrigger(str, Enum):
    ANALYSIS = "analysis"           # Post-execution analysis
    TOOL_DEGRADATION = "tool_degradation"  # Tool quality dropped
    METRIC_MONITOR = "metric_monitor"      # Periodic health check
```

**Evolution flow:**
```python
async def evolve(self, ctx: EvolutionContext) -> Optional[SkillRecord]:
    if evo_type == EvolutionType.FIX:
        return await self._evolve_fix(ctx)      # Same dir, new version
    elif evo_type == EvolutionType.DERIVED:
        return await self._evolve_derived(ctx)  # New dir, parent lineage
    elif evo_type == EvolutionType.CAPTURED:
        return await self._evolve_captured(ctx) # New dir, no parent
```

**Lineage tracking:**
```python
@dataclass
class SkillLineage:
    origin: SkillOrigin  # IMPORTED, FIXED, DERIVED, CAPTURED
    generation: int
    parent_skill_ids: List[str]
    source_task_id: Optional[str]
    change_summary: str
    content_diff: str
    created_by: str  # Model name
```

**Pros:**
- Three clear evolution types
- Lineage tracking for audit trail
- Multiple trigger sources
- Quality metrics drive evolution

**Cons:**
- Complex implementation
- Requires LLM for evolution decisions

---

### Hermes: Agent-Driven Create/Patch/Edit

**Actions from `skill_manager_tool.py`:**

```python
def skill_manage(action: str, name: str, ...) -> str:
    if action == "create":
        return _create_skill(name, content, category)
    elif action == "edit":
        return _edit_skill(name, content)
    elif action == "patch":
        return _patch_skill(name, old_string, new_string)
    elif action == "delete":
        return _delete_skill(name)
    elif action == "write_file":
        return _write_file(name, file_path, file_content)
    elif action == "remove_file":
        return _remove_file(name, file_path)
```

**Security scan on all changes:**
```python
def _security_scan_skill(skill_dir: Path) -> Optional[str]:
    """Scan a skill directory after write. Returns error if blocked."""
    result = scan_skill(skill_dir, source="agent-created")
    allowed, reason = should_allow_install(result)
    if allowed is False:
        return f"Security scan blocked: {reason}"
    return None
```

**Pros:**
- Simple, intuitive actions
- Agent has full control
- Security scanning built-in

**Cons:**
- No automatic evolution triggers
- No quality-based decisions
- No lineage tracking

---

### Recommendation: Skill Evolution

**For Wakey:**

1. **Adopt OpenSpace's evolution types** (FIX, DERIVED, CAPTURED)
2. **Add Hermes's security scanning** for all skill changes
3. **Use OpenSpace's lineage tracking** for audit trail
4. **Implement quality triggers** from OpenSpace

**Proposed evolution triggers:**
- **Post-task analysis:** If task succeeded with novel pattern → CAPTURED candidate
- **Quality degradation:** If skill success rate < 50% → FIX candidate
- **Tool change:** If referenced tool changes → review needed

---

## 4. Retrieval

### OpenViking: Hierarchical Directory Search

**From `hierarchical_retriever.py`:**

```python
class HierarchicalRetriever:
    LEVEL_URI_SUFFIX = {0: ".abstract.md", 1: ".overview.md"}
    GLOBAL_SEARCH_TOPK = 10
    HOTNESS_ALPHA = 0.2  # Weight for hotness in final ranking

    async def retrieve(self, query: TypedQuery, limit: int = 5) -> QueryResult:
        # Step 1: Global vector search
        global_results = await self._global_vector_search(query, ...)
        
        # Step 2: Determine starting directories
        root_uris = self._get_root_uris_for_type(query.context_type)
        
        # Step 3: Recursive search with score propagation
        candidates = await self._recursive_search(
            starting_points=starting_points,
            query_vector=query_vector,
            ...
        )
        
        # Step 4: Rerank and convert
        return QueryResult(matched_contexts=matched[:limit])
```

**Score propagation:**
```python
# Parent score influences child ranking
final_score = alpha * child_score + (1 - alpha) * parent_score
```

**Hotness scoring:**
```python
def hotness_score(active_count: int, updated_at: datetime) -> float:
    """Higher score for frequently-accessed, recently-updated contexts."""
    recency = 1.0 / (1.0 + hours_since_update / 24.0)
    return active_count * recency
```

**Pros:**
- Hierarchical navigation matches mental model
- Score propagation from directories
- Hotness boost for frequently-used items

**Cons:**
- Requires vector DB
- Complex recursive search
- Heavy dependencies

---

### OpenSpace: BM25 + Embedding Hybrid

**From `skill_ranker.py`:**

```python
class SkillRanker:
    PREFILTER_THRESHOLD = 10  # Use BM25 when skills > 10
    SKILL_EMBEDDING_MODEL = "openai/text-embedding-3-small"

    def hybrid_rank(self, query: str, candidates: List[SkillCandidate], top_k: int = 10):
        # Stage 1: BM25 rough-rank
        bm25_top = self._bm25_rank(query, candidates, top_k * 3)
        
        # Stage 2: Embedding re-rank on BM25 candidates
        emb_results = self._embedding_rank(query, bm25_top, top_k)
        
        return emb_results if emb_results else bm25_top[:top_k]
```

**Skill selection with LLM:**
```python
async def select_skills_with_llm(self, task: str, llm_client, max_skills: int = 2):
    # Pre-filter with BM25+embedding when > 10 skills
    if len(available) > PREFILTER_THRESHOLD:
        available = self._prefilter_skills(task, available, max_skills)
    
    # Build catalog with quality stats
    for s in available:
        q = skill_quality.get(s.skill_id)
        catalog_lines.append(f"- **{s.skill_id}**: {s.description} (success {rate:.0%})")
    
    # LLM selects from catalog
    prompt = self._build_skill_selection_prompt(task, catalog, max_skills)
    resp = await llm_client.complete(prompt)
    return parse_selection(resp)
```

**Pros:**
- Two-stage retrieval is efficient
- Quality signals in selection prompt
- Pre-filter reduces LLM cost

**Cons:**
- Requires embedding API
- External LLM for selection

---

### Hermes: FTS5 + Flat Scan

**From `session_search_tool.py`:**

```python
def session_search(query: str, db, limit: int = 3) -> str:
    # FTS5 search
    raw_results = db.search_messages(query=query, limit=50)
    
    # Summarize each matching session
    for session_id, match_info in seen_sessions.items():
        messages = db.get_messages_as_conversation(session_id)
        conversation_text = _format_conversation(messages)
        summary = await _summarize_session(conversation_text, query, session_meta)
        summaries.append({"session_id": session_id, "summary": summary})
    
    return json.dumps({"results": summaries})
```

**For skills:**
```python
def _find_skill(name: str) -> Optional[Dict[str, Any]]:
    """Find a skill by name via directory scan."""
    for skill_md in SKILLS_DIR.rglob("SKILL.md"):
        if skill_md.parent.name == name:
            return {"path": skill_md.parent}
    return None
```

**Pros:**
- Simple, no vector DB needed
- FTS5 is fast for keywords
- LLM summarization provides context

**Cons:**
- No semantic matching
- Flat directory scan is O(n)
- No tiered retrieval

---

### Recommendation: Retrieval

**For Wakey (must be lightweight):**

1. **SQLite FTS5 as primary** (no vector DB)
2. **Add BM25 ranking** like OpenSpace (no embedding required)
3. **L0/L1 tiered loading** for token efficiency
4. **Quality-weighted selection** from OpenSpace

**Proposed retrieval flow:**
```
1. Query FTS5 for candidate skills
2. Rank by: (BM25 score * quality_weight) + hotness_boost
3. Return top-k with L1 overview (not full L2 content)
4. Load L2 content only when skill is selected
```

---

## 5. Token Savings Claims

### OpenViking: "80% Reduction"

**Claimed mechanism:**
- L0/L1/L2 tiered loading
- Load abstract (L0) first, expand on demand
- "Significantly saving costs" in README

**Verifiable?** No benchmark data in repo. Claim is marketing.

---

### OpenSpace: "46% Reduction, 4.2x Quality"

**Claimed mechanism:**
- Skill reuse avoids reasoning from scratch
- Quality metrics track improvement

**Verifiable?** Yes - GDPVal benchmark in repo:
```python
# From README:
# On 50 professional tasks across 6 industries:
# - 46% fewer tokens than baseline (ClawWork)
# - 4.2x higher earnings (economic value)
# - Consistent wins across all fields
```

**Benchmark includes:**
- Building payroll calculators
- Tax return preparation
- Legal memorandum drafting
- Compliance forms

---

### Hermes: No Specific Claims

**No token savings claims in README or docs.**

---

### Recommendation: Token Efficiency

**For Wakey:**

1. **Track quality metrics** like OpenSpace
2. **Implement L0/L1 lazy loading** like OpenViking
3. **Benchmark early** to validate approach

**Key metrics to track:**
- Tokens saved via skill reuse
- Skill selection success rate
- Task completion rate with/without skills

---

## 6. MCP Compatibility

### OpenViking: No MCP

Uses custom AGFS protocol with `viking://` URIs. No MCP support.

---

### OpenSpace: Full MCP Server

**From `mcp_server.py`:**
```python
mcp = FastMCP("OpenSpace")

@mcp.tool()
async def execute_task(task: str, ...) -> str:
    """Delegate a task with auto-skill-registration and auto-evolution."""
    
@mcp.tool()
async def search_skills(query: str, ...) -> str:
    """Search across local & cloud skills."""
    
@mcp.tool()
async def fix_skill(skill_id: str, issue: str) -> str:
    """Manually fix a broken skill."""
    
@mcp.tool()
async def upload_skill(skill_id: str) -> str:
    """Upload a local skill to cloud."""
```

**Pros:**
- Full MCP integration
- Can be used by any MCP client
- Tool-based skill management

---

### Hermes: Custom Tool (No MCP)

Uses `skill_manage` tool for skill operations. No MCP server.

```python
SKILL_MANAGE_SCHEMA = {
    "name": "skill_manage",
    "description": "Manage skills (create, update, delete)...",
    "parameters": {
        "type": "object",
        "properties": {
            "action": {"enum": ["create", "patch", "edit", "delete", ...]},
            "name": {"type": "string"},
            "content": {"type": "string"},
            ...
        }
    }
}
```

---

### Does MCP Matter for Wakey?

**Yes, for integration:**
- Wakey could expose skills via MCP
- Other agents could use Wakey's skills
- Wakey could use external MCP tools

**But not required for core functionality:**
- Internal skill system can be independent
- MCP can be a separate layer

---

## 7. Maturity Assessment

### OpenViking

| Metric | Value |
|--------|-------|
| Organization | Volcano Engine (ByteDance) |
| Language | Python + Rust + Go |
| Components | AGFS (Go), CLI (Rust), Core (Python) |
| Docs | Extensive (EN/CN/JP) |
| Tests | Yes (tests/ directory) |
| Status | Production-ready |

**Pros:**
- Backed by major company
- Multi-language support
- Comprehensive docs

**Cons:**
- Complex build (requires Go, Rust, C++)
- Heavy dependency stack

---

### OpenSpace

| Metric | Value |
|--------|-------|
| Organization | HKUDS (research lab) |
| Language | Python |
| Components | MCP server, skill engine, grounding agent |
| Docs | Good (README + code comments) |
| Tests | Limited |
| Status | Research prototype |

**Pros:**
- Novel evolution system
- Quality metrics built-in
- MCP support

**Cons:**
- Research project (may not be maintained)
- Limited test coverage
- Some incomplete features

---

### Hermes

| Metric | Value |
|--------|-------|
| Organization | Nous Research |
| Language | Python |
| Components | CLI, gateway, tools, skills |
| Docs | Excellent (website + code) |
| Tests | Yes (tests/ directory) |
| Status | Production-ready |

**Pros:**
- Active development
- Production deployment
- Rich feature set

**Cons:**
- Python-only
- No semantic search
- No evolution triggers

---

## Final Recommendations

### Architecture for Wakey

```
┌─────────────────────────────────────────────────────────────┐
│                    wakey-context (crate)                     │
├─────────────────────────────────────────────────────────────┤
│ Storage:                                                    │
│   - SQLite (single file, < 5MB)                            │
│   - FTS5 for keyword search                                 │
│   - No external vector DB                                   │
├─────────────────────────────────────────────────────────────┤
│ Skill Format:                                               │
│   - SKILL.md with YAML frontmatter                          │
│   - .skill_id sidecar for persistent identity               │
│   - L0 (abstract) / L1 (overview) generated on load         │
│   - Optional scripts/ references/ subdirs                   │
├─────────────────────────────────────────────────────────────┤
│ Evolution:                                                  │
│   - FIX: In-place repair                                    │
│   - DERIVED: Enhanced version                               │
│   - CAPTURED: New skill from pattern                        │
│   - Lineage tracking (parent_skill_ids, generation)         │
│   - Quality metrics drive evolution triggers                │
├─────────────────────────────────────────────────────────────┤
│ Retrieval:                                                  │
│   - FTS5 keyword search (no embedding)                      │
│   - BM25 ranking                                            │
│   - Quality-weighted selection                              │
│   - L0 → L1 → L2 lazy loading                               │
└─────────────────────────────────────────────────────────────┘
```

### What to Adopt From Each

| From | What |
|------|------|
| **OpenViking** | L0/L1/L2 tiered abstraction, filesystem metaphor, hotness scoring |
| **OpenSpace** | FIX/DERIVED/CAPTURED evolution, quality metrics, lineage tracking, MCP interface |
| **Hermes** | Extended frontmatter, subdirectory support, security scanning, human-readable markdown |

### What to Avoid

| From | What |
|------|------|
| **OpenViking** | AGFS (Go dependency), separate abstract/overview files, complex build |
| **OpenSpace** | Research-grade code, incomplete features, embedding dependency |
| **Hermes** | Keyword-only search, no evolution triggers, no quality tracking |

### Implementation Priority

1. **P0: Core storage** - SQLite + FTS5 + skill tables
2. **P0: Skill format** - SKILL.md parser + frontmatter
3. **P1: Retrieval** - FTS5 search + BM25 ranking
4. **P1: Quality metrics** - Track selection/applied/completion rates
5. **P2: Evolution** - FIX/DERIVED/CAPTURED actions
6. **P2: MCP interface** - Expose skills as MCP tools

---

## References

- OpenViking: https://github.com/volcengine/OpenViking
- OpenSpace: https://github.com/HKUDS/OpenSpace
- Hermes: https://github.com/NousResearch/hermes-agent
- GDPVal Benchmark: https://github.com/HKUDS/GDPVal