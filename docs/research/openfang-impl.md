# OpenFang Implementation Patterns Research

**Date**: 2025-04-03  
**Source**: `/home/dipendra-sharma/projects/openfang/`  
**Purpose**: Extract architectural patterns for Wakey's multi-crate Rust workspace

---

## Executive Summary

OpenFang is a mature 14-crate Rust workspace implementing an "Agent Operating System" with:
- **Strict dependency hierarchy** (no cycles, types crate at the bottom)
- **WASM sandboxed skills** with fuel metering and capability-based security
- **Event-driven architecture** using tokio broadcast channels
- **Multi-layer security**: capabilities + taint tracking + approval gates
- **Tauri 2.0 desktop** with embedded HTTP server (not webview assets)

Wakey can directly adopt most patterns with simplifications for our narrower scope.

---

## 1. Workspace Structure

### 1.1 Cargo.toml Organization

```toml
# Root Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/openfang-types",
    "crates/openfang-memory",
    "crates/openfang-runtime",
    "crates/openfang-wire",
    "crates/openfang-api",
    "crates/openfang-kernel",
    "crates/openfang-cli",
    "crates/openfang-channels",
    "crates/openfang-migrate",
    "crates/openfang-skills",
    "crates/openfang-desktop",
    "crates/openfang-hands",
    "crates/openfang-extensions",
    "xtask",
]

[workspace.package]
version = "0.3.49"
edition = "2021"
license = "Apache-2.0 OR MIT"
rust-version = "1.75"

[workspace.dependencies]
# Shared dependency versions
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
thiserror = "2"
anyhow = "1"
# ... more deps
```

**Key Patterns**:
- `workspace.dependencies` centralizes version management
- `workspace.package` defines shared metadata (version, edition, license)
- Each crate uses `version.workspace = true` to inherit

### 1.2 Dependency Hierarchy

```
openfang-types          (depends on nothing)
    ↓
openfang-spine          (depends on types) — NOT PRESENT, uses direct imports
    ↓
openfang-memory         (depends on types)
openfang-skills         (depends on types)
    ↓
openfang-runtime        (depends on types, memory, skills)
    ↓
openfang-kernel         (depends on types, memory, runtime, skills, hands, extensions, wire, channels)
    ↓
openfang-desktop        (depends on kernel)
openfang-cli            (depends on kernel)
openfang-api            (depends on kernel)
```

**For Wakey**: Adopt the same strict layering. Our `wakey-types` should have zero dependencies on other crates.

### 1.3 Types Crate Structure

```rust
// crates/openfang-types/src/lib.rs
pub mod agent;
pub mod approval;
pub mod capability;
pub mod comms;
pub mod config;
pub mod error;
pub mod event;
pub mod manifest_signing;
pub mod media;
pub mod memory;
pub mod message;
pub mod model_catalog;
pub mod scheduler;
pub mod serde_compat;
pub mod taint;
pub mod tool;
pub mod tool_compat;
pub mod webhook;
```

**Key Pattern**: All shared types live in one crate. No type re-exports across crates.

**For Wakey**: Our `wakey-types` should follow this exact pattern — all events, errors, config types, and domain types in one place.

---

## 2. WASM Skill Sandbox

### 2.1 Sandbox Architecture

OpenFang uses **Wasmtime** with three-layer protection:

1. **Fuel metering** — deterministic CPU instruction limit
2. **Epoch interruption** — wall-clock timeout via background thread
3. **Capability checks** — every host call validates permissions

```rust
// crates/openfang-runtime/src/sandbox.rs

pub struct SandboxConfig {
    /// Maximum fuel (CPU instruction budget). 0 = unlimited.
    pub fuel_limit: u64,
    /// Maximum WASM linear memory in bytes.
    pub max_memory_bytes: usize,
    /// Capabilities granted to this sandbox instance.
    pub capabilities: Vec<Capability>,
    /// Wall-clock timeout in seconds for epoch-based interruption.
    pub timeout_secs: Option<u64>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            fuel_limit: 1_000_000,  // 1M instructions default
            max_memory_bytes: 16 * 1024 * 1024,  // 16MB
            capabilities: Vec::new(),
            timeout_secs: None,
        }
    }
}
```

