//! Agent Supervisor — Monitor and manage AI coding agents
//!
//! Based on Composio's architecture:
//! - Push, not pull (notifications are primary interface)
//! - Two-tier handling: Tier 1 (auto-fix) vs Tier 2 (notify human)
//!
//! Uses ZeroClaw's loop detection pattern:
//! - Warning → Block → Break escalation
//!
//! Components:
//! - AgentWatcher: filesystem events + log polling
//! - StuckDetector: sliding window state analysis
//! - AutoFixer: Tier 1 auto-fix with Cedar policy check
//! - Reporter: Tier 2 notifications via spine

pub mod detector;
pub mod fixer;
pub mod reporter;
pub mod watcher;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::info;

use wakey_spine::Spine;
use wakey_types::WakeyEvent;

pub use detector::{LoopDetectionResult, StuckDetector, StuckReason};
pub use fixer::{AutoFixer, FixAction, FixResult};
pub use reporter::Reporter;
pub use watcher::AgentWatcher;

/// Agent types we can supervise
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentType {
    /// GSD headless session
    Gsd,
    /// Claude Code session
    ClaudeCode,
    /// OpenAI Codex session
    Codex,
    /// Generic terminal session
    Generic,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Gsd => write!(f, "GSD"),
            AgentType::ClaudeCode => write!(f, "Claude Code"),
            AgentType::Codex => write!(f, "Codex"),
            AgentType::Generic => write!(f, "Agent"),
        }
    }
}

/// Configuration for agent supervision
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// Seconds without activity before flagging as stuck
    pub activity_timeout_secs: u64,

    /// Loop detection: warn after N identical tool calls
    pub loop_warning_threshold: usize,

    /// Loop detection: block after N identical tool calls
    pub loop_block_threshold: usize,

    /// Loop detection: break after N identical tool calls
    pub loop_break_threshold: usize,

    /// Maximum auto-fix retries before escalating
    pub max_auto_retries: usize,

    /// Seconds to wait after fix before checking recovery
    pub fix_monitor_window_secs: u64,

    /// Milliseconds to debounce filesystem events
    pub debounce_ms: u64,

    /// Paths to watch for agent state
    pub watch_paths: Vec<PathBuf>,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            activity_timeout_secs: 300,
            loop_warning_threshold: 2,
            loop_block_threshold: 4,
            loop_break_threshold: 6,
            max_auto_retries: 3,
            fix_monitor_window_secs: 30,
            debounce_ms: 500,
            watch_paths: vec![PathBuf::from(".gsd")],
        }
    }
}

/// Agent session being supervised
#[derive(Debug, Clone)]
pub struct AgentSession {
    /// Unique session identifier
    pub id: String,

    /// Type of agent
    pub agent_type: AgentType,

    /// Task being worked on
    pub task: String,

    /// Worktree path if applicable
    pub worktree: Option<PathBuf>,

    /// Process ID if known
    pub pid: Option<u32>,

    /// Current phase
    pub phase: String,

    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,

    /// Number of consecutive errors
    pub error_count: usize,

    /// Number of fix retries attempted
    pub retry_count: usize,
}

/// Main supervisor struct — coordinates all components
pub struct AgentSupervisor {
    config: SupervisorConfig,
    watcher: AgentWatcher,
    detector: Arc<Mutex<StuckDetector>>,
    fixer: AutoFixer,
    reporter: Reporter,
    spine: Spine,
    sessions: Arc<Mutex<Vec<AgentSession>>>,
}

impl AgentSupervisor {
    /// Create a new agent supervisor
    pub fn new(config: SupervisorConfig, spine: Spine) -> Self {
        let detector = Arc::new(Mutex::new(StuckDetector::new(&config)));
        let reporter = Reporter::new(spine.clone());
        let fixer = AutoFixer::new(config.max_auto_retries);

        let watcher = AgentWatcher::new(
            config.watch_paths.clone(),
            Duration::from_millis(config.debounce_ms),
        );

        Self {
            config,
            watcher,
            detector,
            fixer,
            reporter,
            spine,
            sessions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start supervising a new agent session
    pub async fn spawn_session(
        &self,
        agent_type: AgentType,
        task: String,
        worktree: Option<PathBuf>,
    ) -> String {
        let session_id = format!(
            "{}-{}",
            agent_type.to_string().to_lowercase().replace(' ', "-"),
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("unknown")
        );

        let session = AgentSession {
            id: session_id.clone(),
            agent_type: agent_type.clone(),
            task: task.clone(),
            worktree: worktree.clone(),
            pid: None,
            phase: "starting".to_string(),
            last_activity: chrono::Utc::now(),
            error_count: 0,
            retry_count: 0,
        };

        self.sessions.lock().await.push(session);

        // Emit spawn event
        self.spine.emit(WakeyEvent::AgentSpawned {
            agent_type: agent_type.to_string(),
            task,
            worktree: worktree.map(|p| p.display().to_string()),
        });

        info!(session_id = %session_id, "Agent session spawned");
        session_id
    }

    /// Run the supervisor loop (call from tokio::spawn)
    pub async fn run(self) {
        info!("Agent supervisor started");

        // Subscribe to watcher events
        let mut events_rx = self.watcher.subscribe();

        loop {
            tokio::select! {
                // Handle filesystem events from watcher
                event = events_rx.recv() => {
                    match event {
                        Ok(watcher_event) => {
                            self.handle_watcher_event(watcher_event).await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            info!("Watcher channel closed, stopping supervisor");
                            break;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("Watcher channel lagged by {} events", n);
                        }
                    }
                }

                // Periodic stuck detection check
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    self.check_for_stuck_agents().await;
                }
            }
        }
    }

    /// Handle an event from the watcher
    async fn handle_watcher_event(&self, event: watcher::WatcherEvent) {
        match event {
            watcher::WatcherEvent::StateChanged {
                session_id,
                phase,
                detail,
            } => {
                self.update_session_phase(&session_id, &phase, &detail)
                    .await;
            }
            watcher::WatcherEvent::ErrorDetected {
                session_id,
                error,
                pattern,
            } => {
                self.handle_agent_error(&session_id, &error, &pattern).await;
            }
            watcher::WatcherEvent::ToolCall {
                session_id,
                tool,
                args_hash,
            } => {
                self.record_tool_call(&session_id, &tool, args_hash).await;
            }
            watcher::WatcherEvent::Timeout { session_id } => {
                self.handle_timeout(&session_id).await;
            }
        }
    }

    /// Update session phase
    async fn update_session_phase(&self, session_id: &str, phase: &str, detail: &str) {
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) {
            session.phase = phase.to_string();
            session.last_activity = chrono::Utc::now();

            self.spine.emit(WakeyEvent::AgentProgress {
                agent_type: session.agent_type.to_string(),
                phase: phase.to_string(),
                detail: detail.to_string(),
            });
        }
    }

