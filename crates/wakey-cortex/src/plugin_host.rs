//! Voice Plugin Host — Spawns subprocess and communicates via JSON stdin/stdout.
//!
//! Architecture:
//! ```text
//! ┌─────────────────┐     JSON lines      ┌──────────────────┐
//! │   Wakey Core    │ ←────────────────── → │  Voice Plugin    │
//! │   (Rust)        │    stdin/stdout     │   (any language) │
//! │                 │                      │                  │
//! │  Spine events:  │                      │  Events emitted: │
//! │  ShouldSpeak →  │ ──{"event":"ShouldSpeak","text":"Hi"}──→ │ (plugin speaks) │
//! │  ← VoiceUserSpeaking │ {"event":"VoiceUserSpeaking",...}──│                  │
//! │  ← VoiceWakeyThinking│ {"event":"VoiceWakeyThinking"}───│                  │
//! │  ← VoiceWakeySpeaking│ {"event":"VoiceWakeySpeaking",...}─│                  │
//! │  ← VoiceSessionEnded │ {"event":"VoiceSessionEnded"}───│                  │
//! │  Shutdown →    │ ──{"event":"Shutdown"}─────────────────→ │ (plugin exits)  │
//! └─────────────────┘                      └──────────────────┘
//! ```
//!
//! Key features:
//! - Voice becomes a PLUGIN (Python, Node, Rust, any language)
//! - Core stays clean (no audio dependencies)
//! - JSON lines protocol (simple, debuggable)
//! - Automatic restart on crash
//! - Graceful shutdown via Shutdown event

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Receiver;
use tracing::{debug, error, info, warn};

use wakey_spine::Spine;
use wakey_types::{WakeyEvent, event::Urgency};

/// Plugin configuration from config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Enable voice plugin
    pub enabled: bool,

    /// Plugin name (e.g., "voice-livekit", "voice-deepgram")
    pub plugin: String,

    /// Path to plugin executable/script
    pub plugin_path: PathBuf,

    /// Command to run plugin (e.g., "python3", "node", or direct binary path)
    pub plugin_command: String,

    /// Environment variables to pass to plugin process
    /// Keys are env var names, values can contain ${VAR} references
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            plugin: "voice-none".to_string(),
            plugin_path: PathBuf::from("plugins/voice-none/main.rs"),
            plugin_command: "cargo run".to_string(),
            env: HashMap::new(),
        }
    }
}

/// JSON event sent TO plugin (Wakey → Plugin).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInputEvent {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub urgency: Option<String>,
}

/// JSON event received FROM plugin (Plugin → Wakey).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOutputEvent {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_final: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Plugin host — manages subprocess lifecycle and event translation.
///
/// Spawns the voice plugin as a subprocess, reads JSON events from stdout,
/// emits WakeyEvents on spine, and writes ShouldSpeak events to stdin.
pub struct PluginHost {
    config: PluginConfig,
    spine: Spine,
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    stdout_reader: Option<BufReader<ChildStdout>>,
    running: Arc<AtomicBool>,
    restart_count: u32,
    max_restarts: u32,
}

impl PluginHost {
    /// Create a new plugin host.
    pub fn new(config: PluginConfig, spine: Spine) -> Self {
        Self {
            config,
            spine,
            process: None,
            stdin: None,
            stdout_reader: None,
            running: Arc::new(AtomicBool::new(false)),
            restart_count: 0,
            max_restarts: 5,
        }
    }

    /// Start the plugin subprocess.
    ///
    /// Returns Ok if the process spawned successfully.
    /// Returns error if spawn fails or plugin exits immediately.
    pub fn start(&mut self) -> Result<(), PluginError> {
        if !self.config.enabled {
            info!("Voice plugin disabled in config");
            return Err(PluginError::Disabled);
        }

        self.spawn_process()?;
        self.running.store(true, Ordering::SeqCst);

        // Emit that we're listening (voice ready)
        self.spine.emit(WakeyEvent::VoiceListeningStarted);

        info!(
            plugin = %self.config.plugin,
            command = %self.config.plugin_command,
            "Voice plugin started"
        );

        Ok(())
    }

