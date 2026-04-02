# ZeroClaw Deep Research — Implementation Details

> Extracted from actual Rust source code at `/home/dipendra-sharma/projects/zeroclaw/`
> Date: 2026-04-03

---

## 1. Agent Loop (`src/agent/loop_.rs`)

The agent loop is the core orchestration engine. The file is 352KB with 9000+ lines.

### Main Entry Point: `run_tool_call_loop`

**Location:** `src/agent/loop_.rs:2277`

```rust
pub(crate) async fn run_tool_call_loop(
    provider: &dyn Provider,
    history: &mut Vec<ChatMessage>,
    tools_registry: &[Box<dyn Tool>],
    observer: &dyn Observer,
    provider_name: &str,
    model: &str,
    temperature: f64,
    silent: bool,
    approval: Option<&ApprovalManager>,
    channel_name: &str,
    channel_reply_target: Option<&str>,
    multimodal_config: &crate::config::MultimodalConfig,
    max_tool_iterations: usize,
    cancellation_token: Option<CancellationToken>,
    on_delta: Option<tokio::sync::mpsc::Sender<DraftEvent>>,
    hooks: Option<&crate::hooks::HookRunner>,
    excluded_tools: &[String],
    dedup_exempt_tools: &[String],
    activated_tools: Option<&std::sync::Arc<std::sync::Mutex<crate::tools::ActivatedToolSet>>>,
    model_switch_callback: Option<ModelSwitchCallback>,
    pacing: &crate::config::PacingConfig,
    max_tool_result_chars: usize,
    context_token_budget: usize,
    shared_budget: Option<Arc<std::sync::atomic::AtomicUsize>>,
    force_xml_tools: bool,
) -> Result<String>
```

### Iteration Loop Structure

```rust
// Default max iterations = 10
const DEFAULT_MAX_TOOL_ITERATIONS: usize = 10;

for iteration in 0..max_iterations {
    // 1. Cancellation check
    if cancellation_token.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(ToolLoopCancelled.into());
    }

    // 2. Shared budget check (for subagents)
    if let Some(ref budget) = shared_budget {
        let remaining = budget.load(Ordering::Relaxed);
        if remaining == 0 {
            break;
        }
        budget.fetch_sub(1, Ordering::Relaxed);
    }

    // 3. Preemptive context management
    if context_token_budget > 0 {
        let estimated = estimate_history_tokens(history);
        if estimated > context_token_budget {
            // Fast trim old tool results
            let chars_saved = fast_trim_tool_results(history, 4);
            // If still over, use history pruner
            if recheck > context_token_budget {
                prune_history(history, &HistoryPrunerConfig { ... });
            }
        }
    }

    // 4. Model switch check (runtime model switching)
    if let Some(ref callback) = model_switch_callback {
        if let Some((new_provider, new_model)) = guard.as_ref() {
            return Err(ModelSwitchRequested { ... });
        }
    }

    // 5. Tool filtering for this turn
    let tool_specs = filter_tool_specs_for_turn(tools_registry, groups, user_message);

    // 6. Vision provider routing (if images present)
    let vision_provider_box = if image_marker_count > 0 && !provider.supports_vision() {
        // Create dedicated vision provider
    };

    // 7. LLM call (streaming or non-streaming)
    let chat_result = if should_consume_provider_stream {
        consume_provider_streaming_response(...).await
    } else {
        active_provider.chat(...).await
    };

    // 8. Parse tool calls (native or XML)
    let calls = if resp.tool_calls.is_empty() {
        parse_tool_calls(&response_text)  // Fallback to XML parsing
    } else {
        resp.tool_calls.iter().map(...).collect()
    };

    // 9. If no tool calls, return final response
    if tool_calls.is_empty() {
        return Ok(display_text);
    }

    // 10. Execute tools (parallel or sequential)
    let allow_parallel = should_execute_tools_in_parallel(&tool_calls, approval);
    // ... execute and collect results ...

    // 11. Loop detection
    let loop_result = loop_detector.record(tool_name, args, result);
    match loop_result {
        LoopDetectionResult::Warning(msg) => { /* inject nudge */ }
        LoopDetectionResult::Block(msg) => { /* replace output */ }
        LoopDetectionResult::Break(msg) => { /* terminate turn */ }
        _ => {}
    }
}
```