### 2.2 Guest ABI (What WASM modules must export)

```rust
// Required exports:
// - `memory` — linear memory
// - `alloc(size: i32) -> i32` — allocate bytes, return pointer
// - `execute(input_ptr: i32, input_len: i32) -> i64` — main entry point

// The `execute` function returns a packed i64:
// (result_ptr << 32) | result_len
```

### 2.3 Host ABI (What the sandbox provides)

```rust
// In the "openfang" import module:
// - `host_call(request_ptr: i32, request_len: i32) -> i64` — RPC dispatch
// - `host_log(level: i32, msg_ptr: i32, msg_len: i32)` — logging

// host_call reads JSON: {"method": "...", "params": {...}}
// Returns JSON: {"ok": ...} or {"error": "..."}
```

### 2.4 Engine Creation

```rust
pub struct WasmSandbox {
    engine: Engine,
}

impl WasmSandbox {
    pub fn new() -> Result<Self, SandboxError> {
        let mut config = Config::new();
        config.consume_fuel(true);        // Enable deterministic metering
        config.epoch_interruption(true);  // Enable wall-clock timeouts
        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }
}
```

### 2.5 Execution Flow

```rust
pub async fn execute(
    &self,
    wasm_bytes: &[u8],
    input: serde_json::Value,
    config: SandboxConfig,
    kernel: Option<Arc<dyn KernelHandle>>,
    agent_id: &str,
) -> Result<ExecutionResult, SandboxError> {
    // Spawn on blocking thread (WASM is CPU-bound)
    tokio::task::spawn_blocking(move || {
        // 1. Compile module
        let module = Module::new(engine, wasm_bytes)?;
        
        // 2. Create store with fuel limit
        let mut store = Store::new(engine, GuestState { ... });
        store.set_fuel(config.fuel_limit)?;
        
        // 3. Set epoch deadline + spawn watchdog
        store.set_epoch_deadline(1);
        let engine_clone = engine.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout));
            engine_clone.increment_epoch();
        });
        
        // 4. Build linker with host functions
        let mut linker = Linker::new(engine);
        Self::register_host_functions(&mut linker)?;
        
        // 5. Instantiate and call execute()
        let instance = linker.instantiate(&mut store, &module)?;
        let execute_fn = instance.get_typed_func::<(i32, i32), i64>(&mut store, "execute")?;
        
        // 6. Allocate input in guest memory, call, read result
        // ...
    }).await?
}
```

**For Wakey**: Adopt this exact pattern. Wasmtime is the right choice. The fuel + epoch combination provides both deterministic and wall-clock protection.

---

## 3. Capability-Based Security

### 3.1 Capability Types

```rust
// crates/openfang-types/src/capability.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Capability {
    // File system
    FileRead(String),   // Glob pattern: "/data/*"
    FileWrite(String),
    
    // Network
    NetConnect(String), // "api.openai.com:443"
    NetListen(u16),
    
    // Tools
    ToolInvoke(String), // Specific tool ID
    ToolAll,            // Any tool (dangerous)
    
    // LLM
    LlmQuery(String),
    LlmMaxTokens(u64),
    
    // Agent interaction
    AgentSpawn,
    AgentMessage(String),
    AgentKill(String),
    
    // Memory
    MemoryRead(String),
    MemoryWrite(String),
    
    // Shell
    ShellExec(String),
    EnvRead(String),
    
    // Economic
    EconSpend(f64),
}
```

### 3.2 Capability Matching

