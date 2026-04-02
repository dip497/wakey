# Wakey Coding Standards

These are enforced by tooling and reviewed by the CTO (Claude). GSD workers MUST follow these.

## 1. Error Handling

```rust
// GOOD — library crate, specific error type
use wakey_types::{WakeyError, WakeyResult};

pub fn do_thing() -> WakeyResult<Something> {
    let data = read_file().map_err(|e| WakeyError::Sense {
        sensor: "filesystem".into(),
        message: e.to_string(),
    })?;
    Ok(process(data))
}

// GOOD — binary crate only (wakey-app)
use anyhow::Result;
fn main() -> Result<()> { ... }

// BAD — never in library crates
fn bad() { some_option.unwrap(); }  // NO
fn bad() -> anyhow::Result<()> { }  // NO (except wakey-app)
```

## 2. Async Patterns

```rust
// GOOD — cancellable task
async fn run(spine: Spine, mut shutdown: broadcast::Receiver<WakeyEvent>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(interval) => {
                spine.emit(WakeyEvent::Tick);
            }
            Ok(WakeyEvent::Shutdown) = shutdown.recv() => {
                tracing::info!("Shutting down");
                break;
            }
        }
    }
}

// BAD — uncancellable, will leak on shutdown
async fn bad(spine: Spine) {
    loop {
        tokio::time::sleep(interval).await;
        spine.emit(WakeyEvent::Tick);
    }
}
```

## 3. Trait Design (ZeroClaw Pattern)

```rust
// GOOD — async trait with Send + Sync for Arc<dyn>
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages: &[Message]) -> WakeyResult<String>;
    async fn chat_stream(&self, messages: &[Message]) -> WakeyResult<StreamReceiver>;
    fn name(&self) -> &str;
}

// Register at runtime
fn create_provider(config: &LlmProviderConfig) -> Arc<dyn LlmProvider> {
    Arc::new(OpenAiCompatible::new(config))
}
```

## 4. Event Communication

```rust
// GOOD — communicate through spine
impl MySensor {
    async fn run(&self, spine: &Spine) {
        let data = self.sense().await;
        spine.emit(WakeyEvent::WindowFocusChanged {
            app: data.app,
            title: data.title,
            timestamp: Utc::now(),
        });
    }
}

// BAD — direct cross-crate function call
use wakey_cortex::internal_function;  // NEVER DO THIS
```

## 5. No Duplicate Code

If you need a utility in two crates, it goes in `wakey-types`:
- Common string helpers → `wakey_types::util`
- Shared constants → `wakey_types::constants`
- Platform detection → `wakey_types::platform`

If it's domain-specific, create a module in the owning crate and re-export through its public API.

**Before writing any utility function**, search the workspace:
```bash
cargo clippy --workspace  # will catch unused imports
grep -r "fn function_name" crates/
```

## 6. Configuration

```rust
// GOOD — config drives behavior
pub fn new(config: &HeartbeatConfig) -> Self {
    Self {
        tick_interval: Duration::from_millis(config.tick_interval_ms),
    }
}

// BAD — hardcoded values
pub fn new() -> Self {
    Self {
        tick_interval: Duration::from_secs(2),  // magic number
    }
}
```

## 7. Logging

```rust
// GOOD — structured, appropriate levels
tracing::debug!(app = %app_name, "Window focus changed");
tracing::info!("Heartbeat started");
tracing::warn!(error = %e, "Failed to capture screenshot, skipping");
tracing::error!(error = %e, "LLM provider unreachable");

// BAD
println!("something happened");  // NEVER
log::info!("use tracing instead");  // WRONG CRATE
```

## 8. Dependencies

```toml
# GOOD — minimal features
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }

# BAD — pulling in everything
reqwest = "0.12"  # includes openssl, cookies, etc.
```

## 9. Pre-Commit Checklist

Every commit must pass:
```bash
cargo fmt --all -- --check        # formatting
cargo clippy --workspace -- -D warnings  # linting
cargo check --workspace           # compilation
cargo test --workspace            # tests (when they exist)
```

## 10. File Size Limits

- No source file over 300 lines. If it's longer, split it.
- No function over 50 lines. If it's longer, extract helpers.
- No more than 5 parameters per function. Use a config/options struct.
