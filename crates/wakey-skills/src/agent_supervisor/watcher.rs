//! Agent Watcher — filesystem events + log polling
//!
//! Monitors:
//! - .gsd/STATE.md for state transitions
//! - .gsd/runtime/ for execution logs
//! - Terminal output via pty or log files

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Events emitted by the watcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatcherEvent {
    /// Agent state changed
    StateChanged {
        session_id: String,
        phase: String,
        detail: String,
    },

    /// Error detected in logs
    ErrorDetected {
        session_id: String,
        error: String,
        pattern: String,
    },

    /// Tool call recorded
    ToolCall {
        session_id: String,
        tool: String,
        args_hash: u64,
    },

    /// No activity timeout
    Timeout { session_id: String },
}

/// File path patterns for different agent types
#[derive(Debug, Clone)]
pub struct AgentPaths {
    /// State file (e.g., .gsd/STATE.md)
    pub state_file: PathBuf,

    /// Runtime log directory (e.g., .gsd/runtime/)
    pub runtime_dir: PathBuf,

    /// Activity log files (JSONL)
    pub activity_logs: Vec<PathBuf>,
}

/// Watches agent state files and logs for changes
pub struct AgentWatcher {
    /// Filesystem watcher (notify)
    watcher: RecommendedWatcher,

    /// Broadcast channel for events
    event_tx: broadcast::Sender<WatcherEvent>,

    /// Known agent paths by session ID
    paths: Arc<std::sync::Mutex<HashMap<String, AgentPaths>>>,

    /// Debounce duration
    #[allow(dead_code)]
    debounce: Duration,

    /// Last event times for debouncing
    #[allow(dead_code)]
    last_events: Arc<std::sync::Mutex<HashMap<PathBuf, std::time::Instant>>>,
}

impl AgentWatcher {
    /// Create a new agent watcher
    ///
    /// # Arguments
    /// * `watch_paths` - Root directories to watch (e.g., ["./.gsd"])
    /// * `debounce` - Minimum time between events for same file
    pub fn new(watch_paths: Vec<PathBuf>, debounce: Duration) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let paths = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let last_events = Arc::new(std::sync::Mutex::new(
            HashMap::<PathBuf, std::time::Instant>::new(),
        ));

        // Create the filesystem watcher
        let event_tx_clone = event_tx.clone();
        let last_events_clone = last_events.clone();
        let debounce_clone = debounce;

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Debounce: skip if too soon after last event for same path
                        if let Some(path) = event.paths.first() {
                            let mut last = last_events_clone.lock().unwrap();
                            let should_skip = last
                                .get(path)
                                .is_some_and(|last_time| last_time.elapsed() < debounce_clone);

                            if should_skip {
                                debug!(path = %path.display(), "Debouncing event");
                                return;
                            }
                            last.insert(path.clone(), std::time::Instant::now());
                        }

                        // Handle the event
                        handle_filesystem_event(&event_tx_clone, event);
                    }
                    Err(e) => {
                        error!(error = %e, "Watcher error");
                    }
                }
            },
            Config::default(),
        )
        .expect("Failed to create watcher");

        // Start watching each path
        for path in &watch_paths {
            if path.exists() {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    warn!(path = %path.display(), error = %e, "Failed to watch path");
                } else {
                    info!(path = %path.display(), "Watching for agent activity");
                }
            }
        }

        Self {
            watcher,
            event_tx,
            paths,
            debounce,
            last_events,
        }
    }

    /// Subscribe to watcher events
    pub fn subscribe(&self) -> broadcast::Receiver<WatcherEvent> {
        self.event_tx.subscribe()
    }

    /// Register a new agent session for watching
    pub fn register_session(&mut self, session_id: String, paths: AgentPaths) {
        // Add paths to watch
        if let Err(e) = self
            .watcher
            .watch(&paths.state_file, RecursiveMode::NonRecursive)
        {
            warn!(path = %paths.state_file.display(), error = %e, "Failed to watch state file");
        }

        if paths.runtime_dir.exists()
            && let Err(e) = self
                .watcher
                .watch(&paths.runtime_dir, RecursiveMode::Recursive)
        {
            warn!(path = %paths.runtime_dir.display(), error = %e, "Failed to watch runtime dir");
        }

        self.paths.lock().unwrap().insert(session_id, paths);
    }

    /// Stop watching a session
    pub fn unregister_session(&mut self, session_id: &str) {
        if let Some(paths) = self.paths.lock().unwrap().remove(session_id) {
            let _ = self.watcher.unwatch(&paths.state_file);
            let _ = self.watcher.unwatch(&paths.runtime_dir);
        }
    }

    /// Parse a GSD STATE.md file
    pub fn parse_gsd_state(content: &str) -> Option<GsdState> {
        // STATE.md format (example):
        // # GSD State
        //
        // Status: running
        // Phase: executing
        // Milestone: M001
        // Slice: S01
        // Task: T01
        // Updated: 2024-01-15T10:30:00Z

        let mut state = GsdState::default();

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("Status:") {
                state.status = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Phase:") {
                state.phase = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Milestone:") {
                state.milestone = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Slice:") {
                state.slice = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Task:") {
                state.task = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("Updated:") {
                state.updated = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
        }

        Some(state)
    }
}

