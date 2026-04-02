# ZeroClaw Implementation Patterns — Research Report

**Date**: 2026-04-03  
**Source**: `/home/dipendra-sharma/projects/zeroclaw/`  
**Goal**: Extract concrete patterns for Wakey implementation

---

## Executive Summary

ZeroClaw is a production-grade Rust AI assistant (~200K+ lines) that demonstrates mature patterns for trait-based extensibility, multi-provider LLM abstraction, and embedded vector search. Key takeaways for Wakey:

1. **Trait-first design** enables swappable implementations via factory pattern
2. **Single HTTP client** (reqwest) serves all OpenAI-compatible providers
3. **SQLite + FTS5 + custom vector search** achieves hybrid semantic/keyword retrieval without external dependencies
4. **Aggressive binary optimization** achieves <5MB binary with `opt-level="z"` + LTO

---

## 1. Trait System

### 1.1 Core Trait Definitions

ZeroClaw defines four primary traits that enable runtime swappability:

#### Tool Trait (`src/tools/traits.rs`)

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult>;
    
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
```

**What they did**: Single trait with `async_trait` macro, returns `ToolResult` struct with success/output/error.

**Why it works**: 
- `Send + Sync` bounds enable Arc<Mutex> sharing across threads
- `spec()` method provides LLM-consumable function definition
- Simple interface — implement 4 methods, get full tool integration

#### Provider Trait (`src/providers/traits.rs`)

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    // Capability declaration
    fn capabilities(&self) -> ProviderCapabilities { /* default */ }
    
    // Tool format conversion
    fn convert_tools(&self, tools: &[ToolSpec]) -> ToolsPayload { /* default */ }
    
    // Simple one-shot chat
    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<String>;
    
    // Multi-turn with tools (returns structured response)
    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<ChatResponse>;
    
    // Capability helpers
    fn supports_native_tools(&self) -> bool;
    fn supports_vision(&self) -> bool;
    fn supports_streaming(&self) -> bool;
    
    // Warmup for connection pooling
    async fn warmup(&self) -> anyhow::Result<()> { Ok(()) }
}
```

**Key pattern**: Default implementations let providers declare capabilities, not implement all methods.

```rust
pub struct ProviderCapabilities {
    pub native_tool_calling: bool,
    pub vision: bool,
}
```

**Why it works**:
- Providers declare capabilities once, trait methods auto-adapt
- `ToolsPayload` enum handles different provider formats (Gemini/Anthropic/OpenAI/PromptGuided)
- Default `chat()` injects tool instructions into system prompt if native calling unavailable

#### Memory Trait (`src/memory/traits.rs`)

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
    ) -> anyhow::Result<Vec<MemoryEntry>>;
    
    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;
    async fn list(&self, category: Option<&MemoryCategory>, session_id: Option<&str>) 
        -> anyhow::Result<Vec<MemoryEntry>>;
    async fn forget(&self, key: &str) -> anyhow::Result<bool>;
    async fn count(&self) -> anyhow::Result<usize>;
    async fn health_check(&self) -> bool;
}
```

**What they did**: Minimal CRUD interface with optional session scoping and category filtering.

**Why it works**: Simple enough for multiple backends (SQLite, Postgres, Qdrant, Markdown, None), rich enough for production use.

#### Channel Trait (`src/channels/traits.rs`)

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    
    async fn send(&self, message: &SendMessage) -> anyhow::Result<()>;
    async fn listen(&self, tx: tokio::sync::mpsc::Sender<ChannelMessage>) 
        -> anyhow::Result<()>;
    
    // Optional capabilities with defaults
    async fn health_check(&self) -> bool { true }
    async fn start_typing(&self, _recipient: &str) -> anyhow::Result<()> { Ok(()) }
    async fn stop_typing(&self, _recipient: &str) -> anyhow::Result<()> { Ok(()) }
    fn supports_draft_updates(&self) -> bool { false }
}
```

