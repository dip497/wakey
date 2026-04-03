# OpenSpace Deep Research

> Code-level analysis of OpenSpace internals. Based on actual source code from https://github.com/HKUDS/OpenSpace (commit: April 2026).

## Executive Summary

**OpenSpace is a research prototype, not production software.** It's an impressive academic project with novel ideas (skill evolution, version lineage), but has significant gaps for production use: zero test coverage, heavy dependencies, and an architecture that assumes cloud connectivity.

| Metric | Value |
|--------|-------|
| Lines of Python | ~45,000 |
| Files | 151 `.py` files |
| Test Coverage | **0%** (no test files) |
| Dependencies | ~15 core, platform-specific extras |
| Min Python | 3.12 |

---

## 1. Skill Evolution Engine

### 1.1 Evolution Types (Version DAG Model)

OpenSpace implements a **version DAG** (Directed Acyclic Graph) for skill lineage:

```python
# openspace/skill_engine/types.py

class EvolutionType(str, Enum):
    FIX      = "fix"       # In-place repair, same name/path
    DERIVED  = "derived"   # New skill from existing (enhance/merge)
    CAPTURED = "captured"  # Brand new skill from execution pattern

class SkillOrigin(str, Enum):
    IMPORTED = "imported"  # Initial import, no parent
    CAPTURED = "captured"  # Captured from execution
    DERIVED  = "derived"   # Enhanced/composed from existing
    FIXED    = "fixed"     # New version of same skill
```

**Key Insight**: FIXED creates a new `SkillRecord` with the same `name` and `path` but new `skill_id`. The parent is deactivated (`is_active=False`). This is **in-place versioning** at the DB level, not file-level.

### 1.2 Evolution Triggers

Three trigger sources defined in `evolver.py`:

```python
class EvolutionTrigger(str, Enum):
    ANALYSIS         = "analysis"           # Post-execution LLM analysis
    TOOL_DEGRADATION = "tool_degradation"   # Tool quality drops
    METRIC_MONITOR   = "metric_monitor"     # Periodic health check
```

**Trigger 1 (Post-Analysis)**: After each task, `ExecutionAnalyzer.analyze_execution()` runs an LLM analysis that may suggest:
- FIX: Skill was selected but didn't work
- DERIVED: Skill could be enhanced or merged
- CAPTURED: Novel pattern observed

**Trigger 2 (Tool Degradation)**: `ToolQualityManager` tracks per-tool success rates. When a tool's recent success rate drops, all skills depending on that tool are flagged for FIX evolution.

**Trigger 3 (Metric Monitor)**: Periodic scan of skill health metrics (completion rate, fallback rate). Skills with poor metrics are flagged for evolution.

### 1.3 Quality Metrics

Tracked per-skill in `SkillRecord`:

```python
@dataclass
class SkillRecord:
    total_selections: int = 0    # Times selected by LLM
    total_applied: int = 0       # Times actually used
    total_completions: int = 0   # Times task completed
    total_fallbacks: int = 0     # Times skill unusable
    
    @property
    def applied_rate(self) -> float:
        return self.total_applied / self.total_selections if self.total_selections else 0.0
    
    @property
    def completion_rate(self) -> float:
        return self.total_completions / self.total_applied if self.total_applied else 0.0
    
    @property
    def effective_rate(self) -> float:
        return self.total_completions / self.total_selections if self.total_selections else 0.0
```

These are **atomic SQL counters**, updated via `SkillStore.record_analysis()`:

```python
# Atomic counter updates in SQL
self._conn.execute(
    """
    UPDATE skill_records SET
        total_selections  = total_selections + 1,
        total_applied     = total_applied + ?,
        total_completions = total_completions + ?,
        total_fallbacks   = total_fallbacks + ?,
        last_updated      = ?
    WHERE skill_id = ?
    """,
    (applied, completed, fallback, now_iso, j.skill_id),
)
```

### 1.4 Evolution Loop (Agent Loop)

The evolution uses a **token-driven agent loop** in `SkillEvolver._run_evolution_loop()`:

```python
_MAX_EVOLUTION_ITERATIONS = 5   # Max tool-calling rounds

for iteration in range(_MAX_EVOLUTION_ITERATIONS):
    is_last = iteration == _MAX_EVOLUTION_ITERATIONS - 1
    
    # Final round: disable tools, force decision
    if is_last:
        messages.append({
            "role": "system",
            "content": "This is your FINAL round — no more tool calls allowed. "
                       "You MUST output the skill edit content now."
        })
    
    result = await self._llm_client.complete(
        messages=messages,
        tools=evolution_tools if not is_last else None,
        execute_tools=True,
    )
    
    # Check for completion tokens
    if EVOLUTION_COMPLETE in content or EVOLUTION_FAILED in content:
        return self._parse_evolution_output(content)
```

**Key Design**: The LLM can call tools (read_file, web_search, shell) during iterations 1-N-1. Final iteration forces output. This is more flexible than Hermes's single-shot approach.

### 1.5 Apply-Retry Cycle

After LLM produces edit content, a retry loop handles patch failures:

```python
_MAX_EVOLUTION_ATTEMPTS = 3

for attempt in range(_MAX_EVOLUTION_ATTEMPTS):
    edit_result = apply_fn(current_content)  # fix_skill / derive_skill / create_skill
    
    if edit_result.ok:
        validation_error = _validate_skill_dir(skill_dir)
        if not validation_error:
            return edit_result
    
    # Feed error back to LLM for correction
    messages.append({
        "role": "user",
        "content": f"The edit failed: {edit_result.error}\n\n"
                   f"Please generate a corrected version."
    })
    
    # Get corrected content from LLM
    corrected = await self._llm_client.complete(messages=messages)
    current_content = corrected["message"]["content"]
```

---

## 2. SQLite Skill Registry

### 2.1 Schema (Full DDL)

```sql
-- Main skill records table
CREATE TABLE skill_records (
    skill_id               TEXT PRIMARY KEY,
    name                   TEXT NOT NULL,
    description            TEXT NOT NULL DEFAULT '',
    path                   TEXT NOT NULL DEFAULT '',
    is_active              INTEGER NOT NULL DEFAULT 1,
    category               TEXT NOT NULL DEFAULT 'workflow',
    visibility             TEXT NOT NULL DEFAULT 'private',
    creator_id             TEXT NOT NULL DEFAULT '',
    lineage_origin         TEXT NOT NULL DEFAULT 'imported',
    lineage_generation     INTEGER NOT NULL DEFAULT 0,
    lineage_source_task_id TEXT,
    lineage_change_summary TEXT NOT NULL DEFAULT '',
    lineage_content_diff   TEXT NOT NULL DEFAULT '',
    lineage_content_snapshot TEXT NOT NULL DEFAULT '{}',
    lineage_created_at     TEXT NOT NULL,
    lineage_created_by     TEXT NOT NULL DEFAULT '',
    total_selections       INTEGER NOT NULL DEFAULT 0,
    total_applied          INTEGER NOT NULL DEFAULT 0,
    total_completions      INTEGER NOT NULL DEFAULT 0,
    total_fallbacks        INTEGER NOT NULL DEFAULT 0,
    first_seen             TEXT NOT NULL,
    last_updated           TEXT NOT NULL
);

-- Many-to-many lineage parents
CREATE TABLE skill_lineage_parents (
    skill_id        TEXT NOT NULL REFERENCES skill_records(skill_id) ON DELETE CASCADE,
    parent_skill_id TEXT NOT NULL,
    PRIMARY KEY (skill_id, parent_skill_id)
);

-- One analysis per task
CREATE TABLE execution_analyses (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id                 TEXT NOT NULL UNIQUE,
    timestamp               TEXT NOT NULL,
    task_completed          INTEGER NOT NULL DEFAULT 0,
    execution_note          TEXT NOT NULL DEFAULT '',
    tool_issues             TEXT NOT NULL DEFAULT '[]',
    candidate_for_evolution INTEGER NOT NULL DEFAULT 0,
    evolution_suggestions   TEXT NOT NULL DEFAULT '[]',
    analyzed_by             TEXT NOT NULL DEFAULT '',
    analyzed_at             TEXT NOT NULL
);

-- Per-skill judgments within an analysis
CREATE TABLE skill_judgments (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    analysis_id    INTEGER NOT NULL REFERENCES execution_analyses(id) ON DELETE CASCADE,
    skill_id       TEXT NOT NULL,
    skill_applied  INTEGER NOT NULL DEFAULT 0,
    note           TEXT NOT NULL DEFAULT '',
    UNIQUE(analysis_id, skill_id)
);

-- Tool dependencies
CREATE TABLE skill_tool_deps (
    skill_id TEXT NOT NULL REFERENCES skill_records(skill_id) ON DELETE CASCADE,
    tool_key TEXT NOT NULL,
    critical INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (skill_id, tool_key)
);

-- Tags
CREATE TABLE skill_tags (
    skill_id TEXT NOT NULL REFERENCES skill_records(skill_id) ON DELETE CASCADE,
    tag      TEXT NOT NULL,
    PRIMARY KEY (skill_id, tag)
);
```

