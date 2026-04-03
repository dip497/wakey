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
//! - Agent supervision for GSD (P3)

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

    // Check if .gsd/ exists for agent supervision
    let gsd_exists = PathBuf::from(".gsd").exists();

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
            let skill_registry = {
                // Determine skills directories to scan
                // 1. User's data directory (e.g., ~/.wakey/context/agent/skills)
                // 2. Project-local builtin skills (./skills/builtin) if running from project
                let mut registry = wakey_skills::registry::new(&skills_dir, &index_db);

                let mut count = 0;

                // Scan project-local builtin skills first
                let builtin_skills_dir = PathBuf::from("skills/builtin");
                if builtin_skills_dir.exists() {
                    // Create a temporary registry for builtin skills
                    let builtin_db = std::env::temp_dir().join("wakey_builtin_skills.db");
                    if let Ok(mut builtin_registry) =
                        wakey_skills::registry::new(&builtin_skills_dir, &builtin_db)
                    {
                        match builtin_registry.scan() {
                            Ok(n) => {
                                count += n;
                                info!(count = n, path = %builtin_skills_dir.display(), "Builtin skills indexed");
                                // List skills for logging
                                for skill in builtin_registry.list() {
                                    debug!(name = %skill.name, "Builtin skill available");
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to scan builtin skills");
                            }
                        }
                    }
                }

                // Scan user's skills directory
                if let Ok(ref mut reg) = registry
                    && skills_dir.exists()
                {
                    match reg.scan() {
                        Ok(n) => {
                            count += n;
                            info!(count = n, path = %skills_dir.display(), "User skills indexed");
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to scan user skills");
                        }
                    }
                }

                info!(total = count, "Total skills indexed");
                registry.ok().map(Arc::new)
            };

            // Create agent loop with memory and skills
            // Note: AgentLoop is not Send due to SkillRegistry, so we keep it local
            // and use it for context assembly within this async block if needed
            let _agent_loop = AgentLoop::new(
                provider_clone.clone(),
                memory_clone.clone(),
                skill_registry,
                spine_clone.clone(),
                config_clone.persona.clone(),
            );

            // Start decision loop (calls LLM periodically)
            let decision_shutdown_rx = spine_clone.subscribe();
            tokio::spawn(decision_loop(
                spine_clone.clone(),
                provider_clone,
                memory_clone,
                config_clone.persona.name.clone(),
                decision_shutdown_rx,
            ));

            // Start agent supervisor if .gsd/ exists
            if gsd_exists {
                info!("Detected .gsd/ directory - starting agent supervisor");
                let supervisor_spine = spine_clone.clone();
                tokio::spawn(async move {
                    let config = wakey_skills::agent_supervisor::SupervisorConfig::default();
                    let supervisor = wakey_skills::agent_supervisor::AgentSupervisor::new(
                        config,
                        supervisor_spine,
                    );
                    supervisor.run().await;
                });
            }

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
/// Enhanced version (P3):
/// - Triggers on Tick count (every 15 ticks ≈ 30s with 2s tick interval)
/// - Startup greeting after 5 seconds
/// - Stores conversations in memory
/// - Recalls last session on startup
async fn decision_loop(
    spine: Spine,
    provider: Arc<dyn LlmProvider>,
    memory: Arc<dyn Memory>,
    persona_name: String,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<WakeyEvent>,
) {
    let mut event_rx = spine.subscribe();
    let mut tick_count = 0u32;
    let mut last_window = String::new();
    let mut startup_done = false;

    // Tick-based speech interval (15 ticks ≈ 30s at 2s tick interval)
    const TICKS_BETWEEN_SPEECH: u32 = 15;

    info!("Decision loop started");

    // Startup greeting after a short delay
    let startup_spine = spine.clone();
    let startup_persona = persona_name.clone();
    tokio::spawn(async move {
        // Wait 5 seconds for overlay to be ready
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Emit startup greeting
        startup_spine.emit(WakeyEvent::ShouldSpeak {
            reason: "startup".to_string(),
            urgency: wakey_types::event::Urgency::Low,
            suggested_text: Some(format!(
                "Hi there! I'm {}, your desktop companion. I'll be here when you need me. Just doing my thing while you work!",
                startup_persona
            )),
        });

        info!("Startup greeting emitted");
    });

    loop {
        tokio::select! {
            // Handle events from spine
            Ok(event) = event_rx.recv() => {
                match &event {
                    WakeyEvent::Tick => {
                        tick_count += 1;

                        // First tick: mark startup complete
                        if !startup_done {
                            startup_done = true;
                            info!("Startup complete, tick-based decision loop active");
                        }

                        // Every N ticks, ask LLM to say something (if we have window context)
                        if tick_count.is_multiple_of(TICKS_BETWEEN_SPEECH) && !last_window.is_empty() {
                            info!(tick = tick_count, window = %last_window, "Periodic speech trigger");
                            ask_llm_to_speak(&spine, &provider, &memory, &persona_name, &last_window).await;
                        }
                    }

                    WakeyEvent::WindowFocusChanged { app, title, .. } => {
                        let window = format!("{} - {}", app, title);
                        if window != last_window {
                            debug!(window = %window, "Window focus changed");
                            last_window = window.clone();
                        }
                    }

                    WakeyEvent::Breath => {
                        // Periodic reflection - could trigger memory compaction
                        debug!("Breath event received");
                    }

                    WakeyEvent::Reflect => {
                        info!("Reflect event received, processing...");
                        // Memory reflection - summarize and compact
                        handle_reflect_event(&memory, &spine).await;
                    }

                    WakeyEvent::AgentProgress { agent_type, phase, detail } => {
                        // Report GSD progress
                        info!(agent = %agent_type, phase = %phase, "Agent progress");
                        spine.emit(WakeyEvent::ShouldSpeak {
                            reason: "agent_progress".to_string(),
                            urgency: wakey_types::event::Urgency::Low,
                            suggested_text: Some(format!("{} is now {} - {}", agent_type, phase, detail)),
                        });
                    }

                    WakeyEvent::AgentStuck { agent_type, reason, duration_secs } => {
                        warn!(agent = %agent_type, reason = %reason, duration = duration_secs, "Agent stuck");
                        spine.emit(WakeyEvent::ShouldSpeak {
                            reason: "agent_stuck".to_string(),
                            urgency: wakey_types::event::Urgency::Medium,
                            suggested_text: Some(format!(
                                "Your {} worker seems stuck ({}). Want me to check on it?",
                                agent_type, reason
                            )),
                        });
                    }

                    WakeyEvent::Dream => {
                        // Heavy pattern learning - not implemented yet
                        info!("Dream event - pattern learning not yet implemented");
                    }

                    _ => {
                        // Other events are logged by EventLogger
                    }
                }
            }

            // Shutdown signal
            Ok(WakeyEvent::Shutdown) = shutdown_rx.recv() => {
                info!("Decision loop shutting down");

                // Store session summary before shutdown
                let session_summary = format!(
                    "Session ended at {}. Last window: {}. Total ticks: {}.",
                    chrono::Utc::now().format("%H:%M"),
                    last_window,
                    tick_count
                );
                if let Err(e) = memory.store("session/last.md", &session_summary, &wakey_context::MemoryCategory::Daily).await {
                    warn!(error = ?e, "Failed to store session summary");
                } else {
                    info!("Session summary stored");
                }

                break;
            }
        }
    }
}

/// Handle Reflect event - summarize and compact memory
async fn handle_reflect_event(memory: &Arc<dyn Memory>, spine: &Spine) {
    // Get recent conversation memories
    match memory
        .list(Some(&wakey_context::MemoryCategory::Conversation))
        .await
    {
        Ok(memories) => {
            if !memories.is_empty() {
                let summary = format!(
                    "Reflected on {} conversations. Most recent: {}",
                    memories.len(),
                    memories.first().map(|m| m.l0()).unwrap_or("none")
                );

                // Store the reflection
                if let Err(e) = memory
                    .store(
                        &format!(
                            "reflection/{}.md",
                            chrono::Utc::now().format("%Y%m%d_%H%M%S")
                        ),
                        &summary,
                        &wakey_context::MemoryCategory::Daily,
                    )
                    .await
                {
                    warn!(error = ?e, "Failed to store reflection");
                }

                // Emit that we reflected
                spine.emit(WakeyEvent::ShouldRemember {
                    content: summary,
                    importance: wakey_types::event::Importance::ShortTerm,
                });
            }
        }
        Err(e) => {
            warn!(error = ?e, "Failed to list memories for reflection");
        }
    }
}

/// Ask the LLM to generate something to say about the current context.
///
/// This is the MVP "proactive speech" logic. Now enhanced with memory storage.
async fn ask_llm_to_speak(
    spine: &Spine,
    provider: &Arc<dyn LlmProvider>,
    memory: &Arc<dyn Memory>,
    persona_name: &str,
    current_window: &str,
) {
    // Build context from memory
    let context_memories = match memory.recall(current_window, 3).await {
        Ok(mems) => mems,
        Err(e) => {
            warn!(error = ?e, "Failed to recall memories for context");
            vec![]
        }
    };

    let memory_context = if !context_memories.is_empty() {
        let mem_strs: Vec<String> = context_memories
            .iter()
            .map(|m| m.l0().to_string())
            .collect();
        format!("\n\nRelevant memories:\n- {}", mem_strs.join("\n- "))
    } else {
        String::new()
    };

    let prompt = format!(
        "You are {}, a friendly AI companion that lives on the user's desktop. \
         The user is currently looking at: {}.{} \
         Say something brief, helpful, or encouraging (1-2 sentences max). \
         Be casual and friendly, like a supportive friend. \
         Don't be robotic or overly formal. \
         If they're in a coding/working context, be encouraging. \
         If they're in entertainment, maybe joke about it. \
         Keep it short and natural.",
        persona_name, current_window, memory_context
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
    let memory_clone = memory.clone();
    let window_clone = current_window.to_string();
    let persona_clone = persona_name.to_string();
    tokio::spawn(async move {
        match provider_clone.chat(&messages).await {
            Ok(response) => {
                info!(response_len = response.len(), "LLM responded");

                // Store conversation in memory
                let conversation_key = format!(
                    "conversation/{}.md",
                    chrono::Utc::now().format("%Y%m%d_%H%M%S")
                );
                let conversation_content = format!(
                    "## Context\nWindow: {}\n\n## {} said:\n{}",
                    window_clone, persona_clone, response
                );
                if let Err(e) = memory_clone
                    .store(
                        &conversation_key,
                        &conversation_content,
                        &wakey_context::MemoryCategory::Conversation,
                    )
                    .await
                {
                    warn!(error = ?e, "Failed to store conversation in memory");
                } else {
                    debug!("Conversation stored in memory");
                }

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
