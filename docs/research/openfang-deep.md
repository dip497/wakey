# OpenFang Deep Research

> **Source**: Actual Rust source code from `/home/dipendra-sharma/projects/openfang/`
> **Version**: 0.5.5
> **Date**: 2026-04-03

## Overview

OpenFang is an open-source Agent Operating System written in Rust. It provides:
- Multi-channel agent orchestration (30+ integrations)
- WASM-based skill sandboxing with capability security
- Event-driven inter-agent communication
- Tauri 2.0 desktop application

---

## 1. Multi-Crate Workspace

### Workspace Structure

**File**: `Cargo.toml`

```toml
[workspace]
resolver = "2"
members = [
    "crates/openfang-types",      # Foundation types, events, config
    "crates/openfang-memory",     # SQLite memory substrate
    "crates/openfang-runtime",    # LLM drivers, tool execution, WASM sandbox
    "crates/openfang-wire",       # Agent-to-agent networking (OFP)
    "crates/openfang-api",        # HTTP/WebSocket API server
    "crates/openfang-kernel",     # Core kernel — assembles all subsystems
    "crates/openfang-cli",        # CLI tool
    "crates/openfang-channels",   # 30+ channel integrations
    "crates/openfang-migrate",    # Import from other agent frameworks
    "crates/openfang-skills",     # Skill registry, loader, marketplace
    "crates/openfang-desktop",    # Tauri 2.0 desktop app
    "crates/openfang-hands",      # Curated autonomous capability packages
    "crates/openfang-extensions", # MCP integration, credential vault
    "xtask",                      # Build automation
]
```

### Dependency Graph

```
                    openfang-types
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
    openfang-memory  openfang-wire  openfang-skills
          │               │               │
          └───────┬───────┘               │
                  ▼                       │
           openfang-runtime ◄─────────────┘
                  │
          ┌───────┴───────┐
          ▼               ▼
    openfang-kernel  openfang-hands
          │               │
          └───────┬───────┘
                  ▼
           openfang-api ◄── openfang-channels
                  │              openfang-extensions
          ┌───────┴───────┐
          ▼               ▼
    openfang-cli   openfang-desktop
```

### Key Cargo.toml Insights

```toml
[workspace.dependencies]
# WASM sandbox
wasmtime = "41"

# HTTP server
axum = { version = "0.8", features = ["ws", "multipart"] }

# Async runtime
tokio = { version = "1", features = ["full"] }

# Database
rusqlite = { version = "0.31", features = ["bundled", "serde_json"] }

# Security
ed25519-dalek = { version = "2", features = ["rand_core"] }
sha2 = "0.10"
zeroize = { version = "1", features = ["derive"] }
```

---

## 2. WASM Skill Sandbox

### Runtime: Wasmtime (v41)

**File**: `crates/openfang-runtime/src/sandbox.rs`

```rust
//! WASM sandbox for secure skill/plugin execution.
//!
//! Uses Wasmtime to execute untrusted WASM modules with deny-by-default
//! capability-based permissions. No filesystem, network, or credential
//! access unless explicitly granted.

/// Configuration for a WASM sandbox instance.
#[derive(Debug, Clone)]
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
            fuel_limit: 1_000_000,        // 1M instructions
            max_memory_bytes: 16 * 1024 * 1024, // 16MB
            capabilities: Vec::new(),
            timeout_secs: None,
        }
    }
}
```

### Fuel Metering + Epoch Interruption

```rust
pub struct WasmSandbox {
    engine: Engine,
}

impl WasmSandbox {
    pub fn new() -> Result<Self, SandboxError> {
        let mut config = Config::new();
        config.consume_fuel(true);      // Deterministic CPU metering
        config.epoch_interruption(true); // Wall-clock timeout
        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    fn execute_sync(...) -> Result<ExecutionResult, SandboxError> {
        // Set fuel budget (deterministic metering)
        if config.fuel_limit > 0 {
            store.set_fuel(config.fuel_limit)?;
        }

        // Set epoch deadline (wall-clock metering)
        store.set_epoch_deadline(1);
        let engine_clone = engine.clone();
        let timeout = config.timeout_secs.unwrap_or(30);
        let _watchdog = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(timeout));
            engine_clone.increment_epoch();
        });
        // ... execution ...
    }
}
```