```rust
pub fn capability_matches(granted: &Capability, required: &Capability) -> bool {
    match (granted, required) {
        // ToolAll grants any ToolInvoke
        (Capability::ToolAll, Capability::ToolInvoke(_)) => true,
        
        // Glob pattern matching
        (Capability::FileRead(pattern), Capability::FileRead(path)) => {
            glob_matches(pattern, path)
        }
        
        // Numeric bounds
        (Capability::LlmMaxTokens(granted), Capability::LlmMaxTokens(required)) => {
            granted >= required
        }
        
        // Exact match
        _ if granted == required => true,
        _ => false,
    }
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" { return true; }
    if pattern == value { return true; }
    // Prefix/suffix/middle wildcard matching
    // ...
}
```

### 3.3 Capability Inheritance (Anti-Escalation)

```rust
/// Validate that child capabilities are a subset of parent.
/// Prevents privilege escalation: restricted parent cannot create unrestricted child.
pub fn validate_capability_inheritance(
    parent_caps: &[Capability],
    child_caps: &[Capability],
) -> Result<(), String> {
    for child_cap in child_caps {
        let is_covered = parent_caps
            .iter()
            .any(|parent_cap| capability_matches(parent_cap, child_cap));
        if !is_covered {
            return Err(format!(
                "Privilege escalation denied: child requests {:?} but parent lacks it",
                child_cap
            ));
        }
    }
    Ok(())
}
```

**For Wakey**: Adopt this pattern directly. The capability enum should map to our planned actions (screen capture, mouse/keyboard, file access, etc.).

---

## 4. Taint Tracking

### 4.1 Taint Labels

```rust
// crates/openfang-types/src/taint.rs

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaintLabel {
    ExternalNetwork,  // Data from network
    UserInput,        // Direct user input
    Pii,              // Personally identifiable info
    Secret,           // API keys, tokens, passwords
    UntrustedAgent,   // From sandboxed/untrusted agent
}
```

### 4.2 Tainted Values

```rust
pub struct TaintedValue {
    pub value: String,
    pub labels: HashSet<TaintLabel>,
    pub source: String,  // Human-readable origin
}

impl TaintedValue {
    /// Check whether this value is safe for the given sink.
    pub fn check_sink(&self, sink: &TaintSink) -> Result<(), TaintViolation> {
        for label in &self.labels {
            if sink.blocked_labels.contains(label) {
                return Err(TaintViolation { label, sink_name: sink.name, source: self.source });
            }
        }
        Ok(())
    }
    
    /// Remove a label (explicit declassification).
    pub fn declassify(&mut self, label: &TaintLabel) {
        self.labels.remove(label);
    }
}
```

### 4.3 Predefined Sinks

```rust
pub struct TaintSink {
    pub name: String,
    pub blocked_labels: HashSet<TaintLabel>,
}

impl TaintSink {
    /// Shell execution — blocks external/untrusted/user input
    pub fn shell_exec() -> Self {
        Self {
            name: "shell_exec".to_string(),
            blocked_labels: hashset![ExternalNetwork, UntrustedAgent, UserInput],
        }
    }
    
    /// Network fetch — blocks secrets and PII (exfiltration prevention)
    pub fn net_fetch() -> Self {
        Self {
            name: "net_fetch".to_string(),
            blocked_labels: hashset![Secret, Pii],
        }
    }
}
```

**For Wakey**: Critical pattern for preventing prompt injection and data exfiltration. Our screen capture data should carry `ExternalNetwork` or `UserInput` taint and be blocked from shell execution.

---

## 5. Host Functions (WASM ↔ Rust Bridge)

### 5.1 Dispatch Pattern

```rust
// crates/openfang-runtime/src/host_functions.rs

pub fn dispatch(state: &GuestState, method: &str, params: &serde_json::Value) -> serde_json::Value {
    match method {
        // Always allowed (no capability check)
        "time_now" => host_time_now(),
        
        // Filesystem — requires FileRead/FileWrite
        "fs_read" => host_fs_read(state, params),
        "fs_write" => host_fs_write(state, params),
        
        // Network — requires NetConnect + SSRF protection
        "net_fetch" => host_net_fetch(state, params),
        
        // Shell — requires ShellExec
        "shell_exec" => host_shell_exec(state, params),
        
        _ => json!({"error": format!("Unknown host method: {method}")}),
    }
}
```

