# OpenViking Internals: Deep Research

> This document extracts implementation details from the actual OpenViking Python codebase for reference when building Wakey.

## Overview

OpenViking is a Python-based agent framework with a virtual filesystem abstraction (`viking://`), tiered content loading (L0/L1/L2), hierarchical retrieval, and memory extraction. Key architecture:

- **Language**: Python 3.x (async)
- **Storage**: AGFS (Abstract Generic File System) - a Go-based filesystem server with Python bindings
- **Vector DB**: Pluggable (VikingDB, Volcengine, local SQLite-based)
- **LLM**: OpenAI-compatible HTTP client

---

## 1. Viking Filesystem (`viking://`)

### 1.1 URI Structure

```
viking://{scope}/{space}/{path}
```

- **scope**: `user`, `agent`, `session`, `resources`, `temp`
- **space**: Tenant identifier (user_space_name, agent_space_name)
- **path**: Hierarchical path within scope

**File**: `openviking/storage/viking_fs.py`

```python
# URI to path mapping (line 668-680)
def _uri_to_path(self, uri: str, ctx: Optional[RequestContext] = None) -> str:
    """Map virtual URI to account-isolated AGFS path.
    
    Pure prefix replacement: viking://{remainder} -> /local/{account_id}/{remainder}.
    """
    real_ctx = self._ctx_or_default(ctx)
    account_id = real_ctx.account_id
    _, parts = self._normalized_uri_parts(uri)
    if not parts:
        return f"/local/{account_id}"
    
    safe_parts = [self._shorten_component(p, self._MAX_FILENAME_BYTES) for p in parts]
    return f"/local/{account_id}/{'/'.join(safe_parts)}"
```

### 1.2 Directory Structure (Preset)

**File**: `openviking/core/directories.py`

```python
PRESET_DIRECTORIES: Dict[str, DirectoryDefinition] = {
    "session": DirectoryDefinition(
        path="",
        abstract="Session scope. Stores complete context for a single conversation...",
        overview="Session-level temporary data storage...",
    ),
    "user": DirectoryDefinition(
        path="",
        abstract="User scope. Stores user's long-term memory...",
        overview="User-level persistent data storage...",
        children=[
            DirectoryDefinition(
                path="memories",
                abstract="User's long-term memory storage...",
                children=[
                    DirectoryDefinition(path="preferences", ...),
                    DirectoryDefinition(path="entities", ...),
                    DirectoryDefinition(path="events", ...),
                ],
            ),
        ],
    ),
    "agent": DirectoryDefinition(
        path="",
        abstract="Agent scope. Stores Agent's learning memories, instructions, and skills.",
        children=[
            DirectoryDefinition(path="memories", children=[...]),
            DirectoryDefinition(path="instructions", ...),
            DirectoryDefinition(path="skills", ...),
        ],
    ),
    "resources": DirectoryDefinition(...),
}
```

### 1.3 Access Control

**File**: `openviking/storage/viking_fs.py` (lines 748-770)

```python
def _is_accessible(self, uri: str, ctx: RequestContext) -> bool:
    """Check whether a URI is visible/accessible under current request context."""
    normalized_uri, parts = self._normalized_uri_parts(uri)
    if ctx.role == Role.ROOT:
        return True  # Root sees everything
    
    scope = parts[0]
    if scope in {"resources", "temp"}:
        return True  # Public scopes
    
    space = self._extract_space_from_uri(normalized_uri)
    if space is None:
        return True
    
    # Tenant isolation
    if scope in {"user", "session"}:
        return space == ctx.user.user_space_name()
    if scope == "agent":
        return space == ctx.user.agent_space_name()
    return True
```

### 1.4 AGFS Backend

AGFS is a separate Go binary that provides:
- Local filesystem backend
- S3 backend
- In-memory backend
- Queue system for async processing

**File**: `openviking/agfs_manager.py`

```python
class AGFSManager:
    """Manages the lifecycle of the AGFS server process."""
    
    def _generate_config(self) -> dict:
        config = {
            "server": {"address": f":{self.port}", "log_level": self.log_level},
            "plugins": {
                "localfs": {"enabled": True, "path": "/local", 
                           "config": {"local_dir": str(self.vikingfs_path)}},
                "queuefs": {"enabled": True, "path": "/queue",
                           "config": {"backend": "sqlite", ...}},
            },
        }
        return config
```

---

## 2. Tiered Loading (L0/L1/L2)

### 2.1 Level Definitions