### Guest ABI (WASM Module Contract)

```rust
//! # Guest ABI
//!
//! WASM modules must export:
//! - `memory` — linear memory
//! - `alloc(size: i32) -> i32` — allocate `size` bytes, return pointer
//! - `execute(input_ptr: i32, input_len: i32) -> i64` — main entry point
//!
//! The `execute` function receives JSON input bytes and returns a packed
//! `i64` value: `(result_ptr << 32) | result_len`. The result is JSON bytes.
```

### Host ABI (Functions Provided to WASM)

```rust
/// Register host function imports in the linker ("openfang" module).
fn register_host_functions(linker: &mut Linker<GuestState>) -> Result<(), SandboxError> {
    // host_call: single dispatch for all capability-checked operations.
    linker.func_wrap(
        "openfang",
        "host_call",
        |mut caller: Caller<'_, GuestState>,
         request_ptr: i32,
         request_len: i32|
         -> Result<i64, anyhow::Error> {
            // Read request from guest memory
            // Parse: {"method": "...", "params": {...}}
            // Dispatch to capability-checked handler
            let response = host_functions::dispatch(caller.data(), &method, &params);
            // Return packed (ptr, len) to guest
        },
    )?;

    // host_log: lightweight logging — no capability check required.
    linker.func_wrap(
        "openfang",
        "host_log",
        |mut caller: Caller<'_, GuestState>,
         level: i32,
         msg_ptr: i32,
         msg_len: i32|
         -> Result<(), anyhow::Error> {
            // Log to tracing
        },
    )?;
}
```

---

## 3. Inter-Crate Communication

### Event Bus Architecture

**File**: `crates/openfang-kernel/src/event_bus.rs`

```rust
//! Event bus — pub/sub with pattern matching and history ring buffer.

/// Maximum events retained in the history ring buffer.
const HISTORY_SIZE: usize = 1000;

/// The central event bus for inter-agent and system communication.
pub struct EventBus {
    /// Broadcast channel for all events.
    sender: broadcast::Sender<Event>,
    /// Per-agent event channels.
    agent_channels: DashMap<AgentId, broadcast::Sender<Event>>,
    /// Event history ring buffer.
    history: Arc<RwLock<VecDeque<Event>>>,
}

impl EventBus {
    /// Publish an event to the bus.
    pub async fn publish(&self, event: Event) {
        // Store in history
        { /* ring buffer logic */ }

        // Route to target
        match &event.target {
            EventTarget::Agent(agent_id) => {
                if let Some(sender) = self.agent_channels.get(agent_id) {
                    let _ = sender.send(event.clone());
                }
            }
            EventTarget::Broadcast => {
                let _ = self.sender.send(event.clone());
                for entry in self.agent_channels.iter() {
                    let _ = entry.value().send(event.clone());
                }
            }
            EventTarget::Pattern(_pattern) => {
                // Phase 1: broadcast to all for pattern matching
                let _ = self.sender.send(event.clone());
            }
            EventTarget::System => {
                let _ = self.sender.send(event.clone());
            }
        }
    }
}
```

### Event Types

**File**: `crates/openfang-types/src/event.rs`

```rust
/// Where an event is directed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum EventTarget {
    Agent(AgentId),
    Broadcast,
    Pattern(String),
    System,
}

/// The payload of an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EventPayload {
    Message(AgentMessage),
    ToolResult(ToolOutput),
    MemoryUpdate(MemoryDelta),
    Lifecycle(LifecycleEvent),
    Network(NetworkEvent),
    System(SystemEvent),
    Custom(Vec<u8>),
}

/// Agent lifecycle event.
pub enum LifecycleEvent {
    Spawned { agent_id: AgentId, name: String },
    Started { agent_id: AgentId },
    Suspended { agent_id: AgentId },
    Resumed { agent_id: AgentId },
    Terminated { agent_id: AgentId, reason: String },
    Crashed { agent_id: AgentId, error: String },
}
```

### Kernel Assembly

**File**: `crates/openfang-kernel/src/kernel.rs`