### Tool Call Parsing

**XML Format Parsing** (`src/agent/loop_.rs:400-700`):

```rust
// Parse XML-style tool calls in `<tool_call>` bodies
fn parse_xml_tool_calls(xml_content: &str) -> Option<Vec<ParsedToolCall>> {
    for (tool_name_str, inner_content) in extract_xml_pairs(trimmed) {
        // Nested XML args: <shell><command>pwd</command></shell>
        // OR JSON payload: <shell>{"command":"pwd"}</shell>
    }
}

// MiniMax XML invoke format:
// <invoke name="shell"><parameter name="command">pwd</parameter></invoke>
static MINIMAX_INVOKE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<invoke\b[^>]*\bname\s*=\s*(?:"([^"]+)"|'([^']+)')[^>]*>(.*?)</invoke>"#)
        .unwrap()
});
```

### Credential Scrubbing

**Location:** `src/agent/loop_.rs:189-230`

```rust
static SENSITIVE_KV_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(token|api[_-]?key|password|secret|user[_-]?key|bearer|credential)["']?\s*[:=]\s*(?:"([^"]{8,})"|'([^']{8,})'|([a-zA-Z0-9_\-\.]{8,}))"#).unwrap()
});

pub(crate) fn scrub_credentials(input: &str) -> String {
    SENSITIVE_KV_REGEX.replace_all(input, |caps: &Captures| {
        // Preserve first 4 chars for context, then redact
        format!("{}: {}*[REDACTED]", key, prefix)
    }).to_string()
}
```

---

## 2. Memory System (`src/memory/`)

### Memory Trait Definition

**Location:** `src/memory/traits.rs`

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    fn name(&self) -> &str;

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;
    async fn list(&self, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>;
    async fn forget(&self, key: &str) -> anyhow::Result<bool>;
    async fn count(&self) -> anyhow::Result<usize>;
    async fn health_check(&self) -> bool;

    // Optional: procedural memory for "how to" patterns
    async fn store_procedural(&self, _messages: &[ProceduralMessage], _session_id: Option<&str>) -> anyhow::Result<()>;

    // GDPR export
    async fn export(&self, filter: &ExportFilter) -> anyhow::Result<Vec<MemoryEntry>>;
}

pub enum MemoryCategory {
    Core,          // Long-term facts, preferences, decisions
    Daily,         // Daily session logs
    Conversation,  // Conversation context
    Custom(String),
}

pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,
    pub session_id: Option<String>,
    pub score: Option<f64>,
    pub namespace: String,
    pub importance: Option<f64>,
    pub superseded_by: Option<String>,
}
```

### SQLite Implementation with Hybrid Search

**Location:** `src/memory/sqlite.rs`

**Schema:**

```sql
-- Core memories table
CREATE TABLE IF NOT EXISTS memories (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    content     TEXT NOT NULL,
    category    TEXT NOT NULL DEFAULT 'core',
    embedding   BLOB,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    session_id  TEXT,
    namespace   TEXT DEFAULT 'default',
    importance  REAL DEFAULT 0.5,
    superseded_by TEXT
);

-- FTS5 full-text search (BM25 scoring)
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    key, content, content=memories, content_rowid=rowid
);

-- Embedding cache with LRU eviction
CREATE TABLE IF NOT EXISTS embedding_cache (
    content_hash TEXT PRIMARY KEY,
    embedding    BLOB NOT NULL,
    created_at   TEXT NOT NULL,
    accessed_at  TEXT NOT NULL
);
```

**Hybrid Search Implementation:**

```rust
pub struct SqliteMemory {
    conn: Arc<Mutex<Connection>>,
    embedder: Arc<dyn EmbeddingProvider>,
    vector_weight: f32,   // Default: 0.7
    keyword_weight: f32,  // Default: 0.3
    cache_max: usize,     // Default: 10,000
    search_mode: SearchMode,
}

