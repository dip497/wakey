//! Wakey — Your laptop, alive.
//!
//! This is the main entry point that wires all subsystems together:
//! - Config loader (S1)
//! - Heartbeat with window detection (S2)
//! - LLM provider (S3)
//! - Overlay with sprite and chat bubble (S4)
//! - Decision loop that calls LLM periodically (S5)

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};
use wakey_cortex::heartbeat::HeartbeatRunner;
use wakey_cortex::llm::{LlmProvider, OpenAiCompatible};
use wakey_overlay::run_overlay_with_spine;
use wakey_spine::Spine;
use wakey_types::config::WakeyConfig;
use wakey_types::{ChatMessage, WakeyEvent};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Load .env file (API keys etc)
    dotenvy::dotenv().ok();

    info!("Wakey is waking up...");

    // Load configuration from file
    let config = WakeyConfig::load(Path::new("config/default.toml"))?;
    info!(
        persona = config.persona.name,
        provider = config.llm.default_provider,
        "Configuration loaded"
    );

    // Create the central nervous system
    let spine = Spine::new();
    info!(subscribers = spine.subscriber_count(), "Spine initialized");

    // Create LLM provider from config
    let provider = create_llm_provider(&config)?;

    // Start heartbeat runner and decision loop in a background thread with tokio runtime
    let spine_clone = spine.clone();
    let config_clone = config.clone();
    let provider_clone = provider.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            // Start heartbeat runner
            let heartbeat_runner =
                HeartbeatRunner::new(spine_clone.clone(), &config_clone.heartbeat);
            let heartbeat_shutdown_rx = spine_clone.subscribe();
            tokio::spawn(heartbeat_runner.run(heartbeat_shutdown_rx));

            // Start decision loop (calls LLM periodically)
            let decision_shutdown_rx = spine_clone.subscribe();
            tokio::spawn(decision_loop(
                spine_clone.clone(),
                provider_clone,
                config_clone.persona.name.clone(),
                decision_shutdown_rx,
            ));

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

/// Create LLM provider from configuration.
///
/// Finds the default provider in config and creates an OpenAI-compatible client.
/// Returns None if no providers are configured (Wakey will run without LLM).
fn create_llm_provider(config: &WakeyConfig) -> Result<Arc<dyn LlmProvider>> {
    // Find the default provider config
    let provider_config = config
        .llm
        .providers
        .iter()
        .find(|p| p.name == config.llm.default_provider);

    match provider_config {
        Some(cfg) => {
            let provider = OpenAiCompatible::new(cfg)?;
            info!(
                provider = cfg.name,
                model = cfg.model,
                base = cfg.api_base,
                "LLM provider created"
            );
            Ok(Arc::new(provider))
        }
        None => {
            let available: Vec<&str> = config
                .llm
                .providers
                .iter()
                .map(|p| p.name.as_str())
                .collect();
            warn!(
                default = config.llm.default_provider,
                available = available.join(", "),
                "Default LLM provider not found in config"
            );
            // Return a fallback or error — for MVP, we fail fast
            anyhow::bail!(
                "LLM provider '{}' not found in config. Available providers: {}",
                config.llm.default_provider,
                available.join(", ")
            );
        }
    }
}

/// Decision loop — listens for events and decides when to speak.
///
/// MVP version: Every 15 WindowFocusChanged events (~30s with 2s ticks),
/// asks the LLM to say something about the current window.
async fn decision_loop(
    spine: Spine,
    provider: Arc<dyn LlmProvider>,
    persona_name: String,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<WakeyEvent>,
) {
    let mut event_rx = spine.subscribe();
    let mut focus_count = 0u32;
    let mut last_window = String::new();

    info!("Decision loop started");

    loop {
        tokio::select! {
            // Handle events from spine
            Ok(event) = event_rx.recv() => {
                if let WakeyEvent::WindowFocusChanged { app, title, .. } = event {
                    let window = format!("{} - {}", app, title);
                    if window != last_window {
                        debug!(window = %window, "Window focus changed");
                        last_window = window.clone();
                        focus_count += 1;

                        // Every 15th focus change (~30s), ask LLM to say something
                        if focus_count.is_multiple_of(5) {
                            ask_llm_to_speak(&spine, &provider, &persona_name, &last_window);
                        }
                    }
                }
            }

            // Shutdown signal
            Ok(WakeyEvent::Shutdown) = shutdown_rx.recv() => {
                info!("Decision loop shutting down");
                break;
            }
        }
    }
}

/// Ask the LLM to generate something to say about the current context.
///
/// This is the MVP "proactive speech" logic. In future versions,
/// this will be much smarter (memory, user model, conversation history).
fn ask_llm_to_speak(
    spine: &Spine,
    provider: &Arc<dyn LlmProvider>,
    persona_name: &str,
    current_window: &str,
) {
    let prompt = format!(
        "You are {}, a friendly AI companion that lives on the user's desktop. \
         The user is currently looking at: {}. \
         Say something brief, helpful, or encouraging (1-2 sentences max). \
         Be casual and friendly, like a supportive friend. \
         Don't be robotic or overly formal. \
         If they're in a coding/working context, be encouraging. \
         If they're in entertainment, maybe joke about it. \
         Keep it short and natural.",
        persona_name, current_window
    );

    let messages = vec![
        ChatMessage::system(format!(
            "You are {}, a friendly desktop AI companion.",
            persona_name
        )),
        ChatMessage::user(prompt),
    ];

    info!(window = %current_window, "Asking LLM for something to say");

    // Spawn a task to handle the async LLM call
    let spine_clone = spine.clone();
    let provider_clone = provider.clone();
    let window_clone = current_window.to_string();
    tokio::spawn(async move {
        match provider_clone.chat(&messages).await {
            Ok(response) => {
                info!(response_len = response.len(), "LLM responded");

                // Emit ShouldSpeak event
                spine_clone.emit(WakeyEvent::ShouldSpeak {
                    reason: format!("Window context: {}", window_clone),
                    urgency: wakey_types::event::Urgency::Low,
                    suggested_text: Some(response),
                });
            }
            Err(e) => {
                warn!(error = ?e, "LLM call failed");
            }
        }
    });
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
                        WakeyEvent::ShouldSpeak { suggested_text, .. } => {
                            info!(text = ?suggested_text, "Should speak");
                        }
                        WakeyEvent::Shutdown => {
                            info!("Shutdown event received, stopping logger");
                            break;
                        }
                        other => {
                            debug!(event = ?other, "Event received");
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