### 2.2 Content Snapshots & Diffs

**Critical for Wakey**: OpenSpace stores **full directory snapshots** in `lineage_content_snapshot`:

```python
# SkillLineage in types.py
@dataclass
class SkillLineage:
    content_diff: str = ""  # Combined unified diff
    content_snapshot: Dict[str, str] = field(default_factory=dict)  # {relative_path: content}
```

**Snapshot Policy**:
- IMPORTED/CAPTURED: Add-all diff (every line prefixed with `+`)
- FIXED (1 parent): Unified diff vs parent
- DERIVED (multi-parent): Empty diff (composition is creative, not patchable)

This means **every skill version is fully recoverable** from the DB, even if files are deleted.

### 2.3 Comparison with OpenViking

| Feature | OpenSpace | OpenViking |
|---------|-----------|------------|
| Storage | SQLite DB | Filesystem (L0/L1/L2 JSON files) |
| Versioning | DB-level (SkillRecord nodes) | File-based (no versioning) |
| Search | BM25 + embedding + SQL queries | JSON file scan |
| Lineage | Full DAG with parents/snapshots | None |
| Quality tracking | Per-skill counters in DB | None |

**Wakey Consideration**: OpenSpace's SQLite approach is more suitable for complex lineage tracking, but OpenViking's filesystem approach is simpler and more portable. For Wakey, we could combine: filesystem for skills + SQLite for lineage/quality.

---

## 3. MCP Integration

### 3.1 MCP Server Implementation

OpenSpace exposes **4 MCP tools** via FastMCP:

```python
# openspace/mcp_server.py

mcp = FastMCP("OpenSpace")

@mcp.tool()
async def execute_task(
    task: str,
    workspace_dir: str | None = None,
    max_iterations: int | None = None,
    skill_dirs: list[str] | None = None,
    search_scope: str = "all",
) -> str:
    """Execute a task with OpenSpace's full grounding engine.
    
    1. Auto-register bot skills from skill_dirs
    2. Search for relevant skills (local + cloud)
    3. Attempt skill-guided execution → fallback to pure tools
    4. Auto-analyze → auto-evolve (FIX/DERIVED/CAPTURED)
    """

@mcp.tool()
async def search_skills(query: str, source: str = "all", limit: int = 20) -> str:
    """Hybrid BM25 + embedding search across local + cloud."""

@mcp.tool()
async def fix_skill(skill_dir: str, direction: str) -> str:
    """Manually fix a broken skill (FIX only)."""

@mcp.tool()
async def upload_skill(skill_dir: str, visibility: str = "public") -> str:
    """Upload a local skill to the cloud."""
```

### 3.2 Skill Discovery via MCP

Skills are discovered via **multi-tier directory scan**:

```python
# Priority order:
# 1. OPENSPACE_HOST_SKILL_DIRS env (highest)
# 2. config_grounding.json → skills.skill_dirs
# 3. openspace/skills/ builtin (lowest)

def _get_local_skill_registry():
    skill_paths: List[Path] = []
    
    host_dirs_raw = os.environ.get("OPENSPACE_HOST_SKILL_DIRS", "")
    if host_dirs_raw:
        for d in host_dirs_raw.split(","):
            p = Path(d.strip())
            if p.exists():
                skill_paths.append(p)
    
    # ... config file dirs ...
    
    builtin_skills = Path(__file__).resolve().parent / "skills"
    if builtin_skills.exists():
        skill_paths.append(builtin_skills)
    
    registry = SkillRegistry(skill_dirs=skill_paths)
    registry.discover()
    return registry
```

