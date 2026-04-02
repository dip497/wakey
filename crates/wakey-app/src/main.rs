use anyhow::Result;
use tracing::info;
use wakey_spine::Spine;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().init();

    info!("Wakey is waking up...");

    // Create the central nervous system
    let spine = Spine::new();

    info!(
        subscribers = spine.subscriber_count(),
        "Spine initialized"
    );

    // TODO: Initialize all subsystems and connect to spine
    // 1. Load config
    // 2. Start senses (a11y, clipboard, filesystem watchers)
    // 3. Start heartbeat (tick, breath, reflect, dream)
    // 4. Start memory (OpenViking backend)
    // 5. Start cortex (decision engine + LLM)
    // 6. Start persona (mood, style)
    // 7. Start action (input, terminal, safety)
    // 8. Start overlay (window, sprites, bubbles)
    // 9. Start voice (TTS/STT)
    // 10. Start skills (registry, WASM runtime)
    // 11. Start learning (skill extraction loop)

    info!("Wakey is alive.");

    // Keep running until shutdown
    tokio::signal::ctrl_c().await?;
    spine.emit(wakey_types::WakeyEvent::Shutdown);

    info!("Wakey is going to sleep. Goodnight.");
    Ok(())
}
