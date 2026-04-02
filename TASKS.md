# Wakey — Phase 1 Implementation Tasks

## Task 1: Config Loader
**Crate**: `wakey-types` (extend) + `wakey-app`
**Goal**: Load `config/default.toml` into `WakeyConfig` struct at startup.
**Details**:
- Add `impl WakeyConfig { pub fn load(path: &Path) -> WakeyResult<Self> }` to wakey-types/src/config.rs
- Use the `toml` crate for deserialization
- Fall back to `Default::default()` if no config file found
- Expand `~` to home directory in paths
- Load in wakey-app/src/main.rs at startup

## Task 2: Heartbeat Tick
**Crate**: `wakey-heartbeat`
**Goal**: Emit `WakeyEvent::Tick` every 2 seconds with active window info.
**Details**:
- Create a `HeartbeatRunner` struct that takes a `Spine` and `HeartbeatConfig`
- Spawn a tokio task that loops: sleep(tick_interval) → get active window → emit Tick event
- For active window on Linux, use `xcb` or shell out to `xdotool getactivewindow getwindowname`
- Also emit `WakeyEvent::SystemVitals` with basic CPU/RAM from `/proc/stat` and `/proc/meminfo`
- Must be cancellable via `WakeyEvent::Shutdown`

## Task 3: OpenAI-Compatible LLM Client
**Crate**: `wakey-cortex`
**Goal**: Minimal HTTP client that talks to any OpenAI-compatible API.
**Details**:
- Use `reqwest` with minimal features (rustls-tls, json)
- Implement `POST /chat/completions` only — that's all we need
- Support streaming (SSE) for real-time responses
- Support vision (image_url in messages) for VLM screenshots
- Read provider config from `WakeyConfig.llm`
- Trait: `trait LlmProvider { async fn chat(&self, messages: Vec<Message>) -> WakeyResult<String>; }`
- Single implementation: `OpenAiCompatibleProvider` — works with Ollama, OpenRouter, GLM, vLLM, anything

## Task 4: Overlay Window
**Crate**: `wakey-overlay`
**Goal**: Always-on-top transparent window with a simple animated sprite.
**Details**:
- Use `iced` (or `egui` with `eframe`) for the window
- Window must be: transparent background, always-on-top, click-through except on the sprite, no taskbar icon
- Start with a simple colored circle that "breathes" (scales up/down slowly) as the heartbeat glow
- Position: bottom-right of screen, draggable
- Show a text bubble when Wakey speaks (from ShouldSpeak events)
- Subscribe to spine events for state updates
- On Linux: use X11 hints for always-on-top and click-through

## Task 5: Wire It All Together
**Crate**: `wakey-app`
**Goal**: Main binary that initializes all systems and runs the event loop.
**Details**:
- Load config (Task 1)
- Create Spine
- Start HeartbeatRunner (Task 2)
- Start LLM client (Task 3)
- Start Overlay (Task 4)
- Simple cortex loop: listen for Tick events, every Nth tick, ask LLM "What should I say?" with context of active window
- Ctrl+C → emit Shutdown → graceful cleanup

## Dependencies to Add
```toml
# In workspace Cargo.toml [workspace.dependencies]
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }

# For overlay (choose one)
iced = { version = "0.13", features = ["tokio"] }
# OR
eframe = "0.31"
egui = "0.31"
```

## Performance Targets
- Idle RAM: <20MB (measure with `ps aux | grep wakey`)
- Tick latency: <10ms
- LLM call: async, non-blocking, no UI freeze
- Overlay render: <16ms per frame (60fps)