impl SqliteMemory {
    /// FTS5 BM25 keyword search
    pub fn fts5_search(conn: &Connection, query: &str, limit: usize) -> anyhow::Result<Vec<(String, f32)>> {
        let sql = "SELECT m.id, bm25(memories_fts) as score
                   FROM memories_fts f
                   JOIN memories m ON m.rowid = f.rowid
                   WHERE memories_fts MATCH ?1
                   ORDER BY score
                   LIMIT ?2";
        // BM25 returns negative scores (lower = better), negate for ranking
    }

    /// Vector similarity search: cosine similarity
    pub fn vector_search(
        conn: &Connection,
        query_embedding: &[f32],
        limit: usize,
        category: Option<&str>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<(String, f32)>> {
        // Full table scan with cosine similarity computation
    }

    /// Hybrid merge: weighted fusion of vector + keyword results
    pub async fn recall(&self, query: &str, limit: usize, ...) -> anyhow::Result<Vec<MemoryEntry>> {
        // 1. Get or compute embedding
        let embedding = self.get_or_compute_embedding(query).await?;

        // 2. Run both searches
        let vector_results = Self::vector_search(&conn, &embedding, limit, ...)?;
        let fts_results = Self::fts5_search(&conn, query, limit)?;

        // 3. Weighted fusion: 0.7 * vector_score + 0.3 * keyword_score
        // 4. Apply time decay for non-Core memories
    }
}
```

**PRAGMA Tuning:**

```rust
conn.execute_batch(
    "PRAGMA journal_mode = WAL;
     PRAGMA synchronous  = NORMAL;
     PRAGMA mmap_size    = 8388608;  // 8 MB
     PRAGMA cache_size   = -2000;    // 2 MB
     PRAGMA temp_store   = MEMORY;",
)?;
```

### Embedding Provider

**Location:** `src/memory/embeddings.rs`

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>>;
}

pub struct OpenAiEmbedding {
    base_url: String,
    api_key: String,
    model: String,
    dims: usize,
}

impl EmbeddingProvider for OpenAiEmbedding {
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let resp = self.http_client()
            .post(self.embeddings_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({"model": self.model, "input": texts}))
            .send()
            .await?;
        // Parse embedding vectors from response
    }
}
```

### Multi-Stage Retrieval Pipeline

**Location:** `src/memory/retrieval.rs`

```rust
pub struct RetrievalConfig {
    pub stages: Vec<String>,           // ["cache", "fts", "vector"]
    pub fts_early_return_score: f64,   // 0.85 - skip vector if FTS score high
    pub cache_max_entries: usize,      // 256
    pub cache_ttl: Duration,           // 5 minutes
}

impl RetrievalPipeline {
    pub async fn recall(&self, query: &str, limit: usize, ...) -> anyhow::Result<Vec<MemoryEntry>> {
        for stage in &self.config.stages {
            match stage.as_str() {
                "cache" => {
                    if let Some(cached) = self.check_cache(&ck) {
                        return Ok(cached);
                    }
                }
                "fts" => {
                    let results = self.memory.recall(...).await?;
                    if results.first().and_then(|e| e.score) >= self.config.fts_early_return_score {
                        return Ok(results);  // Early return
                    }
                }
                "vector" => { /* Full hybrid search */ }
            }
        }
    }
}
```

### Markdown Export (Soul)

**Location:** `src/memory/markdown.rs`

```rust
pub struct MarkdownMemory {
    workspace_dir: PathBuf,
}

impl MarkdownMemory {
    fn core_path(&self) -> PathBuf {
        self.workspace_dir.join("MEMORY.md")
    }

    fn daily_path(&self) -> PathBuf {
        let date = Local::now().format("%Y-%m-%d").to_string();
        self.memory_dir().join(format!("{date}.md"))
    }

    async fn store(&self, key: &str, content: &str, category: MemoryCategory, ...) -> anyhow::Result<()> {
        let entry = format!("- **{key}**: {content}");
        let path = match category {
            MemoryCategory::Core => self.core_path(),
            _ => self.daily_path(),
        };
        self.append_to_file(&path, &entry).await
    }
}
```

---

## 3. Cron/Scheduler (`src/cron/`)

### Scheduler Implementation

**Location:** `src/cron/scheduler.rs`

```rust
pub async fn run(config: Config, event_tx: EventBroadcast) -> Result<()> {
    let poll_secs = config.reliability.scheduler_poll_secs.max(5);
    let mut interval = time::interval(Duration::from_secs(poll_secs));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    // Startup catch-up: run ALL overdue jobs before polling loop
    if config.cron.catch_up_on_startup {
        catch_up_overdue_jobs(&config, &security, &event_tx).await;
    }

    loop {
        interval.tick().await;

        let jobs = due_jobs(&config, Utc::now())?;
        process_due_jobs(&config, &security, jobs, SCHEDULER_COMPONENT, &event_tx).await;
    }
}
```

### Window-Based Scheduling (Prevents Missed Beats)

```rust
async fn catch_up_overdue_jobs(config: &Config, ...) {
    // Fetch ALL overdue jobs (ignoring max_tasks limit)
    let jobs = all_overdue_jobs(config, now)?;

    // Execute every missed job once
    process_due_jobs(config, security, jobs, ...).await;
}
```

### Job Execution with Retry

```rust
async fn execute_job_with_retry(config: &Config, security: &SecurityPolicy, job: &CronJob) -> (bool, String) {
    let retries = config.reliability.scheduler_retries;
    let mut backoff_ms = config.reliability.provider_backoff_ms.max(200);

    for attempt in 0..=retries {
        let (success, output) = match job.job_type {
            JobType::Shell => run_job_command(config, security, job).await,
            JobType::Agent => run_agent_job(config, security, job).await,
        };

        if success {
            return (true, output);
        }

        // Non-retryable errors
        if output.starts_with("blocked by security policy:") {
            return (false, output);
        }

        // Exponential backoff with jitter
        time::sleep(Duration::from_millis(backoff_ms + jitter_ms)).await;
        backoff_ms = (backoff_ms.saturating_mul(2)).min(30_000);
    }
}
```

### Cron Store (SQLite)

**Location:** `src/cron/store.rs`

```rust
pub fn add_shell_job(config: &Config, name: Option<String>, schedule: Schedule, command: &str, delivery: Option<DeliveryConfig>) -> Result<CronJob> {
    let next_run = next_run_for_schedule(&schedule, now)?;
    with_connection(config, |conn| {
        conn.execute(
            "INSERT INTO cron_jobs (id, expression, command, schedule, job_type, ...) VALUES (...)",
            params![id, expression, command, schedule_json, ...],
        )
    })
}

pub fn due_jobs(config: &Config, now: DateTime<Utc>) -> Result<Vec<CronJob>> {
    with_connection(config, |conn| {
        conn.prepare(
            "SELECT ... FROM cron_jobs
             WHERE enabled = 1 AND next_run <= ?1
             ORDER BY next_run ASC
             LIMIT ?2"
        )
    })
}
```

---

## 4. Security Policy (`src/security/policy.rs`)

### Autonomy Levels

```rust
pub enum AutonomyLevel {
    ReadOnly,    // Can observe but not act
    Supervised,  // Acts but requires approval for risky operations (default)
    Full,        // Autonomous execution within policy bounds
}
```

### SecurityPolicy Structure

```rust
pub struct SecurityPolicy {
    pub autonomy: AutonomyLevel,
    pub workspace_dir: PathBuf,
    pub workspace_only: bool,
    pub allowed_commands: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub allowed_roots: Vec<PathBuf>,
    pub max_actions_per_hour: u32,
    pub max_cost_per_day_cents: u32,
    pub require_approval_for_medium_risk: bool,
    pub block_high_risk_commands: bool,
    pub shell_env_passthrough: Vec<String>,
    pub shell_timeout_secs: u64,
    pub tracker: PerSenderTracker,
}
```

### can_act Implementation

```rust
pub fn can_act(&self) -> bool {
    self.autonomy != AutonomyLevel::ReadOnly
}

pub fn enforce_tool_operation(&self, operation: ToolOperation, operation_name: &str) -> Result<(), String> {
    match operation {
        ToolOperation::Read => Ok(()),  // Always allowed
        ToolOperation::Act => {
            if !self.can_act() {
                return Err(format!("Security policy: read-only mode, cannot perform '{operation_name}'"));
            }
            if !self.record_action() {
                return Err("Rate limit exceeded: action budget exhausted".to_string());
            }
            Ok(())
        }
    }
}
```

### Command Risk Classification

```rust
pub fn command_risk_level(&self, command: &str) -> CommandRiskLevel {
    for segment in split_unquoted_segments(command) {
        let base = command_basename(base_raw).to_ascii_lowercase();

        // High-risk commands
        if matches!(base, "rm" | "mkfs" | "dd" | "sudo" | "chmod" | "curl" | "wget" | "nc" | "ssh" | "powershell" | ...) {
            return CommandRiskLevel::High;
        }

        // Medium-risk commands (state-changing)
        let medium = match base {
            "git" => args.first().is_some_and(|verb| matches!(verb, "commit" | "push" | "reset" | ...)),
            "npm" | "cargo" => args.first().is_some_and(|verb| matches!(verb, "install" | "add" | ...)),
            "touch" | "mkdir" | "mv" | "cp" => true,
            _ => false,
        };
    }
}
```

### Per-Sender Rate Limiting

```rust
pub struct PerSenderTracker {
    buckets: Mutex<HashMap<String, ActionTracker>>,
}

impl PerSenderTracker {
    pub fn record_for_current(&self, max: u32) -> bool {
        let key = TOOL_LOOP_THREAD_ID.try_with(|v| v.clone()).ok().flatten()
            .unwrap_or_else(|| Self::GLOBAL_KEY.to_string());
        self.record_within(&key, max)
    }
}
```

---

## 5. Provider Trait (LLM Client) (`src/providers/traits.rs`)

### Core Types

```rust
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

pub struct ChatResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,  // For thinking models
}

pub enum StreamEvent {
    TextDelta(StreamChunk),
    ToolCall(ToolCall),
    PreExecutedToolCall { name: String, args: String },
    Final,
}
```

### Provider Trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    fn convert_tools(&self, tools: &[ToolSpec]) -> ToolsPayload {
        ToolsPayload::PromptGuided { instructions: build_tool_instructions_text(tools) }
    }

    async fn chat_with_system(&self, system_prompt: Option<&str>, message: &str, model: &str, temperature: f64) -> anyhow::Result<String>;

    async fn chat(&self, request: ChatRequest<'_>, model: &str, temperature: f64) -> anyhow::Result<ChatResponse> {
        // Default: inject tool instructions into system prompt if not native
    }

    fn supports_native_tools(&self) -> bool { self.capabilities().native_tool_calling }
    fn supports_vision(&self) -> bool { self.capabilities().vision }

    fn stream_chat(&self, request: ChatRequest<'_>, model: &str, temperature: f64, options: StreamOptions) -> stream::BoxStream<'static, StreamResult<StreamEvent>>;
}