    /// Spawn the subprocess and set up stdin/stdout pipes.
    fn spawn_process(&mut self) -> Result<(), PluginError> {
        // Expand env vars (e.g., ${DEEPGRAM_API_KEY} → actual value)
        let env_vars = self.expand_env_vars();

        // Build command
        let mut cmd = Command::new(&self.config.plugin_command);

        // Add plugin path as argument (for scripts)
        cmd.arg(&self.config.plugin_path);

        // Set up pipes
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Let stderr go to Wakey's logs

        // Pass environment variables
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Spawn process
        let mut child = cmd.spawn().map_err(|e| {
            PluginError::SpawnFailed(format!(
                "Failed to spawn plugin '{}': {}",
                self.config.plugin, e
            ))
        })?;

        // Take stdin and stdout handles
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| PluginError::SpawnFailed("Failed to open plugin stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PluginError::SpawnFailed("Failed to open plugin stdout".to_string()))?;

        self.process = Some(child);
        self.stdin = Some(stdin);
        self.stdout_reader = Some(BufReader::new(stdout));

        Ok(())
    }

    /// Expand ${VAR} references in env vars.
    fn expand_env_vars(&self) -> HashMap<String, String> {
        let mut expanded = HashMap::new();

        for (key, template) in &self.config.env {
            // Replace ${VAR} with actual env var value
            let mut value = template.clone();

            // Simple ${VAR} expansion
            if template.contains("${") {
                // Find all ${...} patterns
                let start = template.find("${").unwrap_or(0);
                let end = template.find('}').unwrap_or(template.len());

                if start < end {
                    let var_name = &template[start + 2..end];
                    let env_value = std::env::var(var_name).unwrap_or_default();

                    // Replace ${VAR} with value
                    value = template.replace(&format!("${{{}}}", var_name), &env_value);
                }
            }

            expanded.insert(key.clone(), value);
        }

        expanded
    }

    /// Run the plugin host loop.
    ///
    /// This is the main loop that:
    /// 1. Reads JSON events from plugin stdout → emits to spine
    /// 2. Listens for ShouldSpeak events from spine → writes to plugin stdin
    /// 3. Handles process crash and restart
    /// 4. Handles shutdown
    ///
    /// Blocks until shutdown or unrecoverable error.
    pub async fn run(&mut self, mut shutdown_rx: Receiver<WakeyEvent>) {
        let mut spine_rx = self.spine.subscribe();

        info!("Plugin host loop started");

        loop {
            // Check if process is alive
            if self.process.is_some() {
                // Try to read from stdout (non-blocking check)
                self.read_and_emit_events();
            }

            // Check if we need to restart (process died)
            let process_dead = self.process.as_mut().is_none_or(|p| {
                p.try_wait().map_or(true, |opt| opt.is_some())
            });

            if self.running.load(Ordering::SeqCst) && process_dead {
                // Process died or hasn't started yet
                if self.restart_count < self.max_restarts {
                    self.restart_count += 1;
                    warn!(
                        restart_count = self.restart_count,
                        plugin = %self.config.plugin,
                        "Plugin crashed, restarting..."
                    );

                    if let Err(e) = self.spawn_process() {
                        error!(error = %e, "Failed to restart plugin");
                        self.running.store(false, Ordering::SeqCst);
                        break;
                    }

                    // Small delay before continuing
                    tokio::time::sleep(Duration::from_millis(500)).await;
                } else {
                    error!(
                        max_restarts = self.max_restarts,
                        "Plugin crashed too many times, stopping"
                    );
                    self.running.store(false, Ordering::SeqCst);
                    self.spine.emit(WakeyEvent::VoiceError {
                        message: "Plugin crashed too many times".to_string(),
                    });
                    break;
                }
            }

            // Handle events from spine (ShouldSpeak)
            tokio::select! {
                // Check spine for ShouldSpeak events
                Ok(event) = spine_rx.recv() => {
                    self.handle_spine_event(&event);
                }

                // Shutdown signal
                Ok(WakeyEvent::Shutdown) = shutdown_rx.recv() => {
                    info!("Plugin host shutting down");
                    self.stop();
                    break;
                }

                // Small sleep to prevent busy loop
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        }

        info!("Plugin host loop ended");
    }

    /// Read JSON events from plugin stdout and emit to spine.
    fn read_and_emit_events(&mut self) {
        if let Some(ref mut reader) = self.stdout_reader {
            // Read one line (blocking, but we check quickly)
            let mut line = String::new();

            // Use a quick read attempt
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF - process died
                    debug!("Plugin stdout EOF");
                }
                Ok(_) => {
                    // Parse JSON event
                    let line = line.trim();
                    if line.is_empty() {
                        return;
                    }

                    debug!(line = %line, "Plugin event received");

                    match serde_json::from_str::<PluginOutputEvent>(line) {
                        Ok(event) => self.emit_plugin_event(event),
                        Err(e) => {
                            warn!(error = %e, line = %line, "Failed to parse plugin event");
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "Failed to read from plugin stdout");
                }
            }
        }
    }

    /// Convert plugin output event to WakeyEvent and emit to spine.
    fn emit_plugin_event(&self, event: PluginOutputEvent) {
        let wakey_event = match event.event.as_str() {
            "VoiceListeningStarted" => WakeyEvent::VoiceListeningStarted,
            "VoiceListeningStopped" => WakeyEvent::VoiceListeningStopped,
            "VoiceUserSpeaking" => WakeyEvent::VoiceUserSpeaking {
                text: event.text.unwrap_or_default(),
                is_final: event.is_final.unwrap_or(false),
            },
            "VoiceWakeyThinking" => WakeyEvent::VoiceWakeyThinking,
            "VoiceWakeySpeaking" => WakeyEvent::VoiceWakeySpeaking {
                text: event.text.unwrap_or_default(),
            },
            "VoiceSessionEnded" => WakeyEvent::VoiceSessionEnded,
            "VoiceError" => WakeyEvent::VoiceError {
                message: event.message.unwrap_or_default(),
            },
            other => {
                warn!(event = other, "Unknown plugin event type");
                return;
            }
        };

        info!(event = %event.event, "Emitting spine event from plugin");
        self.spine.emit(wakey_event);
    }

    /// Handle event from spine (ShouldSpeak → write to plugin stdin).
    fn handle_spine_event(&mut self, event: &WakeyEvent) {
        match event {
            WakeyEvent::ShouldSpeak {
                suggested_text: Some(text),
                urgency,
                ..
            } => {
                self.send_to_plugin(PluginInputEvent {
                    event: "ShouldSpeak".to_string(),
                    text: Some(text.clone()),
                    urgency: Some(urgency_to_string(urgency)),
                });
            }
            WakeyEvent::Shutdown => {
                self.send_to_plugin(PluginInputEvent {
                    event: "Shutdown".to_string(),
                    text: None,
                    urgency: None,
                });
            }
            _ => {}
        }
    }

    /// Send JSON event to plugin stdin.
    fn send_to_plugin(&mut self, event: PluginInputEvent) {
        if let Some(ref mut stdin) = self.stdin {
            let json = serde_json::to_string(&event).unwrap_or_default();
            debug!(json = %json, "Sending to plugin");

            if let Err(e) = stdin.write_all(format!("{}\n", json).as_bytes()) {
                warn!(error = %e, "Failed to write to plugin stdin");
            }

            if let Err(e) = stdin.flush() {
                warn!(error = %e, "Failed to flush plugin stdin");
            }
        }
    }

    /// Stop the plugin and clean up.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        // Send shutdown event to plugin
        self.send_to_plugin(PluginInputEvent {
            event: "Shutdown".to_string(),
            text: None,
            urgency: None,
        });

        // Kill process if still running
        if let Some(ref mut child) = self.process
            && child.try_wait().is_ok_and(|opt| opt.is_none())
        {
            // Process still running, kill it
            if let Err(e) = child.kill() {
                warn!(error = %e, "Failed to kill plugin process");
            }
        }

        // Emit session ended
        self.spine.emit(WakeyEvent::VoiceSessionEnded);

        info!("Plugin stopped");
    }