### 5.2 Capability Check + SSRF Protection

```rust
fn host_net_fetch(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let url = params.get("url").and_then(|u| u.as_str())?;
    
    // SECURITY: SSRF protection — check resolved IP
    if let Err(e) = is_ssrf_target(url) {
        return e;
    }
    
    // Capability check
    let host = extract_host_from_url(url);
    if let Err(e) = check_capability(&state.capabilities, &Capability::NetConnect(host)) {
        return e;
    }
    
    // Execute request...
}

fn is_ssrf_target(url: &str) -> Result<(), serde_json::Value> {
    // Block localhost, private IPs, metadata endpoints
    let blocked_hostnames = [
        "localhost",
        "metadata.google.internal",
        "metadata.aws.internal",
        "169.254.169.254",  // AWS/GCP metadata IP
    ];
    
    // Resolve DNS and check every returned IP
    for addr in socket_addr.to_socket_addrs()? {
        if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
            return Err(json!({"error": "SSRF blocked"}));
        }
    }
    Ok(())
}
```

### 5.3 Path Traversal Protection

```rust
fn host_fs_read(state: &GuestState, params: &serde_json::Value) -> serde_json::Value {
    let path = params.get("path").and_then(|p| p.as_str())?;
    
    // 1. Capability check with raw path
    check_capability(&state.capabilities, &Capability::FileRead(path.to_string()))?;
    
    // 2. SECURITY: Reject path traversal AFTER capability gate
    let canonical = safe_resolve_path(path)?;
    
    std::fs::read_to_string(&canonical)
}

fn safe_resolve_path(path: &str) -> Result<PathBuf, serde_json::Value> {
    let p = Path::new(path);
    
    // Reject any ".." components
    for component in p.components() {
        if matches!(component, Component::ParentDir) {
            return Err(json!({"error": "Path traversal denied"}));
        }
    }
    
    // Canonicalize to resolve symlinks
    std::fs::canonicalize(p).map_err(|e| json!({"error": format!("Cannot resolve: {e}")}))
}
```

**For Wakey**: This pattern of "check capability → sanitize input → execute" should be our standard for all host functions.

---

## 6. Event-Driven Architecture

### 6.1 Event Bus

```rust
// crates/openfang-kernel/src/event_bus.rs

pub struct EventBus {
    sender: broadcast::Sender<Event>,
    agent_channels: DashMap<AgentId, broadcast::Sender<Event>>,
    history: Arc<RwLock<VecDeque<Event>>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1024);
        Self {
            sender,
            agent_channels: DashMap::new(),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
        }
    }
    
    pub async fn publish(&self, event: Event) {
        // Store in history ring buffer
        {
            let mut history = self.history.write().await;
            if history.len() >= HISTORY_SIZE {
                history.pop_front();
            }
            history.push_back(event.clone());
        }
        
        // Route to target
        match &event.target {
            EventTarget::Agent(agent_id) => {
                if let Some(sender) = self.agent_channels.get(agent_id) {
                    let _ = sender.send(event);
                }
            }
            EventTarget::Broadcast => {
                let _ = self.sender.send(event);
                for entry in self.agent_channels.iter() {
                    let _ = entry.value().send(event.clone());
                }
            }
            EventTarget::System => {
                let _ = self.sender.send(event);
            }
            _ => {}
        }
    }
    
    pub fn subscribe_agent(&self, agent_id: AgentId) -> broadcast::Receiver<Event> {
        self.agent_channels.entry(agent_id).or_insert_with(|| {
            broadcast::channel(256).0
        }).subscribe()
    }
}
```

### 6.2 Event Types