**What they did**: Platform integrations implement `send()` + `listen()`, optional methods have no-op defaults.

**Why it works**: Channels don't need to implement every feature — defaults are sensible.

### 1.2 Factory Pattern Registration

ZeroClaw uses centralized factory functions instead of runtime registration:

```rust
// Provider factory (src/providers/mod.rs)
pub fn create_provider_with_options(
    name: &str,
    api_key: Option<&str>,
    options: &ProviderRuntimeOptions,
) -> anyhow::Result<Box<dyn Provider>> {
    match name {
        "openrouter" => Ok(Box::new(openrouter::OpenRouterProvider::new(key))),
        "anthropic" => Ok(Box::new(anthropic::AnthropicProvider::new(key))),
        "openai" => Ok(Box::new(openai::OpenAiProvider::with_base_url(api_url, key))),
        "ollama" => Ok(Box::new(ollama::OllamaProvider::new_with_reasoning(...))),
        // ... 30+ providers
        name if name.starts_with("custom:") => {
            let base_url = parse_custom_provider_url(...)?;
            Ok(Box::new(OpenAiCompatibleProvider::new(...)))
        }
        _ => anyhow::bail!("Unknown provider: {name}")
    }
}
```

**Memory factory:**

```rust
pub fn create_memory(
    config: &MemoryConfig,
    workspace_dir: &Path,
    api_key: Option<&str>,
) -> anyhow::Result<Box<dyn Memory>> {
    match classify_memory_backend(&config.backend) {
        MemoryBackendKind::Sqlite => Ok(Box::new(SqliteMemory::new(workspace_dir)?)),
        MemoryBackendKind::Lucid => { /* hybrid SQLite + markdown */ },
        MemoryBackendKind::Postgres => postgres_builder(),
        MemoryBackendKind::Qdrant => { /* remote vector DB */ },
        MemoryBackendKind::Markdown => Ok(Box::new(MarkdownMemory::new(workspace_dir))),
        MemoryBackendKind::None => Ok(Box::new(NoneMemory::new())),
        MemoryBackendKind::Unknown => /* fallback to markdown */
    }
}
```

**Why this works for Wakey:**
- **Simple**: No registry boilerplate, just match on string name
- **Config-driven**: TOML config selects implementation
- **Compile-time safety**: New providers require code changes (explicit, reviewable)

**What we should do differently:**
- Consider a `register_provider()` function for community extensions
- WASM sandbox for untrusted skill providers (already planned in Wakey)

---

## 2. Event/Message Routing

### 2.1 Channel Message Flow

ZeroClaw does NOT use a central event bus. Instead, it uses **direct channel-to-agent communication via tokio mpsc**:

```
┌──────────────┐     mpsc::Sender      ┌──────────────┐
│  Telegram    │──────────────────────▶│              │
│  Discord     │                        │   Channel   │
│  Slack       │──────────────────────▶│   Worker    │
│  CLI         │                        │   Pool      │
└──────────────┘                        └──────────────┘
                                              │
                                              ▼
                                        ┌──────────────┐
                                        │   Provider   │
                                        │   (LLM API) │
                                        └──────────────┘
```

**Code pattern** (`src/channels/mod.rs`):

```rust
// Channel listener sends to mpsc channel
async fn listen(&self, tx: tokio::sync::mpsc::Sender<ChannelMessage>) 
    -> anyhow::Result<()>;

// Worker receives and processes
while let Some(msg) = rx.recv().await {
    process_channel_message(ctx.clone(), msg, cancellation_token).await;
}
```

### 2.2 Conversation History Management

Per-sender history stored in `Mutex<HashMap<String, Vec<ChatMessage>>>`:

```rust
type ConversationHistoryMap = Arc<Mutex<HashMap<String, Vec<ChatMessage>>>>;

const MAX_CHANNEL_HISTORY: usize = 50;

fn append_sender_turn(ctx: &ChannelRuntimeContext, sender_key: &str, turn: ChatMessage) {
    let mut histories = ctx.conversation_histories.lock().unwrap();
    let turns = histories.entry(sender_key.to_string()).or_default();
    turns.push(turn);
    while turns.len() > MAX_CHANNEL_HISTORY {
        turns.remove(0);
    }
}
```

**Why this works:**
- Simple, in-memory, no persistence layer complexity
- Per-sender isolation prevents cross-talk
- Bounded history prevents unbounded growth

### 2.3 Agent Loop Pattern

The agent loop (`src/agent/loop_.rs`) handles tool calling:

```rust
pub async fn run_tool_call_loop(
    provider: &dyn Provider,
    messages: &mut Vec<ChatMessage>,
    tools: &[Box<dyn Tool>],
    max_iterations: usize,
    // ... callbacks for streaming
) -> Result<String> {
    for _ in 0..max_iterations {
        let response = provider.chat(
            ChatRequest { messages, tools: Some(&tool_specs) },
            model, temperature
        ).await?;
        
        if !response.has_tool_calls() {
            return Ok(response.text_or_empty().to_string());
        }
        
        // Execute each tool call
        for tool_call in response.tool_calls {
            let result = execute_tool(&tool_call, tools).await?;
            messages.push(ChatMessage::tool(result));
        }
    }
    
    Err(anyhow!("Max tool iterations exceeded"))
}
```

**For Wakey:**
- **Adopt**: This exact pattern for cortex tool execution
- **Enhance**: Add Cedar policy check before tool execution (already designed)
- **Enhance**: Add observability events for each iteration

---

## 3. Provider Abstraction (LLM Client)

### 3.1 OpenAI-Compatible Client

ZeroClaw's `OpenAiCompatibleProvider` is the workhorse — it supports 30+ providers:

```rust
pub struct OpenAiCompatibleProvider {
    pub name: String,
    pub base_url: String,
    pub credential: Option<String>,
    pub auth_header: AuthStyle,
    supports_vision: bool,
    native_tool_calling: bool,
}

pub enum AuthStyle {
    Bearer,           // Authorization: Bearer <key>
    XApiKey,          // x-api-key: <key>
    Custom(String),   // Custom header
}
```

**Factory usage:**

```rust
// Groq
"groq" => Ok(Box::new(OpenAiCompatibleProvider::new(
    "Groq", "https://api.groq.com/openai/v1", key, AuthStyle::Bearer
))),

// Moonshot (Chinese provider)
name if moonshot_base_url(name).is_some() => Ok(Box::new(OpenAiCompatibleProvider::new(
    "Moonshot", moonshot_base_url(name).unwrap(), key, AuthStyle::Bearer
))),

// Custom endpoint
name if name.starts_with("custom:") => {
    let base_url = name.strip_prefix("custom:").unwrap();
    Ok(Box::new(OpenAiCompatibleProvider::new(...)))
}
```

**For Wakey:**
- **Adopt**: Single HTTP client for all OpenAI-compatible providers
- **Adopt**: `AuthStyle` enum for different auth headers
- **Adopt**: `custom:` prefix for user-defined endpoints

### 3.2 Streaming Implementation

Provider trait supports streaming via `StreamChunk`:

```rust
pub struct StreamChunk {
    pub delta: String,
    pub is_final: bool,
    pub token_count: usize,
}

fn stream_chat_with_system(
    &self,
    system_prompt: Option<&str>,
    message: &str,
    model: &str,
    temperature: f64,
    options: StreamOptions,
) -> stream::BoxStream<'static, StreamResult<StreamChunk>>;
```

**Implementation in OpenAiCompatibleProvider** (simplified):

```rust
let response = client
    .post(url)
    .json(&body)
    .send()
    .await?;

let stream = response.bytes_stream();
stream::unfold(stream, move |mut stream| async move {
    // Parse SSE events, yield StreamChunk
}).boxed()
```

### 3.3 Resilient Provider Wrapper

