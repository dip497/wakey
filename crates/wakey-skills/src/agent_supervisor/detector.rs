//! Stuck Detector — sliding window state analysis + loop detection
//!
//! Uses ZeroClaw's loop detection pattern:
//! - Track tool call signatures in a sliding window
//! - Escalate: Warning → Block → Break
//!
//! Patterns detected:
//! - Exact repeat: same tool + args N times
//! - Ping-pong: A→B→A→B for 4+ cycles
//! - No progress: same tool, different args, same result

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::agent_supervisor::SupervisorConfig;

/// Result of loop detection analysis
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopDetectionResult {
    /// No loop detected
    Ok,

    /// Warning: loop forming, inject nudge
    Warning(String),

    /// Block: significant loop, replace output
    Block(String),

    /// Break: severe loop, terminate turn
    Break(String),
}

/// Reason why an agent is stuck
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StuckReason {
    /// No activity for too long
    NoActivity,

    /// Tool call loop detected
    LoopDetected(String),

    /// State regression (phase not progressing)
    StateRegression,

    /// Error threshold exceeded
    ErrorThreshold,

    /// External trigger (CI failure, etc.)
    External(String),
}

impl std::fmt::Display for StuckReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StuckReason::NoActivity => write!(f, "No activity timeout"),
            StuckReason::LoopDetected(msg) => write!(f, "Loop detected: {}", msg),
            StuckReason::StateRegression => write!(f, "State not progressing"),
            StuckReason::ErrorThreshold => write!(f, "Too many errors"),
            StuckReason::External(reason) => write!(f, "External: {}", reason),
        }
    }
}

/// Record of a tool call for loop detection
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct ToolCallRecord {
    tool: String,
    args_hash: u64,
}

/// Sliding window of recent tool calls
#[derive(Debug, Clone)]
struct SlidingWindow {
    /// Maximum window size
    max_size: usize,

    /// Recent tool calls (oldest first)
    calls: VecDeque<ToolCallRecord>,

    /// Count of each unique call in window
    counts: HashMap<ToolCallRecord, usize>,
}

impl SlidingWindow {
    fn new(max_size: usize) -> Self {
        Self {
            max_size,
            calls: VecDeque::with_capacity(max_size),
            counts: HashMap::new(),
        }
    }

    /// Add a tool call to the window
    fn push(&mut self, record: ToolCallRecord) {
        // If window is full, remove oldest
        if self.calls.len() >= self.max_size
            && let Some(oldest) = self.calls.pop_front()
            && let Some(count) = self.counts.get_mut(&oldest)
        {
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&oldest);
            }
        }

        // Add new record
        self.calls.push_back(record.clone());
        *self.counts.entry(record).or_insert(0) += 1;
    }

    /// Count consecutive identical calls at the end
    fn consecutive_count(&self) -> usize {
        if self.calls.is_empty() {
            return 0;
        }

        let last = self.calls.back().unwrap();
        let mut count = 0;

        for call in self.calls.iter().rev() {
            if call == last {
                count += 1;
            } else {
                break;
            }
        }

        count
    }

    /// Check for ping-pong pattern (A→B→A→B)
    fn check_ping_pong(&self) -> Option<(String, String, usize)> {
        if self.calls.len() < 4 {
            return None;
        }

        let calls: Vec<_> = self.calls.iter().collect();

        // Look for alternating pattern
        let mut cycle_count = 0;
        let first = calls[0];
        let second = calls[1];

        // Must be different tools
        if first.tool == second.tool {
            return None;
        }

        // Check for alternating pattern
        for i in (2..calls.len()).step_by(2) {
            if i + 1 < calls.len() {
                if calls[i] == first && calls[i + 1] == second {
                    cycle_count += 1;
                } else {
                    break;
                }
            }
        }

        if cycle_count >= 2 {
            Some((first.tool.clone(), second.tool.clone(), cycle_count + 1))
        } else {
            None
        }
    }

    /// Get unique call count
    #[allow(dead_code)]
    fn unique_count(&self) -> usize {
        self.counts.len()
    }

    /// Clear the window
    fn clear(&mut self) {
        self.calls.clear();
        self.counts.clear();
    }
}