pub struct ProviderCapabilities {
    pub native_tool_calling: bool,
    pub vision: bool,
    pub prompt_caching: bool,
}

pub enum ToolsPayload {
    Gemini { function_declarations: Vec<Value> },
    Anthropic { tools: Vec<Value> },
    OpenAI { tools: Vec<Value> },
    PromptGuided { instructions: String },
}
```

### OpenAI-Compatible Provider

**Location:** `src/providers/compatible.rs`

```rust
pub struct OpenAiCompatibleProvider {
    pub name: String,
    pub base_url: String,
    pub credential: Option<String>,
    pub auth_header: AuthStyle,
    supports_vision: bool,
    native_tool_calling: bool,
    timeout_secs: u64,
    extra_headers: HashMap<String, String>,
}

pub enum AuthStyle {
    Bearer,
    XApiKey,
    Custom(String),
    ZhipuJwt,  // Zhipu/GLM JWT auth: id.secret -> HMAC-SHA256 JWT
}

impl Provider for OpenAiCompatibleProvider {
    fn chat_completions_url(&self) -> String {
        // Detect if base_url already includes path
        if has_full_endpoint { self.base_url.clone() }
        else { format!("{}/chat/completions", self.base_url) }
    }

    async fn chat_with_tools(&self, messages: &[ChatMessage], tools: &[Value], model: &str, temperature: f64) -> anyhow::Result<ChatResponse> {
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "tools": tools,
            "temperature": temperature,
        });

        let resp = self.http_client()
            .post(self.chat_completions_url())
            .header("Authorization", format!("Bearer {}", credential))
            .json(&body)
            .send()
            .await?;

        // Parse response, extract tool_calls
    }
}
```

---

## 6. Auto-Compaction System

### Context Compressor

**Location:** `src/agent/context_compressor.rs`

```rust
pub struct ContextCompressionConfig {
    pub enabled: bool,
    pub threshold_ratio: f64,          // 0.50 - trigger when 50% of context used
    pub protect_first_n: usize,        // 3 - protect system prompt + initial context
    pub protect_last_n: usize,         // 4 - protect recent messages
    pub max_passes: u32,               // 3
    pub summary_max_chars: usize,      // 4000
    pub timeout_secs: u64,             // 60
}