```rust
// crates/openfang-types/src/event.rs

pub struct Event {
    pub id: EventId,
    pub source: AgentId,
    pub target: EventTarget,
    pub payload: EventPayload,
    pub timestamp: DateTime<Utc>,
    pub correlation_id: Option<EventId>,
    pub ttl: Option<Duration>,
}

pub enum EventTarget {
    Agent(AgentId),
    Broadcast,
    Pattern(String),
    System,
}

pub enum EventPayload {
    Message(AgentMessage),
    ToolResult(ToolOutput),
    MemoryUpdate(MemoryDelta),
    Lifecycle(LifecycleEvent),
    Network(NetworkEvent),
    System(SystemEvent),
    Custom(Vec<u8>),
}
```

**For Wakey**: Replace our planned "spine" concept with this exact pattern. The event bus + typed events is cleaner than trait-based communication.

---

## 7. Memory Substrate

### 7.1 Unified Memory Trait

```rust
// crates/openfang-types/src/memory.rs

#[async_trait]
pub trait Memory: Send + Sync {
    async fn get(&self, agent_id: AgentId, key: &str) -> Result<Option<Value>>;
    async fn set(&self, agent_id: AgentId, key: &str, value: Value) -> Result<()>;
    async fn delete(&self, agent_id: AgentId, key: &str) -> Result<()>;
    
    async fn remember(&self, agent_id: AgentId, content: &str, source: MemorySource, scope: &str, metadata: HashMap<String, Value>) -> Result<MemoryId>;
    async fn recall(&self, query: &str, limit: usize, filter: Option<MemoryFilter>) -> Result<Vec<MemoryFragment>>;
    async fn forget(&self, id: MemoryId) -> Result<()>;
    
    async fn add_entity(&self, entity: Entity) -> Result<String>;
    async fn add_relation(&self, relation: Relation) -> Result<String>;
    async fn query_graph(&self, pattern: GraphPattern) -> Result<Vec<GraphMatch>>;
    
    async fn consolidate(&self) -> Result<ConsolidationReport>;
}
```

### 7.2 Substrate Implementation

```rust
// crates/openfang-memory/src/substrate.rs

pub struct MemorySubstrate {
    conn: Arc<Mutex<Connection>>,  // SQLite
    structured: StructuredStore,    // KV pairs
    semantic: SemanticStore,        // Vector search (Phase 1: LIKE, Phase 2: embeddings)
    knowledge: KnowledgeStore,      // Entity-relation graph
    sessions: SessionStore,         // Conversation sessions
    consolidation: ConsolidationEngine,
    usage: UsageStore,              // Token/cost tracking
}

impl MemorySubstrate {
    pub fn open(db_path: &Path, decay_rate: f32) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        run_migrations(&conn)?;
        // ...
    }
}
```

**For Wakey**: Adopt the unified trait pattern. Our L0/L1/L2 tiers can map to:
- L0 (working memory): In-memory HashMap
- L1 (session): SQLite like OpenFang
- L2 (long-term): Vector store for semantic recall

---

## 8. Approval System (Human-in-the-Loop)

### 8.1 Approval Manager

```rust
// crates/openfang-kernel/src/approval.rs

pub struct ApprovalManager {
    pending: DashMap<Uuid, PendingRequest>,
    policy: RwLock<ApprovalPolicy>,
}

pub struct ApprovalRequest {
    pub id: Uuid,
    pub agent_id: String,
    pub tool_name: String,
    pub description: String,
    pub action_summary: String,
    pub risk_level: RiskLevel,
    pub timeout_secs: u64,
}

pub enum ApprovalDecision {
    Approved,
    Denied,
    TimedOut,
}
```

### 8.2 Request Flow