```rust
pub struct OpenFangKernel {
    /// Kernel configuration.
    pub config: KernelConfig,
    /// Agent registry.
    pub registry: AgentRegistry,
    /// Capability manager.
    pub capabilities: CapabilityManager,
    /// Event bus.
    pub event_bus: EventBus,
    /// Agent scheduler.
    pub scheduler: AgentScheduler,
    /// Memory substrate.
    pub memory: Arc<MemorySubstrate>,
    /// Process supervisor.
    pub supervisor: Supervisor,
    /// Workflow engine.
    pub workflows: WorkflowEngine,
    /// Event-driven trigger engine.
    pub triggers: TriggerEngine,
    /// Background agent executor.
    pub background: BackgroundExecutor,
    /// Merkle hash chain audit trail.
    pub audit_log: Arc<AuditLog>,
    /// Cost metering engine.
    pub metering: Arc<MeteringEngine>,
    /// Default LLM driver.
    default_driver: Arc<dyn LlmDriver>,
    /// WASM sandbox engine (shared across all WASM agent executions).
    wasm_sandbox: WasmSandbox,
    /// RBAC authentication manager.
    pub auth: AuthManager,
    /// Skill registry for plugin skills.
    pub skill_registry: std::sync::RwLock<SkillRegistry>,
    /// MCP server connections.
    pub mcp_connections: tokio::sync::Mutex<Vec<McpConnection>>,
    /// ... many more subsystems
}
```

---

## 4. Security Layers

### Layer 1: Capability-Based Security

**File**: `crates/openfang-types/src/capability.rs`

```rust
/// A specific permission granted to an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Capability {
    // File system
    FileRead(String),   // Glob pattern
    FileWrite(String),

    // Network
    NetConnect(String), // Host pattern
    NetListen(u16),

    // Tools
    ToolInvoke(String),
    ToolAll,

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

    // OFP (networking)
    OfpDiscover,
    OfpConnect(String),
    OfpAdvertise,

    // Economic
    EconSpend(f64),
    EconEarn,
    EconTransfer(String),
}

/// Checks whether a required capability matches any granted capability.
pub fn capability_matches(granted: &Capability, required: &Capability) -> bool {
    match (granted, required) {
        (Capability::ToolAll, Capability::ToolInvoke(_)) => true,
        (Capability::FileRead(pattern), Capability::FileRead(path)) => glob_matches(pattern, path),
        (Capability::NetConnect(pattern), Capability::NetConnect(host)) => glob_matches(pattern, host),
        // ... etc
    }
}

/// Validate that child capabilities are a subset of parent capabilities.
pub fn validate_capability_inheritance(
    parent_caps: &[Capability],
    child_caps: &[Capability],
) -> Result<(), String> {
    for child_cap in child_caps {
        let is_covered = parent_caps.iter().any(|p| capability_matches(p, child_cap));
        if !is_covered {
            return Err(format!(
                "Privilege escalation denied: child requests {:?} but parent does not have a matching grant",
                child_cap
            ));
        }
    }
    Ok(())
}
```

### Layer 2: Taint Tracking

**File**: `crates/openfang-types/src/taint.rs`

```rust
//! Information flow taint tracking for agent data.
//!
//! Implements a lattice-based taint propagation model that prevents tainted
//! values from flowing into sensitive sinks without explicit declassification.

/// A classification label applied to data flowing through the system.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaintLabel {
    ExternalNetwork,
    UserInput,
    Pii,
    Secret,
    UntrustedAgent,
}

/// A value annotated with taint labels tracking its provenance.
pub struct TaintedValue {
    pub value: String,
    pub labels: HashSet<TaintLabel>,
    pub source: String,
}

/// A destination that restricts which taint labels may flow into it.
pub struct TaintSink {
    pub name: String,
    pub blocked_labels: HashSet<TaintLabel>,
}

impl TaintSink {
    /// Blocks external network data and untrusted agent data.
    pub fn shell_exec() -> Self {
        let mut blocked = HashSet::new();
        blocked.insert(TaintLabel::ExternalNetwork);
        blocked.insert(TaintLabel::UntrustedAgent);
        blocked.insert(TaintLabel::UserInput);
        Self { name: "shell_exec".into(), blocked_labels: blocked }
    }

    /// Blocks secrets and PII to prevent data exfiltration.
    pub fn net_fetch() -> Self {
        let mut blocked = HashSet::new();
        blocked.insert(TaintLabel::Secret);
        blocked.insert(TaintLabel::Pii);
        Self { name: "net_fetch".into(), blocked_labels: blocked }
    }
}
```