ZeroClaw wraps providers with retry/fallback:

```rust
pub struct ReliableProvider {
    providers: Vec<(String, Box<dyn Provider>)>,
    max_retries: usize,
    backoff_ms: u64,
}

async fn chat(&self, request: ChatRequest<'_>, ...) -> Result<ChatResponse> {
    for (name, provider) in &self.providers {
        for attempt in 0..self.max_retries {
            match provider.chat(request, model, temp).await {
                Ok(response) => return Ok(response),
                Err(e) if is_retryable(&e) => {
                    tokio::time::sleep(backoff).await;
                    continue;
                }
                Err(e) => break, // Try next provider
            }
        }
    }
    Err(anyhow!("All providers failed"))
}
```

**For Wakey:**
- **Adopt**: Fallback chain for provider reliability
- **Adopt**: Retry with exponential backoff
- **Add**: Circuit breaker pattern (more robust than simple retry)

---

## 4. Memory Backend

### 4.1 SQLite Schema

ZeroClaw's `SqliteMemory` is a full-stack search engine:

```sql
-- Core memories table
CREATE TABLE memories (
    id          TEXT PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    content     TEXT NOT NULL,
    category    TEXT NOT NULL DEFAULT 'core',
    embedding   BLOB,
    session_id  TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- FTS5 full-text search (BM25 scoring)
CREATE VIRTUAL TABLE memories_fts USING fts5(
    key, content, content=memories, content_rowid=rowid
);

-- Triggers to keep FTS in sync
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, key, content)
    VALUES (new.rowid, new.key, new.content);
END;

-- Embedding cache with LRU eviction
CREATE TABLE embedding_cache (
    content_hash TEXT PRIMARY KEY,
    embedding    BLOB NOT NULL,
    created_at   TEXT NOT NULL,
    accessed_at  TEXT NOT NULL
);
```

**PRAGMA tuning for performance:**

```rust
conn.execute_batch(
    "PRAGMA journal_mode = WAL;
     PRAGMA synchronous  = NORMAL;
     PRAGMA mmap_size    = 8388608;   -- 8 MB
     PRAGMA cache_size   = -2000;     -- 2 MB
     PRAGMA temp_store   = MEMORY;"
)?;
```

**For Wakey:**
- **Adopt**: Exact schema for L0/L1 tiers (fits OpenViking pattern)
- **Adopt**: PRAGMA tuning for 24/7 uptime
- **Adopt**: Embedding cache with LRU eviction

### 4.2 Hybrid Search Implementation

**Vector search** (brute-force cosine similarity):

```rust
fn vector_search(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    let mut stmt = conn.prepare(
        "SELECT id, embedding FROM memories WHERE embedding IS NOT NULL"
    )?;
    
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    
    let mut scored: Vec<(String, f32)> = Vec::new();
    for row in rows {
        let (id, blob) = row?;
        let emb = bytes_to_vec(&blob);
        let sim = cosine_similarity(query_embedding, &emb);
        if sim > 0.0 {
            scored.push((id, sim));
        }
    }
    
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.truncate(limit);
    Ok(scored)
}
```

**FTS5 BM25 keyword search:**

```rust
fn fts5_search(conn: &Connection, query: &str, limit: usize) 
    -> Result<Vec<(String, f32)>> 
{
    let fts_query: String = query
        .split_whitespace()
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    
    let sql = "SELECT m.id, bm25(memories_fts) as score
               FROM memories_fts f
               JOIN memories m ON m.rowid = f.rowid
               WHERE memories_fts MATCH ?1
               ORDER BY score
               LIMIT ?2";
    // BM25 returns negative scores (lower = better), negate for ranking
}
```

**Hybrid merge** (weighted fusion):