| Level | File | Purpose | Size |
|-------|------|---------|------|
| L0 | `.abstract.md` | One-sentence summary | ~256 chars |
| L1 | `.overview.md` | Medium detail, free Markdown | ~2000 chars |
| L2 | `content.md` or original file | Full content | Unlimited |

**File**: `openviking/core/context.py`

```python
class ContextLevel(int, Enum):
    """Context level (L0/L1/L2) for vector indexing"""
    ABSTRACT = 0  # L0: abstract
    OVERVIEW = 1  # L1: overview
    DETAIL = 2    # L2: detail/content
```

### 2.2 L0/L1 Generation

**File**: `openviking/storage/queuefs/semantic_processor.py`

The `SemanticProcessor` generates `.abstract.md` and `.overview.md` bottom-up:

```python
class SemanticProcessor(DequeueHandlerBase):
    """Processes messages from SemanticQueue, generates .abstract.md and .overview.md."""
    
    async def on_dequeue(self, data: Optional[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
        msg = SemanticMsg.from_dict(data)
        
        if msg.context_type == "memory":
            await self._process_memory_directory(msg)
        else:
            executor = SemanticDagExecutor(processor=self, ...)
            await executor.run(msg.uri)
```

**For directories**:

```python
async def _generate_overview(self, dir_uri: str, file_summaries: List[Dict], 
                             children_abstracts: List[Dict], ...) -> str:
    """Generate directory's .overview.md (L1)."""
    
    # Budget guard for large directories
    estimated_size = len(file_summaries_str) + len(children_abstracts_str)
    over_budget = estimated_size > semantic.max_overview_prompt_chars
    
    if over_budget and many_files:
        # Split into batches, generate partials, merge
        overview = await self._batched_generate_overview(...)
    else:
        overview = await self._single_generate_overview(...)
    
    return overview
```

**Prompt template** (`openviking/prompts/templates/semantic/overview_generation.yaml`):

```yaml
template: |
  Generate a directory overview for "{{ dir_name }}".
  
  Files:
  {{ file_summaries }}
  
  Subdirectories:
  {{ children_abstracts }}
  
  Write in {{ output_language }}.
```

### 2.3 When L2 is Loaded

L2 (full content) is loaded only when:
1. User explicitly requests it (e.g., `read_file`)
2. Agent needs to drill down from search results
3. Memory extraction reads conversation history

**File**: `openviking/storage/viking_fs.py`

```python
async def read_file(self, uri: str, offset: int = 0, limit: int = -1, ...) -> str:
    """Read single file, optionally sliced by line range."""
    path = self._uri_to_path(uri, ctx=ctx)
    raw = self.agfs.read(path)  # Full read
    raw = await self._decrypt_content(raw, ctx=ctx)
    text = self._decode_bytes(raw)
    
    # Line slicing
    if offset == 0 and limit == -1:
        return text
    lines = text.splitlines(keepends=True)
    return "".join(lines[offset:offset+limit] if limit != -1 else lines[offset:])
```

### 2.4 Token Savings

For a typical codebase with 10MB of source:
- L0 index: ~10KB (one sentence per directory)
- L1 index: ~100KB (overview per directory)
- Full retrieval: Only loads relevant L2 files

Estimated savings: **99%+ tokens** for broad exploration.

---

## 3. Directory Recursive Retrieval

### 3.1 Three-Phase Retrieval

**File**: `openviking/retrieve/hierarchical_retriever.py`

```python
class HierarchicalRetriever:
    async def retrieve(self, query: TypedQuery, ctx: RequestContext, limit: int = 5, ...):
        # Phase 1: Global vector search to find starting directories
        global_results = await self._global_vector_search(
            query_vector=query_vector,
            context_type=query.context_type.value if query.context_type else None,
            target_dirs=target_dirs,
            limit=max(limit, self.GLOBAL_SEARCH_TOPK),
        )
        
        # Phase 2: Merge starting points (roots + global hits)
        starting_points = self._merge_starting_points(query.query, root_uris, global_results)
        
        # Phase 3: Recursive search with score propagation
        candidates = await self._recursive_search(
            query=query.query,
            starting_points=starting_points,
            limit=limit,
            threshold=effective_threshold,
        )
        
        return QueryResult(query=query, matched_contexts=candidates[:limit])
```

### 3.2 Recursive Search Algorithm