### Layer 3: Tool Policy

**File**: `crates/openfang-runtime/src/tool_policy.rs`

```rust
/// Effect of a policy rule.
pub enum PolicyEffect {
    Allow,
    Deny,
}

/// A single tool policy rule with glob pattern support.
pub struct ToolPolicyRule {
    pub pattern: String,      // e.g., "shell_*", "web_*"
    pub effect: PolicyEffect,
}

/// Complete tool policy configuration.
pub struct ToolPolicy {
    pub agent_rules: Vec<ToolPolicyRule>,  // Highest priority
    pub global_rules: Vec<ToolPolicyRule>, // Checked second
    pub groups: Vec<ToolGroup>,            // Named collections
    pub subagent_max_depth: u32,           // Default: 10
    pub subagent_max_concurrent: u32,      // Default: 5
}

/// Resolve whether a tool is accessible — deny-wins.
pub fn resolve_tool_access(tool_name: &str, policy: &ToolPolicy, depth: u32) -> ToolAccessResult {
    // Check depth limit for subagent-related tools
    if is_subagent_tool(tool_name) && depth > policy.subagent_max_depth {
        return ToolAccessResult::DepthExceeded { current: depth, max: policy.subagent_max_depth };
    }

    // Phase 1: Check agent deny rules (highest priority)
    // Phase 2: Check global deny rules
    // Phase 3: If any allow rules exist, tool must match at least one
}
```

### Layer 4: Host Function Security

**File**: `crates/openfang-runtime/src/host_functions.rs`

```rust
/// Dispatch a host call to the appropriate handler.
pub fn dispatch(state: &GuestState, method: &str, params: &serde_json::Value) -> serde_json::Value {
    match method {
        "time_now" => host_time_now(),  // Always allowed

        // Filesystem — requires FileRead/FileWrite
        "fs_read" => host_fs_read(state, params),
        "fs_write" => host_fs_write(state, params),

        // Network — requires NetConnect
        "net_fetch" => host_net_fetch(state, params),

        // Shell — requires ShellExec
        "shell_exec" => host_shell_exec(state, params),
        // ...
    }
}

/// SSRF protection: check if a hostname resolves to a private/internal IP.
fn is_ssrf_target(url: &str) -> Result<(), serde_json::Value> {
    // Only allow http:// and https://
    // Block: localhost, metadata.google.internal, 169.254.169.254
    // Resolve DNS and check every returned IP against private ranges
}

/// Secure path resolution — rejects ".." components
fn safe_resolve_path(path: &str) -> Result<PathBuf, serde_json::Value> {
    for component in Path::new(path).components() {
        if matches!(component, Component::ParentDir) {
            return Err(json!({"error": "Path traversal denied"}));
        }
    }
    std::fs::canonicalize(path).map_err(|e| json!({"error": format!("Cannot resolve: {e}")}))
}
```

### Layer 5: Skill Verification

**File**: `crates/openfang-skills/src/verify.rs`

```rust
/// Scan a skill manifest for potentially dangerous capabilities.
pub fn security_scan(manifest: &SkillManifest) -> Vec<SkillWarning> {
    // Check for dangerous runtime types (Node.js)
    // Check for dangerous capabilities (ShellExec, NetConnect(*))
    // Check for dangerous tools (shell_exec, file_delete)
    // Flag if > 10 tools required
}

/// Scan prompt content for injection attacks.
pub fn scan_prompt_content(content: &str) -> Vec<SkillWarning> {
    // Critical: prompt override attempts
    //   - "ignore previous instructions"
    //   - "you are now"
    //   - "system prompt override"

    // Warning: data exfiltration patterns
    //   - "send to http"
    //   - "exfiltrate"

    // Warning: shell command references
    //   - "rm -rf", "sudo "
}
```