    /// Record a tool call for loop detection
    async fn record_tool_call(&self, session_id: &str, tool: &str, args_hash: u64) {
        let mut detector = self.detector.lock().await;
        let result = detector.record(session_id, tool, args_hash);

        match result {
            LoopDetectionResult::Warning(msg) => {
                self.reporter.warn(session_id, &msg).await;
            }
            LoopDetectionResult::Block(msg) => {
                self.reporter.blocked(session_id, &msg).await;
            }
            LoopDetectionResult::Break(msg) => {
                self.handle_stuck(session_id, StuckReason::LoopDetected(msg))
                    .await;
            }
            LoopDetectionResult::Ok => {}
        }
    }

    /// Handle an agent error
    async fn handle_agent_error(&self, session_id: &str, error: &str, pattern: &str) {
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.iter_mut().find(|s| s.id == session_id) {
            session.error_count += 1;
            let auto_fixable = self.fixer.is_auto_fixable(pattern);
            let agent_type_str = session.agent_type.to_string();
            let retry_count = session.retry_count;

            self.spine.emit(WakeyEvent::AgentError {
                agent_type: agent_type_str.clone(),
                error: error.to_string(),
                auto_fixable,
            });

            // Attempt auto-fix if possible
            if auto_fixable && retry_count < self.config.max_auto_retries {
                drop(sessions); // Release lock before fix
                self.attempt_fix(session_id, pattern, error).await;
            } else {
                // Escalate to Tier 2
                drop(sessions);
                self.reporter
                    .needs_input(session_id, &agent_type_str, error)
                    .await;
            }
        }
    }

    /// Handle timeout (no activity)
    async fn handle_timeout(&self, session_id: &str) {
        self.handle_stuck(session_id, StuckReason::NoActivity).await;
    }

    /// Handle stuck agent
    async fn handle_stuck(&self, session_id: &str, reason: StuckReason) {
        let sessions = self.sessions.lock().await;

        if let Some(session) = sessions.iter().find(|s| s.id == session_id) {
            let duration = chrono::Utc::now()
                .signed_duration_since(session.last_activity)
                .num_seconds() as u64;

            let agent_type = session.agent_type.to_string();
            drop(sessions);

            self.spine.emit(WakeyEvent::AgentStuck {
                agent_type: agent_type.clone(),
                reason: reason.to_string(),
                duration_secs: duration,
            });

            // Always escalate stuck to reporter
            self.reporter.stuck(session_id, &agent_type, &reason).await;
        }
    }

    /// Attempt an auto-fix
    async fn attempt_fix(&self, session_id: &str, pattern: &str, _error: &str) {
        if let Some(action) = self.fixer.get_fix_action(pattern) {
            match self.fixer.execute_fix(&action).await {
                FixResult::Success(msg) => {
                    self.spine.emit(WakeyEvent::AgentFixed {
                        agent_type: "GSD".to_string(), // TODO: get from session
                        fix: msg,
                    });
                }
                FixResult::Failed(msg) => {
                    self.reporter.fix_failed(session_id, &msg).await;
                }
                FixResult::NeedsApproval => {
                    self.reporter.needs_approval(session_id, &action).await;
                }
            }
        }
    }

    /// Periodic check for stuck agents
    async fn check_for_stuck_agents(&self) {
        let mut sessions = self.sessions.lock().await;
        let now = chrono::Utc::now();
        let timeout = chrono::Duration::seconds(self.config.activity_timeout_secs as i64);

        for session in sessions.iter_mut() {
            let idle_time = now.signed_duration_since(session.last_activity);

            if idle_time > timeout && session.phase != "completed" && session.phase != "failed" {
                tracing::warn!(
                    session_id = %session.id,
                    idle_secs = idle_time.num_seconds(),
                    "Agent idle for too long"
                );

                self.spine.emit(WakeyEvent::AgentStuck {
                    agent_type: session.agent_type.to_string(),
                    reason: "No activity".to_string(),
                    duration_secs: idle_time.num_seconds() as u64,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_display() {
        assert_eq!(AgentType::Gsd.to_string(), "GSD");
        assert_eq!(AgentType::ClaudeCode.to_string(), "Claude Code");
    }

    #[test]
    fn test_default_config() {
        let config = SupervisorConfig::default();
        assert_eq!(config.activity_timeout_secs, 300);
        assert_eq!(config.loop_warning_threshold, 2);
        assert_eq!(config.max_auto_retries, 3);
    }
}