pub struct ContextCompressor {
    config: ContextCompressionConfig,
    context_window: usize,
    memory: Option<Arc<dyn Memory>>,
}

impl ContextCompressor {
    pub async fn compress_if_needed(&self, history: &mut Vec<ChatMessage>, provider: &dyn Provider, model: &str) -> Result<CompressionResult> {
        let tokens = estimate_tokens(history);
        let threshold = (self.context_window as f64 * self.config.threshold_ratio) as usize;

        if tokens < threshold {
            return Ok(CompressionResult { compressed: false, ... });
        }

        for pass in 0..self.config.max_passes {
            // 1. Fast trim oversized tool results
            self.fast_trim_tool_results(history);

            // 2. Summarize middle section (not protected)
            let summary = self.summarize_segment(&history[protect_start..protect_end], provider, model).await?;

            // 3. Replace middle with summary
            // 4. Persist summary to memory
            if let Some(ref memory) = self.memory {
                memory.store("compaction_summary", &summary, MemoryCategory::Core, None).await?;
            }
        }
    }
}
```

### History Pruner

**Location:** `src/agent/history_pruner.rs`

```rust
pub struct HistoryPrunerConfig {
    pub enabled: bool,
    pub max_tokens: usize,
    pub keep_recent: usize,
    pub collapse_tool_results: bool,
}