---

## 5. Desktop App (Tauri 2.0)

### Main Entry Point

**File**: `crates/openfang-desktop/src/lib.rs`

```rust
//! OpenFang Desktop — Native Tauri 2.0 wrapper.
//!
//! Boots the kernel + embedded API server, then opens a native window.

pub fn run() {
    // Boot kernel + embedded server
    let server_handle = server::start_server().expect("Failed to start OpenFang server");
    let port = server_handle.port;
    let url = format!("http://127.0.0.1:{port}");

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window when second instance tries to launch
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().args(["--minimized"]).build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(KernelState { kernel: server_handle.kernel.clone(), started_at: Instant::now() });

    builder
        .setup(move |app| {
            // Create main window pointing at embedded HTTP server
            let _window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url.parse().expect("Invalid URL")),
            )
            .title("OpenFang")
            .inner_size(1280.0, 800.0)
            .min_inner_size(800.0, 600.0)
            .center()
            .visible(true)
            .build()?;

            // Set up system tray
            tray::setup_tray(app)?;

            // Forward critical kernel events as native notifications
            let app_handle = app.handle().clone();
            let mut event_rx = kernel_for_notifications.event_bus.subscribe_all();
            tauri::async_runtime::spawn(async move {
                loop {
                    match event_rx.recv().await {
                        Ok(event) => {
                            // Only: Crashed, KernelStopping, QuotaEnforced
                            // Skip: health checks, spawns, suspends
                        }
                        // ...
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide to tray on close instead of quitting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        });
}
```

### Tauri Configuration

**File**: `crates/openfang-desktop/tauri.conf.json`

```json
{
  "productName": "OpenFang",
  "version": "0.5.5",
  "identifier": "ai.openfang.desktop",
  "app": {
    "security": {
      "csp": "default-src 'self' http://127.0.0.1:* ws://127.0.0.1:* ...; object-src 'none'"
    }
  },
  "plugins": {
    "updater": {
      "pubkey": "...",
      "endpoints": ["https://github.com/.../latest.json"]
    }
  },
  "bundle": {
    "category": "Productivity",
    "shortDescription": "Open-source Agent Operating System"
  }
}
```

---

## 6. Skill System + FangHub

### Skill Manifest Structure

**File**: `crates/openfang-skills/src/lib.rs`

```rust
/// The runtime type for a skill.
pub enum SkillRuntime {
    Python,     // Subprocess
    Wasm,       // Sandbox
    Node,       // OpenClaw compatibility
    Shell,      // Subprocess
    Builtin,    // Compiled into binary
    PromptOnly, // Default — injects context into LLM system prompt
}

/// A skill manifest (parsed from skill.toml).
pub struct SkillManifest {
    pub skill: SkillMeta,
    pub runtime: SkillRuntimeConfig,
    pub tools: SkillTools,
    pub requirements: SkillRequirements,
    pub prompt_context: Option<String>,
    pub source: Option<SkillSource>,
}

pub struct SkillRequirements {
    pub tools: Vec<String>,        // Built-in tools needed
    pub capabilities: Vec<String>, // Host capabilities needed
}
```

### Skill Registry

**File**: `crates/openfang-skills/src/registry.rs`

```rust
pub struct SkillRegistry {
    skills: HashMap<String, InstalledSkill>,
    skills_dir: PathBuf,
    frozen: bool,  // Stable mode — no new skills
    blocked_skills_count: usize,  // Skills blocked for injection
}

impl SkillRegistry {
    /// Load all bundled skills (compile-time embedded).
    pub fn load_bundled(&mut self) -> usize { /* ... */ }

    /// Load all installed skills from directory.
    pub fn load_all(&mut self) -> Result<usize, SkillError> {
        // Auto-detect SKILL.md and convert to skill.toml
        // Run prompt injection scan
        // Block critical threats
    }

    /// Load workspace-scoped skills that override global.
    pub fn load_workspace_skills(&mut self, dir: &Path) -> Result<usize, SkillError>;

    /// Get all tool definitions from enabled skills.
    pub fn all_tool_definitions(&self) -> Vec<SkillToolDef>;
}
```

### FangHub Marketplace