```rust
impl ApprovalManager {
    /// Submit request, returns future that resolves when approved/denied/timed out
    pub async fn request_approval(&self, req: ApprovalRequest) -> ApprovalDecision {
        let (tx, rx) = oneshot::channel();
        self.pending.insert(req.id, PendingRequest { request: req, sender: tx });
        
        match tokio::time::timeout(Duration::from_secs(req.timeout_secs), rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                self.pending.remove(&req.id);
                ApprovalDecision::TimedOut
            }
        }
    }
    
    /// Resolve from API/UI
    pub fn resolve(&self, request_id: Uuid, decision: ApprovalDecision) -> Result<ApprovalResponse, String> {
        let (_, pending) = self.pending.remove(&request_id).ok_or("Not found")?;
        pending.sender.send(decision).ok();
        Ok(ApprovalResponse { request_id, decision, ... })
    }
}
```

### 8.3 Risk Classification

```rust
pub fn classify_risk(tool_name: &str) -> RiskLevel {
    match tool_name {
        "shell_exec" => RiskLevel::Critical,
        "file_write" | "file_delete" => RiskLevel::High,
        "web_fetch" | "browser_navigate" => RiskLevel::Medium,
        _ => RiskLevel::Low,
    }
}
```

**For Wakey**: Essential for safe autonomous operation. Our high-risk actions (shell, file write, mouse/keyboard) should require approval.

---

## 9. Desktop App (Tauri 2.0)

### 9.1 Architecture

```rust
// crates/openfang-desktop/src/lib.rs

pub fn run() {
    // Boot kernel + embedded server FIRST
    let server_handle = server::start_server().expect("Failed to start server");
    let port = server_handle.port;
    let kernel = server_handle.kernel.clone();
    
    let url = format!("http://127.0.0.1:{port}");
    
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().args(["--minimized"]).build())
        .manage(KernelState { kernel, started_at: Instant::now() })
        .invoke_handler(tauri::generate_handler![
            commands::get_port,
            commands::get_status,
            commands::import_agent_toml,
            // ...
        ])
        .setup(move |app| {
            // Create window pointing at embedded HTTP server
            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url.parse().unwrap()),
            )
            .title("OpenFang")
            .inner_size(1280.0, 800.0)
            .build()?;
            
            // Forward kernel events to OS notifications
            spawn_notification_forwarder(app.handle(), kernel);
            
            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide to tray on close instead of quitting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("Failed to build Tauri app")
        .run(|_app, _event| {});
    
    // Cleanup
    server_handle.shutdown();
}
```

### 9.2 Key Design Decisions

1. **Embedded HTTP server** — Tauri window navigates to `http://127.0.0.1:{port}`, not embedded assets
2. **System tray** — Hide to tray on close, don't quit
3. **Single instance** — Second launch focuses existing window
4. **Auto-start** — Register with OS to start on login
5. **Native notifications** — Forward critical kernel events

### 9.3 IPC Commands

```rust
#[tauri::command]
pub fn get_status(kernel: tauri::State<'_, KernelState>) -> Value {
    json!({
        "status": "running",
        "agents": kernel.kernel.registry.list().len(),
        "uptime_secs": kernel.started_at.elapsed().as_secs(),
    })
}

#[tauri::command]
pub fn import_agent_toml(app: tauri::AppHandle, kernel: tauri::State<'_, KernelState>) -> Result<String, String> {
    let path = app.dialog().file().blocking_pick_file()?;
    let content = std::fs::read_to_string(path)?;
    let manifest: AgentManifest = toml::from_str(&content)?;
    kernel.kernel.spawn_agent(manifest)?;
    Ok(manifest.name)
}
```

**For Wakey**: This pattern is ideal for our overlay. Key differences:
- We need always-on-top window, not normal window
- We need transparent/frameless window for sprite overlay
- Consider egui or iced for pure Rust UI instead of webview

---

## 10. Skill System

### 10.1 Skill Manifest

```toml
# skill.toml
[skill]
name = "web-summarizer"
version = "0.1.0"
description = "Summarizes web pages"
author = "community"
license = "MIT"

[runtime]
type = "python"  # or "wasm", "node", "builtin", "promptonly"
entry = "src/main.py"

[[tools.provided]]
name = "summarize_url"
description = "Fetch and summarize a URL"
input_schema = { type = "object", properties = { url = { type = "string" } }, required = ["url"] }

[requirements]
tools = ["web_fetch"]
capabilities = ["NetConnect(*)"]
```