pub fn prune_history(messages: &mut Vec<ChatMessage>, config: &HistoryPrunerConfig) -> PruneStats {
    // Phase 1: Collapse assistant+tool pairs into short summaries
    if messages[i].role == "assistant" && messages[i + 1].role == "tool" {
        let summary = format!("[Tool result: {}...]", truncated);
        messages[i] = ChatMessage { role: "assistant", content: summary };
        messages.remove(i + 1);
    }

    // Phase 2: Drop oldest non-protected messages until under budget
    while estimate_tokens(messages) > config.max_tokens {
        messages.remove(idx);
    }
}
```

### Loop Detection

**Location:** `src/agent/loop_detector.rs`

```rust
pub(crate) struct LoopDetector {
    config: LoopDetectorConfig,
    window: VecDeque<ToolCallRecord>,
}

pub(crate) enum LoopDetectionResult {
    Ok,
    Warning(String),   // Inject nudge
    Block(String),     // Replace output
    Break(String),     // Terminate turn
}

impl LoopDetector {
    pub fn record(&mut self, name: &str, args: &Value, result: &str) -> LoopDetectionResult {
        // Pattern 1: Exact repeat (same tool + args 3+ times)
        if consecutive >= max + 2 {
            return LoopDetectionResult::Break(...);
        }

        // Pattern 2: Ping-pong (A->B->A->B for 4+ cycles)
        // Pattern 3: No progress (same tool, different args, same result 5+ times)
    }
}
```

---

## 7. Daemon Mode (`src/daemon/mod.rs`)

```rust
pub async fn run(config: Config, host: String, port: u16) -> Result<()> {
    // Shared event broadcast for real-time dashboard updates
    let (event_tx, _rx) = tokio::sync::broadcast::channel::<serde_json::Value>(256);

    // Spawn component supervisors with exponential backoff
    let mut handles: Vec<JoinHandle<()>> = vec![
        spawn_state_writer(config.clone()),
        spawn_component_supervisor("gateway", ..., || crate::gateway::run_gateway(...)),
        spawn_component_supervisor("channels", ..., || crate::channels::start_channels(...)),
        spawn_component_supervisor("heartbeat", ..., || run_heartbeat_worker(...)),
        spawn_component_supervisor("scheduler", ..., || crate::cron::scheduler::run(...)),
    ];

    wait_for_shutdown_signal().await?;

    for handle in &handles {
        handle.abort();
    }
}

