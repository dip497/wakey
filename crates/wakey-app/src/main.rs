//! Wakey — Your laptop, alive.
//!
//! This is the main entry point that wires all subsystems together:
//! - Config loader (S1)
//! - Heartbeat with window detection (S2)
//! - LLM provider (S3)
//! - Overlay with sprite and chat bubble (S4)
//! - Decision loop that calls LLM periodically (S5)
//! - Voice mode with push-to-talk (S6)
//! - Memory + Skills integration (P2-S3)

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use wakey_context::{Memory, SqliteMemory};
use wakey_cortex::AgentLoop;
use wakey_cortex::heartbeat::HeartbeatRunner;
use wakey_cortex::llm::{LlmProvider, OpenAiCompatible};
use wakey_cortex::voice::VoiceSession;
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
    let config = WakeyConfig::load(PathBuf::from("config/default.toml").as_path())?;
    info!(
        persona = config.persona.name,
        provider = config.llm.default_provider,
        "Configuration loaded"
    );

    // Create the central nervous system
    let spine = Spine::new();
    info!(subscribers = spine.subscriber_count(), "Spine initialized");

    // Initialize memory and skills (P2-S3)
    let memory = init_memory(&config)?;

    // Create LLM provider from config
    let provider = create_llm_provider(&config)?;

    // Start heartbeat runner and decision loop in a background thread with tokio runtime
    let spine_clone = spine.clone();
    let config_clone = config.clone();
    let provider_clone = provider.clone();
    let memory_clone = memory.clone();
    let skills_dir = config
        .general
        .data_dir
        .join("context")
        .join("agent")
        .join("skills");
    let index_db = config.general.data_dir.join("context").join("skills.db");
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

            // Create skill registry inside the async block (not Send safe)
            // SkillRegistry contains rusqlite::Connection which is not Send/Sync,
            // but we're using single-threaded tokio runtime so this is safe.
            #[allow(clippy::arc_with_non_send_sync)]
            let skill_registry = match wakey_skills::registry::new(&skills_dir, &index_db) {
                Ok(mut registry) => {
                    let count = registry.scan().ok().unwrap_or(0);
                    info!(count = count, "Skills indexed");
                    Some(Arc::new(registry))
                }
                Err(e) => {
                    warn!(error = %e, "Failed to initialize skills");
                    None
                }
            };

            // Create agent loop with memory and skills
            let _agent_loop = AgentLoop::new(
                provider_clone.clone(),
                memory_clone,
                skill_registry,
                spine_clone.clone(),
                config_clone.persona.clone(),
            );

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

    // Start voice session in its own thread (cpal Stream is !Send, can't use tokio::spawn)
    // Voice runs independently and uses server-side VAD for speech detection.
    // Push-to-talk (Space key) will be added when global keyboard hook is implemented.
    if config.voice.enabled {
        let voice_spine = spine.clone();
        let voice_config = config.voice.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Voice runtime failed");

            rt.block_on(async move {
                info!("Voice session started (push-to-talk: Space, VAD: enabled)");

                // Run voice sessions in a loop
                loop {
                    match VoiceSession::new(voice_config.clone(), voice_spine.clone()) {
                        Ok(mut session) => {
                            // Run one STT→LLM→TTS cycle
                            if let Err(e) = session.start().await {
                                // Check if it's a shutdown or disabled error
                                match e {
                                    wakey_cortex::voice::VoiceError::Disabled => {
                                        info!("Voice disabled in config");
                                        break;
                                    }
                                    other => {
                                        warn!(error = %other, "Voice session ended, restarting...");
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to create voice session");
                            // Don't restart if API key is missing
                            if matches!(e, wakey_cortex::voice::VoiceError::MissingApiKey(_)) {
                                break;
                            }
                        }
                    }

                    // Small pause between sessions
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }

                info!("Voice session stopped");
            });
        });
    } else {
        info!("Voice mode disabled in config");
    }

    info!("Starting overlay windows...");

    // Run overlay on main thread (blocks until window closes)
    run_overlay_with_spine(spine);

    info!("Wakey is going to sleep. Goodnight.");
    Ok(())
}

/// Initialize memory backend
fn init_memory(config: &WakeyConfig) -> Result<Arc<dyn Memory>> {
    let data_dir = config.general.data_dir.join("context");
    std::fs::create_dir_all(&data_dir).ok();

    let db_path = data_dir.join("index.db");
    let memory = SqliteMemory::new(db_path).map_err(|e| anyhow::anyhow!("Memory init: {}", e))?;

    info!(
        path = %config.general.data_dir.display(),
        "Memory initialized"
    );

    Ok(Arc::new(memory))
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
                match &event {
                    WakeyEvent::WindowFocusChanged { app, title, .. } => {
                        let window = format!("{} - {}", app, title);
                        if window != last_window {
                            debug!(window = %window, "Window focus changed");
                            last_window = window.clone();
                            focus_count += 1;

                            // Every 5th focus change, ask LLM to say something
                            if focus_count.is_multiple_of(5) {
                                ask_llm_to_speak(&spine, &provider, &persona_name, &last_window).await;
                            }
                        }
                    }

                    WakeyEvent::Reflect => {
                        info!("Reflect event received, processing...");
                        // Memory reflection handled by agent_loop
                    }

                    WakeyEvent::Tick | WakeyEvent::Breath | WakeyEvent::Dream => {
                        // These are handled by heartbeat runner
                    }

                    _ => {
                        // Other events are logged by EventLogger
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
/// This is the MVP "proactive speech" logic. Now enhanced with memory and skills.
async fn ask_llm_to_speak(
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
                        // Voice events
                        WakeyEvent::VoiceListeningStarted => {
                            info!("Voice: Listening started (mic active)");
                        }
                        WakeyEvent::VoiceListeningStopped => {
                            info!("Voice: Listening stopped");
                        }
                        WakeyEvent::VoiceUserSpeaking { text, is_final } => {
                            if *is_final {
                                info!(text = %text, "Voice: User said (final)");
                            } else {
                                debug!(text = %text, "Voice: User speaking (intermediate)");
                            }
                        }
                        WakeyEvent::VoiceWakeyThinking => {
                            info!("Voice: Wakey thinking...");
                        }
                        WakeyEvent::VoiceWakeySpeaking { text } => {
                            info!(text = %text, "Voice: Wakey speaking");
                        }
                        WakeyEvent::VoiceSessionEnded => {
                            info!("Voice: Session ended");
                        }
                        WakeyEvent::VoiceError { message } => {
                            warn!(message = %message, "Voice: Error");
                        }
                        // Memory and skill events
                        WakeyEvent::ShouldRemember {
                            content,
                            importance,
                        } => {
                            info!(content = %content, importance = ?importance, "Should remember");
                        }
                        WakeyEvent::SkillExtracted { name, description } => {
                            info!(name = %name, description = %description, "Skill extracted");
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