```rust
pub fn hybrid_merge(
    vector_results: &[(String, f32)],
    keyword_results: &[(String, f32)],
    vector_weight: f32,
    keyword_weight: f32,
    limit: usize,
) -> Vec<ScoredResult> {
    // Normalize keyword scores to [0,1]
    let max_kw = keyword_results.iter().map(|(_, s)| *s).fold(0.0, f32::max);
    
    // Deduplicate by ID, merge scores
    let mut map: HashMap<String, ScoredResult> = HashMap::new();
    for (id, score) in vector_results {
        map.entry(id.clone()).or_insert_with(|| ScoredResult {
            id: id.clone(),
            vector_score: Some(*score),
            keyword_score: None,
            final_score: 0.0,
        });
    }
    for (id, score) in keyword_results {
        let normalized = score / max_kw;
        map.entry(id.clone())
            .and_modify(|r| r.keyword_score = Some(normalized))
            .or_insert_with(|| ScoredResult { ... });
    }
    
    // Final score = weighted sum
    for r in &mut results {
        r.final_score = vector_weight * r.vector_score.unwrap_or(0.0)
                       + keyword_weight * r.keyword_score.unwrap_or(0.0);
    }
    results.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap());
    results.truncate(limit);
    results
}
```

**For Wakey:**
- **Adopt**: Hybrid search pattern (proven effective)
- **Adopt**: LRU embedding cache
- **Enhance**: Use `sqlite-vec` extension for vector indices (scales to millions)
- **Enhance**: Add HNSW index for approximate nearest neighbor (faster than brute force)

### 4.3 Embedding Provider

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>>;
    
    async fn embed_one(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut results = self.embed(&[text]).await?;
        results.pop().ok_or_else(|| anyhow!("Empty embedding result"))
    }
}
```

**Factory:**

```rust
pub fn create_embedding_provider(
    provider: &str,
    api_key: Option<&str>,
    model: &str,
    dims: usize,
) -> Box<dyn EmbeddingProvider> {
    match provider {
        "openai" => Box::new(OpenAiEmbedding::new(
            "https://api.openai.com", key, model, dims
        )),
        "openrouter" => Box::new(OpenAiEmbedding::new(
            "https://openrouter.ai/api/v1", key, model, dims
        )),
        name if name.starts_with("custom:") => {
            Box::new(OpenAiEmbedding::new(base_url, key, model, dims))
        }
        _ => Box::new(NoopEmbedding),  // Keyword-only fallback
    }
}
```

---

## 5. Build/Binary Optimization

### 5.1 Cargo Profile

```toml
[profile.release]
opt-level = "z"        # Optimize for size
lto = "fat"            # Maximum cross-crate optimization
codegen-units = 1      # Serialized codegen (smaller binary)
strip = true           # Remove debug symbols
panic = "abort"        # Reduce binary size (no unwinding)

[profile.release-fast]
inherits = "release"
codegen-units = 8      # Parallel codegen for faster builds

[profile.dist]
inherits = "release"
opt-level = "z"
lto = "fat"
codegen-units = 1
strip = true
panic = "abort"
```

**Dependency pruning:**

```toml
# Tokio - only needed features
tokio = { version = "1.42", default-features = false, features = [
    "rt-multi-thread", "macros", "time", "net", "io-util", "sync", 
    "process", "io-std", "fs", "signal"
] }

# Reqwest - rustls instead of native TLS
reqwest = { version = "0.12", default-features = false, features = [
    "json", "rustls-tls", "stream"
] }