    /// Check if plugin is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Convert Urgency enum to string for JSON serialization.
fn urgency_to_string(urgency: &Urgency) -> String {
    match urgency {
        Urgency::Low => "low",
        Urgency::Medium => "medium",
        Urgency::High => "high",
        Urgency::Critical => "critical",
    }
    .to_string()
}

/// Plugin host errors.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Voice plugin is disabled")]
    Disabled,

    #[error("Failed to spawn plugin: {0}")]
    SpawnFailed(String),

    #[error("Failed to communicate with plugin: {0}")]
    CommunicationFailed(String),

    #[error("Plugin crashed: {0}")]
    Crashed(String),
}

impl Drop for PluginHost {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        // SAFETY: Test-only code, setting env var for test purposes
        unsafe {
            std::env::set_var("TEST_KEY", "test_value");
        }

        let config = PluginConfig {
            enabled: true,
            plugin: "test".to_string(),
            plugin_path: PathBuf::from("test"),
            plugin_command: "test".to_string(),
            env: HashMap::from([
                ("DIRECT_KEY".to_string(), "direct_value".to_string()),
                ("EXPANDED_KEY".to_string(), "${TEST_KEY}".to_string()),
            ]),
        };

        let host = PluginHost::new(config, Spine::new());
        let expanded = host.expand_env_vars();

        assert_eq!(
            expanded.get("DIRECT_KEY"),
            Some(&"direct_value".to_string())
        );
        assert_eq!(
            expanded.get("EXPANDED_KEY"),
            Some(&"test_value".to_string())
        );
    }

    #[test]
    fn test_plugin_input_event_serialization() {
        let event = PluginInputEvent {
            event: "ShouldSpeak".to_string(),
            text: Some("Hello!".to_string()),
            urgency: Some("low".to_string()),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("ShouldSpeak"));
        assert!(json.contains("Hello!"));
        assert!(json.contains("low"));
    }

    #[test]
    fn test_plugin_output_event_deserialization() {
        let json = r#"{"event":"VoiceUserSpeaking","text":"hello","is_final":true}"#;
        let event: PluginOutputEvent = serde_json::from_str(json).unwrap();

        assert_eq!(event.event, "VoiceUserSpeaking");
        assert_eq!(event.text, Some("hello".to_string()));
        assert_eq!(event.is_final, Some(true));
    }
}
