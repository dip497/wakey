# P1-S5: Wire It All — First Conversation

## Goal
Wakey starts up, shows overlay, detects active window, talks to LLM, says something in chat bubble.

## Crate
wakey-app (src/main.rs)

## What to implement

### Startup sequence
```rust
#[tokio::main]
async fn main() -> Result<()> {
    // 1. Init tracing
    // 2. Load config
    let config = WakeyConfig::load(Path::new("config/default.toml"))?;
    
    // 3. Create spine
    let spine = Spine::new();
    
    // 4. Create LLM provider from config
    let provider = OpenAiCompatible::from_config(&config.llm)?;
    
    // 5. Start heartbeat (background task)
    let heartbeat = HeartbeatRunner::new(spine.clone(), config.heartbeat.clone());
    tokio::spawn(heartbeat.run(spine.subscribe()));
    
    // 6. Start decision loop (background task)
    //    Listens for Tick events, every 5th tick asks LLM what to say
    tokio::spawn(decision_loop(spine.clone(), provider));
    
    // 7. Start overlay (main thread — egui needs main thread)
    //    Subscribes to ShouldSpeak events
    run_overlay(spine.clone(), config.persona.clone())?;
    
    Ok(())
}
```

### Decision loop (simple MVP version)
```rust
async fn decision_loop(spine: Spine, provider: Arc<dyn LlmProvider>) {
    let mut rx = spine.subscribe();
    let mut tick_count = 0;
    let mut last_window = String::new();
    
    while let Ok(event) = rx.recv().await {
        match event {
            WakeyEvent::WindowFocusChanged { app, title, .. } => {
                last_window = format!("{} - {}", app, title);
                tick_count += 1;
                
                // Every 15th tick (~30s), ask LLM
                if tick_count % 15 == 0 {
                    let prompt = format!(
                        "You are Wakey, a friendly AI companion. The user is currently in: {}. 
                         Say something brief, helpful, or encouraging (1-2 sentences max). 
                         Be casual and friendly, not robotic.",
                        last_window
                    );
                    if let Ok(response) = provider.chat(&[...]).await {
                        spine.emit(WakeyEvent::ShouldSpeak { ... });
                    }
                }
            }
            WakeyEvent::Shutdown => break,
            _ => {}
        }
    }
}
```

## Dependencies
- This slice depends on S1 (config), S2 (heartbeat), S3 (LLM), S4 (overlay)
- Run AFTER the other 4 slices are done

## Read first
- All other P1 specs
- crates/wakey-app/src/main.rs (existing skeleton)

## Verify
```bash
cargo run --package wakey-app
# Expected: overlay appears, after ~30s Wakey says something about active window
```

## Acceptance criteria
- `wakey` binary starts and shows overlay within 1 second
- Heartbeat tick events logged every 2s
- After ~30s, Wakey speaks about the active window
- Chat bubble shows the text with typewriter effect
- Ctrl+C graceful shutdown
- Idle RAM < 30MB (MVP target, optimize later)