# Serde - minimal features
serde = { version = "1.0", default-features = false, features = ["derive"] }
```

**For Wakey:**
- **Adopt**: Exact profile configuration
- **Add**: `upx` compression for further size reduction (optional)
- **Measure**: Track binary size in CI

### 5.2 Feature Flags

```toml
[features]
default = []
hardware = ["nusb", "tokio-serial"]
channel-matrix = ["dep:matrix-sdk"]
memory-postgres = ["dep:postgres"]
observability-otel = ["dep:opentelemetry", "dep:opentelemetry_sdk"]
browser-native = ["dep:fantoccini"]
whatsapp-web = ["dep:wa-rs", "dep:wa-rs-core", ...]
```

**Why this works:**
- Users only compile what they need
- Keeps base binary small
- Enables community contributions without bloating core

---

## 6. Key Recommendations for Wakey

### Adopt Directly

| Pattern | ZeroClaw | Wakey Adaptation |
|---------|----------|------------------|
| Tool trait | `async_trait`, `Send + Sync` | Same, add Cedar policy hook |
| Provider trait | Capabilities + default methods | Same, add streaming-first |
| Memory trait | CRUD + session scoping | Same for L0/L1 tiers |
| OpenAI-compatible client | Single HTTP client for 30+ providers | Same with `AuthStyle` enum |
| Hybrid search | FTS5 + vector + weighted merge | Same + HNSW index |
| Build profile | `opt-level="z"`, LTO, strip | Same |

### Adapt for Wakey

| Pattern | ZeroClaw | Wakey Adaptation |
|---------|----------|------------------|
| Event system | Direct mpsc channels | Event spine (tokio broadcast) for crate isolation |
| Provider factory | Match on string | Match + WASM sandbox for extensions |
| Memory backend | SQLite only | Tiered: SQLite (L0/L1) + Viking (L2) |
| Heartbeat | Simple tick + file read | Multi-rhythm: tick/breath/reflect/dream |
| Safety | N/A | Cedar policy engine before every action |

### Do Differently

| Area | ZeroClaw | Wakey Reason |
|------|----------|--------------|
| Event routing | Direct calls | Wakey needs crate isolation via spine |
| Safety | None | Cedar policies for action filtering |
| Persona | Static system prompt | Evolving personality from experience |
| Learning | N/A | Skill extraction from successful tool use |
| Heartbeat | Single interval | Multiple rhythms for different cognition levels |

---

## 7. Code Reference

All snippets in this report are from ZeroClaw source:

- `src/tools/traits.rs` — Tool trait definition
- `src/providers/traits.rs` — Provider trait with capabilities
- `src/providers/mod.rs` — Provider factory
- `src/providers/compatible.rs` — OpenAI-compatible client
- `src/memory/traits.rs` — Memory trait
- `src/memory/sqlite.rs` — SQLite + FTS5 + vector implementation
- `src/memory/vector.rs` — Cosine similarity + hybrid merge
- `src/memory/embeddings.rs` — Embedding provider trait
- `src/channels/traits.rs` — Channel trait
- `src/channels/mod.rs` — Channel worker + history management
- `src/agent/loop_.rs` — Tool calling loop
- `src/heartbeat/engine.rs` — Heartbeat implementation
- `Cargo.toml` — Build profiles

---

## Appendix: Quick Reference Snippets

### Creating a Provider

```rust
let provider = providers::create_provider("openrouter", Some("sk-or-..."))?;
let response = provider.chat_with_system(
    Some("You are helpful"),
    "Hello!",
    "anthropic/claude-sonnet-4",
    0.7
).await?;
```

### Creating Memory

```rust
let config = MemoryConfig { backend: "sqlite".into(), ... };
let memory = memory::create_memory(&config, &workspace_dir, None)?;
memory.store("user_pref", "Likes Rust", MemoryCategory::Core, None).await?;
let results = memory.recall("programming preferences", 5, None).await?;
```

### Hybrid Search Weights

```rust
// Default weights from ZeroClaw
let vector_weight = 0.7;   // Semantic similarity
let keyword_weight = 0.3;  // BM25 keyword match
```

### Embedding Cache LRU

```rust
// Default cache size
const EMBEDDING_CACHE_SIZE: usize = 10_000;

// Eviction query
"DELETE FROM embedding_cache WHERE content_hash IN (
    SELECT content_hash FROM embedding_cache
    ORDER BY accessed_at ASC
    LIMIT MAX(0, (SELECT COUNT(*) FROM embedding_cache) - ?)
)"
```