```python
async def _recursive_search(self, query: str, starting_points: List[Tuple[str, float]], ...):
    """Recursive search with directory priority and score propagation."""
    
    dir_queue: List[tuple] = []  # Priority queue: (-score, uri)
    visited: set = set()
    collected_by_uri: Dict[str, Dict] = {}
    
    # Initialize with starting points
    for uri, score in starting_points:
        heapq.heappush(dir_queue, (-score, uri))
    
    while dir_queue:
        _, current_uri = heapq.heappop(dir_queue)
        if current_uri in visited:
            continue
        visited.add(current_uri)
        
        # Vector search children
        results = await vector_proxy.search_children_in_tenant(
            parent_uri=current_uri,
            query_vector=query_vector,
            limit=pre_filter_limit,
        )
        
        # Rerank results
        query_scores = self._rerank_scores(query, documents, fallback_scores)
        
        for r, score in zip(results, query_scores):
            # Score propagation: blend child score with parent
            final_score = alpha * score + (1 - alpha) * current_score
            
            if passes_threshold(final_score):
                collected_by_uri[r["uri"]] = r
            
            # Recurse into directories (not L2 files)
            if r.get("level") != 2:
                heapq.heappush(dir_queue, (-final_score, r["uri"]))
    
    return sorted(collected_by_uri.values(), key=lambda x: x["_final_score"], reverse=True)
```

### 3.3 Intent Analysis

**File**: `openviking/retrieve/intent_analyzer.py`

```python
class IntentAnalyzer:
    """Analyzes session context to generate query plans."""
    
    async def analyze(self, compression_summary: str, messages: List[Message], 
                      current_message: str, ...) -> QueryPlan:
        prompt = render_prompt("retrieval.intent_analysis", {
            "compression_summary": summary,
            "recent_messages": recent_messages,
            "current_message": current,
            "context_type": context_type.value if context_type else "",
            "target_abstract": target_abstract,
        })
        
        response = await vlm.get_completion_async(prompt)
        parsed = parse_json_from_response(response)
        
        queries = [
            TypedQuery(
                query=q.get("query"),
                context_type=ContextType(q.get("context_type")),
                intent=q.get("intent"),
                priority=q.get("priority"),
            )
            for q in parsed.get("queries", [])
        ]
        
        return QueryPlan(queries=queries, reasoning=parsed.get("reasoning"))
```

---

## 4. Memory Extraction

### 4.1 End-of-Session Extraction

**File**: `openviking/session/memory_extractor.py`

```python
class MemoryExtractor:
    """Extracts 8 categories of memories from session messages."""
    
    CATEGORY_DIRS = {
        MemoryCategory.PROFILE: "memories/profile.md",
        MemoryCategory.PREFERENCES: "memories/preferences",
        MemoryCategory.ENTITIES: "memories/entities",
        MemoryCategory.EVENTS: "memories/events",
        MemoryCategory.CASES: "memories/cases",
        MemoryCategory.PATTERNS: "memories/patterns",
        MemoryCategory.TOOLS: "memories/tools",
        MemoryCategory.SKILLS: "memories/skills",
    }
    
    async def extract(self, context: dict, user: UserIdentifier, session_id: str, ...):
        """Extract memory candidates from messages."""
        
        # Format messages with tool calls
        formatted_messages = self._format_messages_with_parts(messages)
        
        # Detect output language
        output_language = self._detect_output_language(messages)
        
        # LLM extraction
        prompt = render_prompt("compression.memory_extraction", {
            "summary": history_summary,
            "recent_messages": formatted_messages,
            "user": user._user_id,
            "output_language": output_language,
        })
        response = await vlm.get_completion_async(prompt)
        data = parse_json_from_response(response)
        
        candidates = []
        for mem in data.get("memories", []):
            category = MemoryCategory(mem.get("category", "patterns"))
            
            if category in (MemoryCategory.TOOLS, MemoryCategory.SKILLS):
                # Tool/Skill memory with stats
                candidates.append(ToolSkillCandidateMemory(
                    category=category,
                    abstract=mem.get("abstract"),
                    tool_name=calibrate_tool_name(mem.get("tool_name"), tool_parts),
                    call_time=stats.get("call_count", 0),
                    success_time=stats.get("success_time", 0),
                    ...
                ))
            else:
                candidates.append(CandidateMemory(...))
        
        return candidates
```

### 4.2 Tool/Skill Stats Collection