**File**: `crates/openfang-skills/src/marketplace.rs`

```rust
/// FangHub registry configuration.
pub struct MarketplaceConfig {
    pub registry_url: String,    // "https://api.github.com"
    pub github_org: String,      // "openfang-skills"
}

pub struct MarketplaceClient {
    config: MarketplaceConfig,
    http: reqwest::Client,
}

impl MarketplaceClient {
    /// Search for skills by query.
    pub async fn search(&self, query: &str) -> Result<Vec<SkillSearchResult>, SkillError>;

    /// Install a skill from GitHub repo.
    pub async fn install(&self, skill_name: &str, target_dir: &Path) -> Result<String, SkillError>;
}
```

---

## 7. LLM Driver Abstraction

**File**: `crates/openfang-runtime/src/llm_driver.rs`

```rust
/// Trait for LLM drivers.
#[async_trait]
pub trait LlmDriver: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    async fn stream(
        &self,
        request: CompletionRequest,
        tx: tokio::sync::mpsc::Sender<StreamEvent>,
    ) -> Result<CompletionResponse, LlmError>;
}

pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u32,
    pub temperature: f32,
    pub system: Option<String>,
    pub thinking: Option<ThinkingConfig>,
}

pub enum StreamEvent {
    TextDelta { text: String },
    ToolUseStart { id: String, name: String },
    ToolInputDelta { text: String },
    ToolUseEnd { id: String, name: String, input: serde_json::Value },
    ThinkingDelta { text: String },
    ContentComplete { stop_reason: StopReason, usage: TokenUsage },
    PhaseChange { phase: String, detail: Option<String> },
}

/// Driver configuration.
pub struct DriverConfig {
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub skip_permissions: bool,
}
```

### Fallback Driver Chain

```rust
// Kernel boots with fallback chain
let driver: Arc<dyn LlmDriver> = if driver_chain.len() > 1 {
    Arc::new(FallbackDriver::with_models(model_chain))
} else if let Some(single) = driver_chain.into_iter().next() {
    single
} else {
    Arc::new(StubDriver)  // Returns helpful error, dashboard still boots
};
```

---

## Key Patterns for Wakey

### 1. Event Spine Pattern
OpenFang uses a central `EventBus` with `tokio::sync::broadcast` channels. Wakey should adopt this for the `wakey-spine` crate.

### 2. Capability Security
- Capabilities are immutable after agent creation
- Child agents cannot exceed parent capabilities
- Glob patterns for flexible matching

### 3. WASM Sandbox
- Wasmtime with fuel metering (deterministic CPU budget)
- Epoch interruption (wall-clock timeout)
- Explicit Guest ABI (memory, alloc, execute)
- Explicit Host ABI (host_call, host_log) with capability checks

### 4. Multi-Layer Security
1. Capability-based permissions
2. Taint tracking (information flow)
3. Tool policy (deny-wins, depth limits)
4. Host function security (SSRF, path traversal)
5. Skill verification (prompt injection scanning)

### 5. LLM Driver Trait
Simple trait with `complete` and `stream` methods. OpenAI-compatible HTTP only.

### 6. Desktop Integration
- Tauri 2.0 with embedded HTTP server
- System tray for background operation
- Native notifications for critical events
- Single-instance enforcement

---

## File Reference

| Area | Key Files |
|------|-----------|
| Workspace | `Cargo.toml` |
| WASM Sandbox | `crates/openfang-runtime/src/sandbox.rs`, `host_functions.rs` |
| Event Bus | `crates/openfang-kernel/src/event_bus.rs` |
| Kernel | `crates/openfang-kernel/src/kernel.rs` |
| Capabilities | `crates/openfang-types/src/capability.rs` |
| Taint | `crates/openfang-types/src/taint.rs` |
| Tool Policy | `crates/openfang-runtime/src/tool_policy.rs` |
| Skills | `crates/openfang-skills/src/lib.rs`, `registry.rs`, `verify.rs` |
| Desktop | `crates/openfang-desktop/src/lib.rs`, `tauri.conf.json` |
| LLM Driver | `crates/openfang-runtime/src/llm_driver.rs` |
| Events | `crates/openfang-types/src/event.rs` |