### 3.3 RetrieveSkillTool (Mid-Iteration Retrieval)

OpenSpace exposes a **mid-iteration skill retrieval tool** that agents can call during execution:

```python
# openspace/skill_engine/retrieve_tool.py

class RetrieveSkillTool(LocalTool):
    _name = "retrieve_skill"
    _description = (
        "Search for specialized skill guidance when the current approach "
        "isn't working or the task requires domain-specific knowledge."
    )
    
    async def _arun(self, query: str) -> str:
        # Uses same pipeline as initial selection:
        # quality filter → BM25+embedding → LLM plan-then-select
        selected, record = await self._skill_registry.select_skills_with_llm(
            query, llm_client=self._llm_client, max_skills=1
        )
        return self._skill_registry.build_context_injection(selected)
```

This is similar to Hermes's skill invocation but more dynamic — skills can be retrieved mid-task.

---

## 4. Collective Intelligence (Cloud)

### 4.1 Cloud Client

```python
# openspace/cloud/client.py

class OpenSpaceClient:
    """HTTP client for the cloud API."""
    
    def upload_skill(self, skill_dir: Path, visibility: str, origin: str, 
                     parent_skill_ids: List[str]) -> Dict:
        # 1. Stage artifact (multipart upload)
        artifact_id, file_count = self.stage_artifact(skill_path)
        
        # 2. Compute content diff (vs ancestor if applicable)
        content_diff = self._compute_content_diff(skill_dir, visibility, parents)
        
        # 3. Create record
        record_id = f"{name}__clo_{uuid.uuid4().hex[:8]}"
        return self.create_record({
            "record_id": record_id,
            "artifact_id": artifact_id,
            "origin": origin,
            "visibility": visibility,
            "parent_skill_ids": parents,
            "content_diff": content_diff,
        })
    
    def import_skill(self, skill_id: str, target_dir: Path) -> Dict:
        # 1. Fetch metadata
        record_data = self.fetch_record(skill_id)
        
        # 2. Download artifact zip
        zip_data = self.download_artifact(skill_id)
        
        # 3. Extract with path traversal protection
        self._extract_zip(zip_data, target_dir)
        
        # 4. Write .skill_id sidecar
        (target_dir / ".skill_id").write_text(skill_id)
```

### 4.2 Cloud Search

Hybrid search combining local BM25 with cloud embedding search:

```python
# openspace/cloud/search.py

async def hybrid_search_skills(query: str, local_skills, store, source: str, limit: int):
    candidates: List[Dict] = []
    
    # Local candidates
    if source in ("all", "local") and local_skills:
        candidates.extend(build_local_candidates(local_skills, store))
    
    # Cloud candidates
    if source in ("all", "cloud"):
        cloud_client = OpenSpaceClient(auth_headers, api_base)
        cloud_search_items = await asyncio.to_thread(
            cloud_client.search_record_embeddings,
            query=query, limit=300
        )
        candidates.extend(build_cloud_candidates(cloud_search_items))
    
    # Generate embeddings for local candidates
    query_embedding = await generate_embedding(query)
    for c in candidates:
        if not c.get("_embedding"):
            c["_embedding"] = await generate_embedding(c["_embedding_text"])
    
    engine = SkillSearchEngine()
    return engine.search(query, candidates, query_embedding=query_embedding, limit=limit)
```

### 4.3 Is Cloud Required?

**No**. OpenSpace works fully in local-only mode:

```python
# MCP tool has search_scope parameter
@mcp.tool()
async def execute_task(..., search_scope: str = "all"):
    # search_scope: "all" (local+cloud) | "local" (local only)
```

When `search_scope="local"` or when cloud is unavailable (no API key), OpenSpace falls back to local-only.