```python
# From tool_skill_utils.py
def collect_tool_stats(tool_parts: List[ToolPart]) -> Dict[str, Dict]:
    """Aggregate statistics from tool call parts."""
    stats = {}
    for part in tool_parts:
        name = part.tool_name
        if name not in stats:
            stats[name] = {"call_count": 0, "success_time": 0, "duration_ms": 0, ...}
        
        stats[name]["call_count"] += 1
        if part.tool_status == "completed":
            stats[name]["success_time"] += 1
        stats[name]["duration_ms"] += part.duration_ms or 0
        stats[name]["prompt_tokens"] += part.prompt_tokens or 0
        stats[name]["completion_tokens"] += part.completion_tokens or 0
    
    return stats
```

### 4.3 Memory Merge Operations

**File**: `openviking/session/memory/memory_updater.py`

```python
class MemoryUpdater:
    """Applies MemoryOperations to storage."""
    
    async def apply_operations(self, operations, ctx: RequestContext, ...):
        # Resolve all URIs
        resolved_ops = resolve_all_operations(operations, registry, user_space, agent_space)
        
        # Apply writes
        for resolved_op in resolved_ops.write_operations:
            await self._apply_write(resolved_op.model, resolved_op.uri, ctx)
        
        # Apply edits (with patch support)
        for resolved_op in resolved_ops.edit_operations:
            await self._apply_edit(resolved_op.model, resolved_op.uri, ctx)
        
        # Apply deletes
        for _, uri in resolved_ops.delete_operations:
            await self._apply_delete(uri, ctx)
        
        # Vectorize all changes
        await self._vectorize_memories(result, ctx)
```

### 4.4 ReAct Extraction Loop

**File**: `openviking/session/memory/extract_loop.py`

```python
class ExtractLoop:
    """Simplified ReAct orchestrator for memory updates."""
    
    async def run(self) -> Tuple[Optional[MemoryOperations], List[Dict]]:
        iteration = 0
        max_iterations = self.max_iterations
        
        messages = [system_instruction, schema_instruction]
        messages.extend(await self.context_provider.prefetch(...))
        
        while iteration < max_iterations:
            iteration += 1
            
            # LLM call with tools
            tool_calls, operations = await self._call_llm(messages)
            
            if tool_calls:
                await self._execute_tool_calls(messages, tool_calls, tools_used)
                continue
            
            if operations:
                # Check if we need to refetch unread files
                refetch_uris = await self._check_unread_existing_files(operations)
                if refetch_uris:
                    await self._add_refetch_results_to_messages(messages, refetch_uris)
                    continue
                
                return operations, tools_used
        
        raise RuntimeError(f"Reached {max_iterations} iterations without completion")
```

---

## 5. Skill Self-Evolution

### 5.1 Skill Registry

**File**: `openviking/core/skill_loader.py`

```python
class SkillLoader:
    """Load and parse SKILL.md files."""
    
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
    
    @classmethod
    def to_skill_md(cls, skill_dict: Dict) -> str:
        frontmatter = {"name": skill_dict["name"], "description": skill_dict.get("description")}
        yaml_str = yaml.dump(frontmatter, allow_unicode=True, sort_keys=False)
        return f"---\n{yaml_str}---\n\n{skill_dict.get('content', '')}"
```

### 5.2 Skill Processing

**File**: `openviking/utils/skill_processor.py`

```python
class SkillProcessor:
    """Handles skill processing and storage."""
    
    async def process_skill(self, data: Any, viking_fs: VikingFS, ctx: RequestContext, ...):
        # Parse skill from various formats
        skill_dict, auxiliary_files, base_path = self._parse_skill(data)
        
        # Generate L1 overview
        overview = await self._generate_overview(skill_dict, config)
        
        # Write to viking://agent/skills/{name}/
        skill_dir_uri = f"viking://agent/skills/{skill_dict['name']}"
        await viking_fs.write_context(
            uri=skill_dir_uri,
            content=skill_dict.get("content"),
            abstract=skill_dict.get("description"),
            overview=overview,
            content_filename="SKILL.md",
        )
        
        # Write auxiliary files
        await self._write_auxiliary_files(viking_fs, auxiliary_files, skill_dir_uri)
        
        # Index to vector store
        await self._index_skill(context, skill_dir_uri)
```

### 5.3 Skill Memory Updates

Skills are updated via the memory extraction system:

```python
# In memory_extractor.py
async def _merge_skill_memory(self, skill_name: str, candidate: CandidateMemory, ctx):
    """Merge Skill Memory with accumulated statistics."""
    uri = f"viking://agent/{agent_space}/memories/skills/{skill_name}.md"
    
    # Read existing
    existing = await viking_fs.read_file(uri, ctx=ctx) or ""
    
    # Parse and merge statistics
    existing_stats = self._parse_skill_statistics(existing)
    merged_stats = self._merge_skill_statistics(existing_stats, new_stats)
    
    # Generate merged content
    merged_content = self._generate_skill_memory_content(
        skill_name, merged_stats, merged_guidelines, fields=merged_fields
    )
    
    await viking_fs.write_file(uri, merged_content, ctx=ctx)
```

### 5.4 Self-Evolution Decision

The LLM decides during memory extraction whether to:
1. **Create** a new skill memory (no existing file)
2. **Update** an existing skill memory (merge with accumulated stats)

The decision is implicit in the extraction output format - the LLM returns a memory with a `skill_name` field, and the system determines create vs. merge based on file existence.

---

## 6. Key Patterns for Wakey

### 6.1 Virtual Filesystem

- Use a URI scheme (`viking://`) for all resources
- Map URIs to physical storage paths with tenant isolation
- Support multiple backends (local, S3, memory)
- Implement access control at the URI level

### 6.2 Tiered Loading

- Generate L0 (abstract) and L1 (overview) at ingest time
- Store as hidden files (`.abstract.md`, `.overview.md`)
- Use async queue for generation to avoid blocking
- Load L2 only when explicitly needed

### 6.3 Hierarchical Retrieval

- Start with vector search to find relevant directories
- Recursively explore with score propagation
- Use reranking for precision
- Support both keyword and semantic queries

### 6.4 Memory Extraction

- Run at end of session (or periodically for long sessions)
- Use structured LLM output with JSON schema
- Collect tool execution stats automatically
- Support merge operations for incremental updates

### 6.5 Skill Evolution

- Store skills as Markdown with YAML frontmatter
- Maintain execution statistics per skill
- Merge new observations with accumulated knowledge
- Index skills for retrieval

---

## 7. Relevant Code Snippets

### 7.1 Context Object

```python
# openviking/core/context.py
@dataclass
class Context:
    uri: str
    parent_uri: Optional[str]
    is_leaf: bool
    abstract: str
    context_type: str  # skill, memory, resource
    category: str      # patterns, cases, preferences, entities, events
    level: int         # 0, 1, 2
    user: Optional[UserIdentifier]
    account_id: str
    owner_space: str
    vector: Optional[List[float]]
```

### 7.2 Embedding Queue Message

```python
# openviking/storage/queuefs/embedding_msg.py
@dataclass
class EmbeddingMsg:
    id: str
    uri: str
    parent_uri: str
    text: str
    context_type: str
    level: int
    account_id: str
    owner_space: str
```

### 7.3 Tool Part (Message Component)

```python
# openviking/message/part.py
@dataclass
class ToolPart:
    tool_name: str
    tool_input: Dict[str, Any]
    tool_output: str
    tool_status: str  # "completed" | "error"
    duration_ms: int
    prompt_tokens: int
    completion_tokens: int
    skill_uri: Optional[str]
```

---

## 8. Differences from Wakey's Rust Architecture

| Aspect | OpenViking (Python) | Wakey (Rust) |
|--------|---------------------|--------------|
| Filesystem | AGFS (Go binary) | Native Rust fs |
| Event System | tokio broadcast | Event Spine (tokio broadcast) |
| Storage | Pluggable backends | SQLite + custom |
| Memory | Python dicts + Pydantic | Rust structs + serde |
| Vector DB | External service | Embedded (usearch/hnsw) |
| LLM Client | OpenAI SDK | HTTP client (reqwest) |
| Skills | Markdown files | WASM modules |

### Recommendations for Wakey

1. **Keep the URI abstraction** - it's clean and enables tenant isolation
2. **Implement L0/L1 as metadata** - store in SQLite alongside content
3. **Use the SemanticQueue pattern** - async processing for LLM calls
4. **Adopt the ReAct extraction loop** - but with Rust's ownership model
5. **Skills as WASM** - more powerful than Markdown, enables hot-reload

---

## 9. File Reference

Key files to study when implementing Wakey:

- `openviking/storage/viking_fs.py` - Virtual filesystem implementation
- `openviking/core/directories.py` - Preset directory structure
- `openviking/retrieve/hierarchical_retriever.py` - Retrieval algorithm
- `openviking/session/memory_extractor.py` - Memory extraction
- `openviking/storage/queuefs/semantic_processor.py` - L0/L1 generation
- `openviking/session/memory/extract_loop.py` - ReAct orchestrator
- `openviking/utils/skill_processor.py` - Skill handling