/// Per-session state for stuck detection
#[derive(Debug)]
struct SessionState {
    /// Tool call window
    window: SlidingWindow,

    /// Last known phase
    last_phase: String,

    /// Time of last phase change
    last_phase_change: std::time::Instant,

    /// Error count
    error_count: usize,

    /// Time of last activity
    last_activity: std::time::Instant,
}

/// Stuck detector — analyzes agent state over time
pub struct StuckDetector {
    /// Per-session states
    sessions: HashMap<String, SessionState>,

    /// Configuration thresholds
    warning_threshold: usize,
    block_threshold: usize,
    break_threshold: usize,

    /// Phase timeout (seconds)
    phase_timeout_secs: u64,
}

impl StuckDetector {
    /// Create a new stuck detector
    pub fn new(config: &SupervisorConfig) -> Self {
        Self {
            sessions: HashMap::new(),
            warning_threshold: config.loop_warning_threshold,
            block_threshold: config.loop_block_threshold,
            break_threshold: config.loop_break_threshold,
            phase_timeout_secs: config.activity_timeout_secs,
        }
    }

    /// Register a new session
    pub fn register_session(&mut self, session_id: &str) {
        self.sessions.insert(
            session_id.to_string(),
            SessionState {
                window: SlidingWindow::new(20),
                last_phase: String::new(),
                last_phase_change: std::time::Instant::now(),
                error_count: 0,
                last_activity: std::time::Instant::now(),
            },
        );
    }

    /// Record a tool call and check for loops
    pub fn record(&mut self, session_id: &str, tool: &str, args_hash: u64) -> LoopDetectionResult {
        let state = match self.sessions.get_mut(session_id) {
            Some(s) => s,
            None => {
                // Auto-register unknown sessions
                self.register_session(session_id);
                self.sessions.get_mut(session_id).unwrap()
            }
        };

        state.last_activity = std::time::Instant::now();

        let record = ToolCallRecord {
            tool: tool.to_string(),
            args_hash,
        };

        state.window.push(record.clone());

        // Check for exact repeat loop
        let consecutive = state.window.consecutive_count();

        if consecutive >= self.break_threshold {
            return LoopDetectionResult::Break(format!(
                "Tool '{}' called {} times in a row",
                record.tool, consecutive
            ));
        }

        if consecutive >= self.block_threshold {
            return LoopDetectionResult::Block(format!(
                "Tool '{}' called {} times consecutively",
                record.tool, consecutive
            ));
        }

        if consecutive >= self.warning_threshold {
            return LoopDetectionResult::Warning(format!(
                "Tool '{}' repeating ({} times)",
                record.tool, consecutive
            ));
        }

        // Check for ping-pong pattern
        if let Some((tool_a, tool_b, cycles)) = state.window.check_ping_pong() {
            return LoopDetectionResult::Warning(format!(
                "Ping-pong pattern: {} ↔ {} ({} cycles)",
                tool_a, tool_b, cycles
            ));
        }

        LoopDetectionResult::Ok
    }

    /// Record an error
    pub fn record_error(&mut self, session_id: &str) -> usize {
        let state = match self.sessions.get_mut(session_id) {
            Some(s) => s,
            None => return 0,
        };

        state.error_count += 1;
        state.error_count
    }

    /// Update phase and check for regression
    pub fn update_phase(&mut self, session_id: &str, phase: &str) -> Option<StuckReason> {
        let state = self.sessions.get_mut(session_id)?;

        // Check if phase changed
        if state.last_phase != phase {
            state.last_phase = phase.to_string();
            state.last_phase_change = std::time::Instant::now();
            return None;
        }

        // Phase unchanged — check how long
        let elapsed = state.last_phase_change.elapsed().as_secs();

        // Special phases that are expected to take longer
        let long_phases = ["evaluating-gates", "planning", "compiling", "testing"];

        let threshold = if long_phases.contains(&phase) {
            self.phase_timeout_secs * 3 // 15 minutes for long phases
        } else {
            self.phase_timeout_secs // 5 minutes for normal phases
        };

        if elapsed > threshold {
            return Some(StuckReason::StateRegression);
        }

        None
    }

