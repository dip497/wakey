use anyhow::Result;
use std::time::Duration;
use tracing::{info, warn};
use wakey_cortex::heartbeat::HeartbeatRunner;
use wakey_overlay::run_overlay_with_spine;
use wakey_spine::Spine;
use wakey_types::WakeyEvent;
use wakey_types::config::HeartbeatConfig;

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("Wakey is waking up...");

    // Create the central nervous system
    let spine = Spine::new();

    info!(subscribers = spine.subscriber_count(), "Spine initialized");

    // Create default heartbeat config (2s tick)
    let heartbeat_config = HeartbeatConfig {
        tick_interval_ms: 2000,
        breath_interval_ms: 30_000,
        reflect_interval_ms: 900_000,
        dream_hour: 4,
    };

    // Start heartbeat runner and event logger in a background thread with tokio runtime
    let spine_clone = spine.clone();
    let heartbeat_config_clone = heartbeat_config.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            // Start heartbeat runner
            let heartbeat_runner =
                HeartbeatRunner::new(spine_clone.clone(), &heartbeat_config_clone);
            let shutdown_rx = spine_clone.subscribe();
            tokio::spawn(heartbeat_runner.run(shutdown_rx));

            // Start event logger
            let event_logger = EventLogger::new(spine_clone.subscribe());
            tokio::spawn(event_logger.run());

            // Wait for shutdown signal
            tokio::signal::ctrl_c().await.ok();
            info!("Shutdown signal received");
            spine_clone.emit(WakeyEvent::Shutdown);

            // Give subsystems time to clean up
            tokio::time::sleep(Duration::from_millis(500)).await;
        });
    });

    info!("Starting overlay window...");

    // Run overlay on main thread (blocks until window closes)
    run_overlay_with_spine(spine);

    info!("Wakey is going to sleep. Goodnight.");
    Ok(())
}

/// Logs all events flowing through the spine.
struct EventLogger {
    receiver: tokio::sync::broadcast::Receiver<WakeyEvent>,
}

impl EventLogger {
    fn new(receiver: tokio::sync::broadcast::Receiver<WakeyEvent>) -> Self {
        Self { receiver }
    }

    async fn run(mut self) {
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    match &event {
                        WakeyEvent::Tick => {
                            // Skip logging Tick events (too noisy)
                        }
                        WakeyEvent::WindowFocusChanged { app, title, .. } => {
                            info!(app = %app, title = %title, "Window focus changed");
                        }
                        WakeyEvent::SystemVitals {
                            battery_percent,
                            cpu_usage,
                            ram_usage_mb,
                            ..
                        } => {
                            info!(
                                battery = ?battery_percent,
                                cpu = %cpu_usage,
                                ram_mb = %ram_usage_mb,
                                "System vitals"
                            );
                        }
                        WakeyEvent::Shutdown => {
                            info!("Shutdown event received, stopping logger");
                            break;
                        }
                        other => {
                            info!(event = ?other, "Event received");
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    info!("Spine closed, stopping logger");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!(lagged = n, "Event logger lagged, continuing");
                }
            }
        }
    }
}