### 10.2 Skill Registry

```rust
pub struct SkillRegistry {
    skills: HashMap<String, InstalledSkill>,
    skills_dir: PathBuf,
    frozen: bool,  // Stable mode: no new skills after boot
}

impl SkillRegistry {
    pub fn load_all(&mut self) -> Result<usize, SkillError> {
        for entry in fs::read_dir(&self.skills_dir)? {
            let path = entry.path();
            if path.join("skill.toml").exists() {
                self.load_skill(&path)?;
            } else if path.join("SKILL.md").exists() {
                // Auto-convert OpenClaw format
                let converted = openclaw_compat::convert_skillmd(&path)?;
                openclaw_compat::write_openfang_manifest(&path, &converted.manifest)?;
            }
        }
        Ok(count)
    }
}
```

### 10.3 Skill Execution

```rust
pub async fn execute_skill_tool(
    manifest: &SkillManifest,
    skill_dir: &Path,
    tool_name: &str,
    input: &Value,
) -> Result<SkillToolResult, SkillError> {
    match manifest.runtime.runtime_type {
        SkillRuntime::Python => execute_python(skill_dir, &manifest.runtime.entry, tool_name, input).await,
        SkillRuntime::Wasm => execute_wasm(skill_dir, &manifest.runtime.entry, input).await,
        SkillRuntime::PromptOnly => Ok(SkillToolResult {
            output: json!({"note": "Use built-in tools directly"}),
            is_error: false,
        }),
        _ => Err(SkillError::RuntimeNotAvailable(...)),
    }
}
```

**For Wakey**: Adopt the TOML manifest format. Support `PromptOnly` (instructions in system prompt) and `Wasm` (sandboxed execution). Python skills are less important for our use case.

---

## 11. Heartbeat Protocol

OpenFang's heartbeat is a **monitoring system**, not a consciousness cycle:

```rust
// crates/openfang-kernel/src/heartbeat.rs

pub fn check_agents(registry: &AgentRegistry, config: &HeartbeatConfig) -> Vec<HeartbeatStatus> {
    let now = Utc::now();
    
    for entry in registry.list() {
        if entry.state != AgentState::Running { continue; }
        
        let inactive_secs = (now - entry.last_active).num_seconds();
        let timeout_secs = entry.manifest.autonomous
            .map(|a| a.heartbeat_interval_secs * 2)
            .unwrap_or(config.default_timeout_secs);
        
        if inactive_secs > timeout_secs {
            warn!("Agent {} is unresponsive", entry.name);
            // Publish HealthCheckFailed event
        }
    }
}
```

**For Wakey**: Our "heartbeat" is different — it's continuous consciousness with multiple rhythms (tick/breath/reflect/dream). OpenFang's pattern is for health monitoring; we need a more sophisticated approach.

---

## 12. Recommendations for Wakey

### Adopt Directly

| Pattern | OpenFang Implementation | Wakey Adaptation |
|---------|------------------------|------------------|
| Workspace structure | 14 crates with strict hierarchy | Same pattern, fewer crates |
| WASM sandbox | Wasmtime + fuel + epoch + capabilities | Same, maybe stricter defaults |
| Capability system | Tagged enum + glob matching | Same, add screen/mouse/keyboard capabilities |
| Taint tracking | Labels + sinks + declassification | Same, critical for prompt injection defense |
| Event bus | tokio broadcast + typed events | Same, replace planned "spine" |
| Memory trait | Async trait + SQLite substrate | Same, add vector store for L2 |
| Approval system | DashMap + oneshot channels | Same, for dangerous actions |

### Modify for Wakey's Scope