/// Handle a filesystem event
fn handle_filesystem_event(event_tx: &broadcast::Sender<WatcherEvent>, event: Event) {
    // We care about create and modify events on specific files
    if !event.kind.is_create() && !event.kind.is_modify() {
        return;
    }

    for path in &event.paths {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        match file_name {
            "STATE.md" => {
                // Read and parse state
                if let Ok(content) = std::fs::read_to_string(path)
                    && let Some(state) = AgentWatcher::parse_gsd_state(&content)
                {
                    let session_id = infer_session_id(path);

                    let _ = event_tx.send(WatcherEvent::StateChanged {
                        session_id,
                        phase: state.phase,
                        detail: format!("Milestone: {}, Slice: {}", state.milestone, state.slice),
                    });
                }
            }
            name if name.ends_with(".jsonl") || name.ends_with(".log") => {
                // Parse log for errors and tool calls
                if let Ok(content) = std::fs::read_to_string(path) {
                    parse_log_content(event_tx, path, &content);
                }
            }
            _ => {}
        }
    }
}

/// Infer session ID from path
fn infer_session_id(path: &Path) -> String {
    // Extract session ID from path like .gsd/worktrees/M001/...
    // For now, use a simple heuristic

    path.components()
        .find_map(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.starts_with("M") && s.contains('-') {
                Some(s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "default".to_string())
}

/// Parse log content for errors and tool calls
fn parse_log_content(event_tx: &broadcast::Sender<WatcherEvent>, path: &Path, content: &str) {
    // Only parse the last ~100 lines to avoid re-processing old content
    let lines: Vec<&str> = content.lines().rev().take(100).collect();

    for line in lines.into_iter().rev() {
        // Try to parse as JSON
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            // Check for error patterns
            if let Some(error) = json.get("error").and_then(|e| e.as_str()) {
                let session_id = infer_session_id(path);
                let _ = event_tx.send(WatcherEvent::ErrorDetected {
                    session_id,
                    error: error.to_string(),
                    pattern: classify_error(error),
                });
            }

            // Check for tool calls
            if let Some(tool) = json.get("tool").and_then(|t| t.as_str()) {
                let args = json.get("args").unwrap_or(&serde_json::Value::Null);
                let args_hash = calculate_hash(&args.to_string());

                let session_id = infer_session_id(path);
                let _ = event_tx.send(WatcherEvent::ToolCall {
                    session_id,
                    tool: tool.to_string(),
                    args_hash,
                });
            }
        }
    }
}

/// Classify an error message to determine fix pattern
fn classify_error(error: &str) -> String {
    let error_lower = error.to_lowercase();

    if error_lower.contains("cannot find crate") || error_lower.contains("unresolved import") {
        "missing_crate".to_string()
    } else if error_lower.contains("error[e0") {
        "compile_error".to_string()
    } else if error_lower.contains("rate limit") || error_lower.contains("429") {
        "rate_limit".to_string()
    } else if error_lower.contains("unauthorized") || error_lower.contains("401") {
        "auth_failure".to_string()
    } else if error_lower.contains("fmt") || error_lower.contains("format") {
        "format_error".to_string()
    } else if error_lower.contains("clippy") {
        "lint_error".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Simple hash function for args
fn calculate_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// GSD state parsed from STATE.md
#[derive(Debug, Clone, Default)]
pub struct GsdState {
    pub status: String,
    pub phase: String,
    pub milestone: String,
    pub slice: String,
    pub task: String,
    pub updated: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gsd_state() {
        let content = r#"# GSD State

Status: running
Phase: executing
Milestone: M001
Slice: S01
Task: T01
Updated: 2024-01-15T10:30:00Z
"#;

        let state = AgentWatcher::parse_gsd_state(content).unwrap();
        assert_eq!(state.status, "running");
        assert_eq!(state.phase, "executing");
        assert_eq!(state.milestone, "M001");
    }

    #[test]
    fn test_classify_error() {
        assert_eq!(classify_error("cannot find crate `serde`"), "missing_crate");
        assert_eq!(classify_error("error[E0277]: trait bound"), "compile_error");
        assert_eq!(classify_error("rate limit exceeded"), "rate_limit");
        assert_eq!(classify_error("401 Unauthorized"), "auth_failure");
    }
}