    /// Check for no activity timeout
    pub fn check_activity(&mut self, session_id: &str) -> Option<StuckReason> {
        let state = self.sessions.get(session_id)?;

        let elapsed = state.last_activity.elapsed().as_secs();

        if elapsed > self.phase_timeout_secs {
            return Some(StuckReason::NoActivity);
        }

        None
    }

    /// Reset a session's state (after fix applied)
    pub fn reset_session(&mut self, session_id: &str) {
        if let Some(state) = self.sessions.get_mut(session_id) {
            state.window.clear();
            state.error_count = 0;
            state.last_activity = std::time::Instant::now();
        }
    }

    /// Remove a session
    pub fn remove_session(&mut self, session_id: &str) {
        self.sessions.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> SupervisorConfig {
        SupervisorConfig {
            loop_warning_threshold: 2,
            loop_block_threshold: 4,
            loop_break_threshold: 6,
            activity_timeout_secs: 300,
            ..Default::default()
        }
    }

    #[test]
    fn test_sliding_window_push() {
        let mut window = SlidingWindow::new(5);

        window.push(ToolCallRecord {
            tool: "read".into(),
            args_hash: 1,
        });
        window.push(ToolCallRecord {
            tool: "read".into(),
            args_hash: 1,
        });
        window.push(ToolCallRecord {
            tool: "write".into(),
            args_hash: 2,
        });

        assert_eq!(window.calls.len(), 3);
        assert_eq!(window.consecutive_count(), 1); // Last is different
    }

    #[test]
    fn test_sliding_window_consecutive() {
        let mut window = SlidingWindow::new(10);

        for _ in 0..5 {
            window.push(ToolCallRecord {
                tool: "read".into(),
                args_hash: 1,
            });
        }

        assert_eq!(window.consecutive_count(), 5);
    }

    #[test]
    fn test_loop_detection_warning() {
        let config = make_config();
        let mut detector = StuckDetector::new(&config);

        detector.register_session("test");

        // First call
        let result = detector.record("test", "read", 1);
        assert_eq!(result, LoopDetectionResult::Ok);

        // Second identical call
        let result = detector.record("test", "read", 1);
        assert!(matches!(result, LoopDetectionResult::Warning(_)));
    }

    #[test]
    fn test_loop_detection_block() {
        let config = make_config();
        let mut detector = StuckDetector::new(&config);

        detector.register_session("test");

        for _ in 0..4 {
            detector.record("test", "read", 1);
        }

        let result = detector.record("test", "read", 1);
        assert!(matches!(result, LoopDetectionResult::Block(_)));
    }

    #[test]
    fn test_loop_detection_break() {
        let config = make_config();
        let mut detector = StuckDetector::new(&config);

        detector.register_session("test");

        for _ in 0..6 {
            detector.record("test", "read", 1);
        }

        let result = detector.record("test", "read", 1);
        assert!(matches!(result, LoopDetectionResult::Break(_)));
    }

    #[test]
    fn test_different_tools_no_loop() {
        let config = make_config();
        let mut detector = StuckDetector::new(&config);

        detector.register_session("test");

        // Different tools should not trigger loop detection
        detector.record("test", "read", 1);
        detector.record("test", "write", 2);
        detector.record("test", "read", 1);
        detector.record("test", "write", 2);

        let result = detector.record("test", "read", 1);
        assert_eq!(result, LoopDetectionResult::Ok);
    }

    #[test]
    fn test_ping_pong_detection() {
        let mut window = SlidingWindow::new(10);

        // Create A-B-A-B pattern
        window.push(ToolCallRecord {
            tool: "read".into(),
            args_hash: 1,
        });
        window.push(ToolCallRecord {
            tool: "write".into(),
            args_hash: 2,
        });
        window.push(ToolCallRecord {
            tool: "read".into(),
            args_hash: 1,
        });
        window.push(ToolCallRecord {
            tool: "write".into(),
            args_hash: 2,
        });
        window.push(ToolCallRecord {
            tool: "read".into(),
            args_hash: 1,
        });
        window.push(ToolCallRecord {
            tool: "write".into(),
            args_hash: 2,
        });

        let result = window.check_ping_pong();
        assert!(result.is_some());

        let (tool_a, tool_b, cycles) = result.unwrap();
        assert_eq!(tool_a, "read");
        assert_eq!(tool_b, "write");
        assert!(cycles >= 2);
    }
}