fn spawn_component_supervisor<F, Fut>(name: &'static str, initial_backoff_secs: u64, max_backoff_secs: u64, mut run_component: F) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = initial_backoff_secs;
        loop {
            match run_component().await {
                Ok(()) => { backoff = initial_backoff_secs; }
                Err(e) => { tracing::error!("Daemon component '{name}' failed: {e}"); }
            }
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            backoff = backoff.saturating_mul(2).min(max_backoff);
        }
    })
}

async fn wait_for_shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        let mut sigint = signal(SignalKind::interrupt())?;
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sighup = signal(SignalKind::hangup())?;  // Ignored - daemon survives

        tokio::select! {
            _ = sigint.recv() => { tracing::info!("Received SIGINT"); }
            _ = sigterm.recv() => { tracing::info!("Received SIGTERM"); }
            _ = sighup.recv() => { /* Ignore - stay running */ }
        }
    }
}
```

---

## Key Patterns for Wakey Implementation

### 1. Event Spine Equivalent
ZeroClaw uses `tokio::sync::broadcast` channels for real-time event propagation. This is the pattern for `wakey-spine`.

### 2. Provider Trait Pattern
The `Provider` trait with capabilities and `ToolsPayload` enum allows runtime polymorphism. Wakey should follow this pattern for LLM clients.

### 3. Memory Abstraction
The `Memory` trait with SQLite + vector + FTS5 hybrid search is a reference implementation. Wakey's `wakey-memory` should implement this trait.

### 4. Security Policy Enforcement
Three-level autonomy (ReadOnly/Supervised/Full) with per-sender rate limiting and command risk classification.

### 5. Tool Execution Loop
- Native tool calling when provider supports it
- XML fallback parsing for prompt-guided tools
- Parallel execution when safe
- Loop detection with escalation (Warning → Block → Break)

### 6. Context Management
- Preemptive trimming before context overflow
- History pruning with protected messages
- Context compression via LLM summarization
- Memory persistence of summaries

### 7. Scheduler Design
- Startup catch-up for missed jobs
- Exponential backoff with jitter
- Declarative job sync from config
- One-shot auto-delete jobs