| Pattern | OpenFang | Wakey |
|---------|----------|-------|
| Heartbeat | Health monitoring | Consciousness cycles (tick/breath/reflect/dream) |
| Skills | Python + WASM + PromptOnly | WASM + PromptOnly (no Python) |
| Desktop | Tauri + embedded HTTP server | Native overlay (iced/egui, no webview) |
| Channels | Discord/Slack/Telegram | Voice (STT/TTS) + screen perception |

### Skip for Now

- **MCP server support** — Wakey doesn't need external tool servers initially
- **A2A protocol** — No multi-agent orchestration needed initially
- **Economic model** — No payment/cost tracking needed
- **Workflow engine** — Start with simpler action patterns

---

## 13. Code Examples for Wakey

### Capability Enum for Wakey

```rust
// wakey-types/src/capability.rs

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Capability {
    // Screen perception
    ScreenCapture,          // Can capture screen
    ScreenRegion(u32, u32, u32, u32),  // Specific region only
    
    // Input actions
    MouseMove,
    MouseClick,
    KeyboardType,
    KeyboardHotkey(String),  // Specific hotkey allowed
    
    // File system
    FileRead(String),   // Glob pattern
    FileWrite(String),
    
    // Network (for LLM calls)
    NetConnect(String),
    
    // Voice
    AudioCapture,
    AudioPlay,
    
    // System
    ShellExec(String),
    EnvRead(String),
}
```

### Event Types for Wakey

```rust
// wakey-types/src/event.rs

pub enum EventPayload {
    // Perception
    ScreenCaptured { frame: Vec<u8>, bounds: (u32, u32, u32, u32) },
    AudioCaptured { samples: Vec<f32> },
    TextRecognized { text: String, source: String },
    
    // Action
    MouseMoved { x: i32, y: i32 },
    MouseClicked { button: MouseButton, x: i32, y: i32 },
    KeyPressed { key: KeyCode },
    TextTyped { text: String },
    
    // Consciousness
    TickFired { tick_num: u64 },
    BreathFired { breath_num: u64 },
    ReflectFired { insights: Vec<String> },
    DreamFired { memories_consolidated: usize },
    
    // Memory
    MemoryStored { key: String, scope: MemoryScope },
    MemoryRecalled { query: String, results: usize },
    
    // LLM
    LlmRequest { prompt_tokens: usize },
    LlmResponse { completion_tokens: usize, model: String },
    
    // Lifecycle
    AgentSpawned { name: String },
    AgentCrashed { error: String },
}
```

### Simplified WASM Sandbox for Wakey

```rust
// wakey-skills/src/sandbox.rs

pub struct WakeySandbox {
    engine: Engine,
}

impl WakeySandbox {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        config.max_wasm_stack(1024 * 64);  // 64KB stack
        Ok(Self { engine: Engine::new(&config)? })
    }
    
    pub async fn execute(
        &self,
        wasm_bytes: &[u8],
        input: Value,
        capabilities: Vec<Capability>,
    ) -> Result<Value> {
        tokio::task::spawn_blocking(move || {
            // Compile, instantiate, execute with strict defaults
            let config = SandboxConfig {
                fuel_limit: 500_000,  // Stricter than OpenFang
                timeout_secs: Some(10),
                capabilities,
            };
            // ... similar to OpenFang
        }).await?
    }
}
```

---

## 14. Conclusion

OpenFang provides an excellent reference implementation for Wakey. Key takeaways:

1. **Strict workspace hierarchy** prevents circular dependencies
2. **WASM sandbox with fuel + epoch** is the right approach for skills
3. **Capability-based security** with inheritance checks prevents escalation
4. **Taint tracking** is essential for prompt injection defense
5. **Event bus** is cleaner than trait-based inter-crate communication
6. **Approval system** enables safe autonomous operation
7. **Tauri 2.0 pattern** works well for desktop, but consider native UI for overlay

The main adaptation needed is Wakey's unique "consciousness" layer (tick/breath/reflect/dream) which OpenFang doesn't have. This should be a new crate that sits above the kernel level.

---

*End of Research Report*