**However**, cloud provides:
- Embedding search across thousands of community skills
- Skill sharing across machines/users
- Centralized quality metrics

---

## 5. GDPVal Benchmark

### 5.1 Methodology

The benchmark measures **skill-driven token savings**:

1. **Phase 1 (Cold → Warm)**: Run 50 tasks sequentially, skills accumulate
2. **Phase 2 (Full Warm)**: Re-run same 50 tasks with all Phase 1 skills

Key metrics:
- Token usage (prompt, completion, total)
- Execution metrics (iterations, tool calls)
- Skills accumulated
- **Evaluation**: ClawWork's LLMEvaluator with 0.6 payment cliff

### 5.2 The "4.2x Income" and "46% Token Savings" Claims

From `calc_subset_performance.py`:

```python
# Head-to-head comparison: OpenSpace Phase 2 vs ClawWork agents
ratio_p2 = p2_e / cw_e if cw_e > 0 else float('inf')
ratio_str = f"{ratio_p2:.1f}x"

# Token savings calculation
tok_save = (1 - p2_total_tokens / cs_total_tokens) * 100
ag_save = (1 - p2_agent_tokens / cs_agent_tokens) * 100
```

**Important Caveats**:
1. The 4.2x is comparing **Phase 2** (warm start with accumulated skills) vs ClawWork agents
2. The 46% token savings is Phase 2 vs Phase 1 (same agent, different skill state)
3. The benchmark uses ClawWork's evaluation rubric (same LLM evaluator, same 0.6 cliff)
4. Model used: `qwen3.5-plus-02-15` (same pricing as ClawWork's Qwen3.5-Plus agent)

### 5.3 Is the Benchmark Reproducible?

**Partially**. Requirements:
1. ClawWork repo cloned alongside OpenSpace
2. `EVALUATION_API_KEY` for LLM evaluator
3. `OPENROUTER_API_KEY` for the agent
4. Download GDPVal dataset from HuggingFace

The benchmark includes:
- `tasks_50.json` — 50 task IDs (deterministic subset)
- `skills/` — evolved skills from Phase 1
- `.openspace/openspace.db` — skill DB with lineage

---

## 6. Code Quality Assessment

### 6.1 Structure

```
openspace/
├── skill_engine/      # Evolution, storage, analysis (core)
├── cloud/             # API client, search, embedding
├── grounding/         # Tool execution, agent loop
├── llm/               # LLM client wrapper
├── recording/         # Execution trajectory recording
├── config/            # Configuration loading
├── host_detection/    # OpenClaw/Nanobot integration
├── local_server/      # Platform-specific adapters
├── mcp_server.py      # MCP entry point
└── tool_layer.py      # Main OpenSpace class
```

### 6.2 Strengths

1. **Clear separation of concerns**: Skill engine, grounding, cloud are independent
2. **Well-documented**: Good docstrings, inline comments explaining design choices
3. **Type hints**: Uses `typing` module extensively
4. **Dataclasses**: Clean data modeling with `@dataclass`
5. **Async-first**: Proper async/await throughout

### 6.3 Weaknesses

1. **Zero test coverage**: No test files found in the repo
2. **Heavy dependencies**: litellm, mcp, anthropic, openai, flask, pyautogui, pillow
3. **No CI/CD**: No GitHub Actions or CI configuration
4. **Hardcoded constants**: Many thresholds defined inline
5. **Error handling**: Some broad `except Exception` catches
6. **No versioning**: No semantic versioning, just `0.1.0`

### 6.4 Production Readiness Verdict

**Not production-ready**. This is a research prototype with:

- Novel ideas worth borrowing (version DAG, quality metrics, evolution triggers)
- Good architecture but incomplete implementation
- Missing critical production features (tests, CI, monitoring, error handling)

**For Wakey, we should**:
1. Borrow the version DAG model for skill lineage
2. Adopt the quality metrics approach
3. Use SQLite for skill metadata (not filesystem)
4. Implement proper tests from the start

---

## 7. Comparison with OpenViking + Hermes

| Feature | OpenSpace | OpenViking | Hermes |
|---------|-----------|------------|--------|
| **Skill Storage** | SQLite DB | Filesystem (L0/L1/L2 JSON) | Filesystem (SKILL.md) |
| **Version Control** | Full DAG with snapshots | None | None |
| **Skill Search** | BM25 + embedding + SQL | JSON scan | File scan |
| **Evolution** | FIX/DERIVED/CAPTURED with LLM loop | None | None |
| **Quality Tracking** | Per-skill counters in DB | None | None |
| **MCP Integration** | 4 tools exposed | None | None |
| **Cloud Sync** | Full upload/download with lineage | None | None |
| **Agent Loop** | Token-driven, tool-calling | N/A | Single-shot skill injection |
| **Dependencies** | ~15 core + platform | Light (JSON, file I/O) | Light |

### Key Takeaways for Wakey

1. **Skill Registry**: Use SQLite like OpenSpace, not filesystem like Hermes. The version DAG model is essential for tracking skill evolution.

2. **Evolution**: OpenSpace's three-type model (FIX/DERIVED/CAPTURED) is more sophisticated than what we had planned. Consider adopting it.

3. **Quality Metrics**: OpenSpace's approach (selections → applied → completions) is a good foundation. Add to Wakey's `wakey-context` crate.

4. **Agent Loop**: OpenSpace's token-driven loop with tool access is more flexible than Hermes's single-shot. For Wakey's `wakey-cortex`, consider a similar approach.

5. **MCP**: OpenSpace exposes skills as MCP tools. For Wakey, we should provide the same integration path.

6. **Cloud**: OpenSpace's cloud is optional. For Wakey, we should support local-only operation by default, with optional cloud sync.

---

## 8. Code Snippets Worth Borrowing

### 8.1 SkillLineage with Content Snapshot

```python
@dataclass
class SkillLineage:
    origin: SkillOrigin
    generation: int = 0
    parent_skill_ids: List[str] = field(default_factory=list)
    source_task_id: Optional[str] = None
    change_summary: str = ""  # LLM-generated description
    content_diff: str = ""    # Unified diff
    content_snapshot: Dict[str, str] = field(default_factory=dict)  # Full directory state
```

### 8.2 Atomic Counter Updates

```python
# In SQL, avoiding race conditions
UPDATE skill_records SET
    total_selections  = total_selections + 1,
    total_applied     = total_applied + ?,
    total_completions = total_completions + ?,
    last_updated      = ?
WHERE skill_id = ?
```

### 8.3 Evolution Trigger Thresholds

```python
# Relaxed thresholds for candidate screening
_FALLBACK_THRESHOLD = 0.4        # Fallback rate > 40%
_LOW_COMPLETION_THRESHOLD = 0.35 # Completion rate < 35%
_HIGH_APPLIED_FOR_FIX = 0.4      # High applied but low completion
_MODERATE_EFFECTIVE_THRESHOLD = 0.55  # Moderate effectiveness
```

### 8.4 Safety Check

```python
# Skills are blocked if they contain dangerous patterns
flags = check_skill_safety(embedding_text)
if not is_skill_safe(flags):
    logger.info(f"BLOCKED local skill {s.skill_id} — {flags}")
    continue
```

---

## 9. Recommendations for Wakey

1. **Adopt SQLite for skill registry**: OpenSpace's approach is more robust than filesystem-only.

2. **Implement version DAG**: The lineage model with content snapshots is essential for tracking skill evolution.

3. **Add quality metrics**: Track selections/applied/completions/fallbacks per skill.

4. **Three evolution types**: FIX (in-place), DERIVED (new skill from existing), CAPTURED (new from execution).

5. **Token-driven agent loop**: Allow skill evolution to use tools during the loop.

6. **Local-first with optional cloud**: Don't require cloud connectivity.

7. **MCP integration**: Expose skills as MCP tools for external agents.

8. **Write tests from the start**: OpenSpace's biggest weakness is zero test coverage.

---

## 10. References

- OpenSpace repo: https://github.com/HKUDS/OpenSpace
- GDPVal dataset: https://huggingface.co/datasets/openai/gdpval
- ClawWork repo: https://github.com/HKUDS/ClawWork (evaluation rubrics)
- MCP spec: https://modelcontextprotocol.io/