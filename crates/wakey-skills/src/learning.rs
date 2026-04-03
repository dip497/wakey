//! Learning tracker — Hermes iteration tracking pattern
//!
//! Tracks tool iterations since last skill creation.
//! Triggers skill review after N iterations.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use tracing::debug;

/// Default number of iterations before skill review trigger
const DEFAULT_NUDGE_INTERVAL: u32 = 10;

/// Minimum iterations for complex task detection
const COMPLEX_TASK_THRESHOLD: u32 = 5;

/// Learning tracker state
#[derive(Debug)]
pub struct LearningTracker {
    /// Iterations since last skill creation/use
    iters_since_skill: Arc<AtomicU32>,

    /// Interval for triggering skill review
    nudge_interval: u32,

    /// Total iterations this session
    total_iterations: Arc<AtomicU32>,

    /// Errors encountered this session
    errors_encountered: Arc<AtomicU32>,

    /// User corrections this session
    user_corrections: Arc<AtomicU32>,
}

impl LearningTracker {
    /// Create a new learning tracker with default interval
    pub fn new() -> Self {
        Self::with_interval(DEFAULT_NUDGE_INTERVAL)
    }

    /// Create a new learning tracker with custom interval
    pub fn with_interval(nudge_interval: u32) -> Self {
        debug!(interval = nudge_interval, "Created learning tracker");
        Self {
            iters_since_skill: Arc::new(AtomicU32::new(0)),
            nudge_interval,
            total_iterations: Arc::new(AtomicU32::new(0)),
            errors_encountered: Arc::new(AtomicU32::new(0)),
            user_corrections: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Called on each tool invocation
    ///
    /// Increments iteration counters. Check should_nudge() after.
    pub fn on_tool_call(&self) {
        self.iters_since_skill.fetch_add(1, Ordering::Relaxed);
        self.total_iterations.fetch_add(1, Ordering::Relaxed);
    }

    /// Called when a skill is created or used
    ///
    /// Resets the iteration counter.
    pub fn on_skill_use(&self) {
        self.iters_since_skill.store(0, Ordering::Relaxed);
        debug!("Reset skill iteration counter");
    }

    /// Called when an error is encountered
    ///
    /// Errors can trigger skill creation even with fewer iterations.
    pub fn on_error(&self) {
        self.errors_encountered.fetch_add(1, Ordering::Relaxed);
    }

    /// Called when user corrects the agent's approach
    ///
    /// User corrections are strong signals for skill capture.
    pub fn on_user_correction(&self) {
        self.user_corrections.fetch_add(1, Ordering::Relaxed);
    }

    /// Check if skill review should be triggered
    ///
    /// Returns true when:
    /// - Iterations >= nudge_interval, OR
    /// - Errors >= 2 AND iterations >= 3, OR
    /// - User corrections >= 1 AND iterations >= 3
    pub fn should_nudge(&self) -> bool {
        let iters = self.iters_since_skill.load(Ordering::Relaxed);
        let errors = self.errors_encountered.load(Ordering::Relaxed);
        let corrections = self.user_corrections.load(Ordering::Relaxed);

        // Standard trigger: iteration count reached
        if iters >= self.nudge_interval {
            return true;
        }

        // Error trigger: multiple errors with some iterations
        if errors >= 2 && iters >= 3 {
            return true;
        }

        // Correction trigger: user corrected approach
        if corrections >= 1 && iters >= 3 {
            return true;
        }

        false
    }

    /// Consume the nudge (reset counters)
    ///
    /// Call after deciding whether to trigger skill review.
    /// Returns whether a nudge was pending.
    pub fn consume_nudge(&self) -> bool {
        let iters = self.iters_since_skill.swap(0, Ordering::Relaxed);
        let errors = self.errors_encountered.swap(0, Ordering::Relaxed);
        let corrections = self.user_corrections.swap(0, Ordering::Relaxed);

        iters >= self.nudge_interval
            || (errors >= 2 && iters >= 3)
            || (corrections >= 1 && iters >= 3)
    }

    /// Check if task was complex (5+ tool calls)
    pub fn is_complex_task(&self) -> bool {
        self.total_iterations.load(Ordering::Relaxed) >= COMPLEX_TASK_THRESHOLD
    }

    /// Get current iteration counts
    pub fn stats(&self) -> LearningStats {
        LearningStats {
            iters_since_skill: self.iters_since_skill.load(Ordering::Relaxed),
            total_iterations: self.total_iterations.load(Ordering::Relaxed),
            errors_encountered: self.errors_encountered.load(Ordering::Relaxed),
            user_corrections: self.user_corrections.load(Ordering::Relaxed),
            nudge_interval: self.nudge_interval,
        }
    }

    /// Reset all counters
    pub fn reset(&self) {
        self.iters_since_skill.store(0, Ordering::Relaxed);
        self.total_iterations.store(0, Ordering::Relaxed);
        self.errors_encountered.store(0, Ordering::Relaxed);
        self.user_corrections.store(0, Ordering::Relaxed);
        debug!("Reset all learning counters");
    }
}

impl Default for LearningTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Learning statistics snapshot
#[derive(Debug, Clone, Copy)]
pub struct LearningStats {
    /// Iterations since last skill creation
    pub iters_since_skill: u32,

    /// Total iterations this session
    pub total_iterations: u32,

    /// Errors encountered
    pub errors_encountered: u32,

    /// User corrections
    pub user_corrections: u32,

    /// Configured nudge interval
    pub nudge_interval: u32,
}

/// Learning trigger reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerReason {
    /// Iteration count reached
    IterationCount,

    /// Multiple errors encountered
    ErrorsOvercome,

    /// User corrected approach
    UserCorrection,
}

impl LearningTracker {
    /// Get the trigger reason (if should_nudge is true)
    pub fn trigger_reason(&self) -> Option<TriggerReason> {
        let iters = self.iters_since_skill.load(Ordering::Relaxed);
        let errors = self.errors_encountered.load(Ordering::Relaxed);
        let corrections = self.user_corrections.load(Ordering::Relaxed);

        if iters >= self.nudge_interval {
            return Some(TriggerReason::IterationCount);
        }

        if errors >= 2 && iters >= 3 {
            return Some(TriggerReason::ErrorsOvercome);
        }

        if corrections >= 1 && iters >= 3 {
            return Some(TriggerReason::UserCorrection);
        }

        None
    }
}

/// Skill review prompt builder
pub struct SkillReviewPrompt;

impl SkillReviewPrompt {
    /// Build the skill review prompt for background analysis
    ///
    /// Based on Hermes _SKILL_REVIEW_PROMPT
    pub fn build(trigger: TriggerReason, conversation_summary: &str) -> String {
        let trigger_context = match trigger {
            TriggerReason::IterationCount => "A complex task was completed after many tool calls.",
            TriggerReason::ErrorsOvercome => {
                "The task encountered errors but was eventually completed."
            }
            TriggerReason::UserCorrection => "The user corrected the approach during execution.",
        };

        format!(
            r#"Review the conversation and consider creating or updating a skill.

**Trigger**: {}
**Context**: {}

## Guidelines

1. **Create** a new skill if:
   - A non-trivial workflow was discovered
   - An approach required trial and error
   - The user expressed desire to "remember this"

2. **Update** an existing skill if:
   - Instructions were stale or wrong
   - OS-specific issues were encountered
   - Missing steps or pitfalls were found

3. **Skip** if nothing is worth saving

## Response Format

If creating/updating a skill, output:
```
SKILL_ACTION: create|update
SKILL_NAME: skill-name
SKILL_REASON: Brief explanation
```

If nothing to save, just say: "Nothing to save."
"#,
            trigger_context, conversation_summary
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tracker() {
        let tracker = LearningTracker::new();
        let stats = tracker.stats();

        assert_eq!(stats.iters_since_skill, 0);
        assert_eq!(stats.nudge_interval, DEFAULT_NUDGE_INTERVAL);
    }

    #[test]
    fn test_on_tool_call() {
        let tracker = LearningTracker::new();

        tracker.on_tool_call();
        tracker.on_tool_call();

        let stats = tracker.stats();
        assert_eq!(stats.iters_since_skill, 2);
        assert_eq!(stats.total_iterations, 2);
    }

    #[test]
    fn test_on_skill_use() {
        let tracker = LearningTracker::new();

        tracker.on_tool_call();
        tracker.on_tool_call();
        tracker.on_skill_use();

        let stats = tracker.stats();
        assert_eq!(stats.iters_since_skill, 0);
        assert_eq!(stats.total_iterations, 2);
    }

    #[test]
    fn test_should_nudge_iteration() {
        let tracker = LearningTracker::with_interval(5);

        for _ in 0..4 {
            tracker.on_tool_call();
        }
        assert!(!tracker.should_nudge());

        tracker.on_tool_call();
        assert!(tracker.should_nudge());
    }

    #[test]
    fn test_should_nudge_errors() {
        let tracker = LearningTracker::new();

        // Errors with insufficient iterations
        tracker.on_error();
        tracker.on_error();
        tracker.on_tool_call();
        tracker.on_tool_call();
        assert!(!tracker.should_nudge());

        // Errors with enough iterations
        tracker.on_tool_call();
        assert!(tracker.should_nudge());
    }

    #[test]
    fn test_should_nudge_correction() {
        let tracker = LearningTracker::new();

        // Correction with insufficient iterations
        tracker.on_user_correction();
        tracker.on_tool_call();
        tracker.on_tool_call();
        assert!(!tracker.should_nudge());

        // Correction with enough iterations
        tracker.on_tool_call();
        assert!(tracker.should_nudge());
    }

    #[test]
    fn test_consume_nudge() {
        let tracker = LearningTracker::with_interval(3);

        for _ in 0..3 {
            tracker.on_tool_call();
        }

        assert!(tracker.consume_nudge());

        // Counter should be reset
        let stats = tracker.stats();
        assert_eq!(stats.iters_since_skill, 0);
    }

    #[test]
    fn test_trigger_reason() {
        let tracker = LearningTracker::with_interval(5);

        // No trigger
        assert_eq!(tracker.trigger_reason(), None);

        // Iteration trigger
        for _ in 0..5 {
            tracker.on_tool_call();
        }
        assert_eq!(
            tracker.trigger_reason(),
            Some(TriggerReason::IterationCount)
        );

        tracker.reset();

        // Error trigger
        tracker.on_error();
        tracker.on_error();
        tracker.on_tool_call();
        tracker.on_tool_call();
        tracker.on_tool_call();
        assert_eq!(
            tracker.trigger_reason(),
            Some(TriggerReason::ErrorsOvercome)
        );

        tracker.reset();

        // Correction trigger
        tracker.on_user_correction();
        tracker.on_tool_call();
        tracker.on_tool_call();
        tracker.on_tool_call();
        assert_eq!(
            tracker.trigger_reason(),
            Some(TriggerReason::UserCorrection)
        );
    }

    #[test]
    fn test_is_complex_task() {
        let tracker = LearningTracker::new();

        for _ in 0..4 {
            tracker.on_tool_call();
        }
        assert!(!tracker.is_complex_task());

        tracker.on_tool_call();
        assert!(tracker.is_complex_task());
    }

    #[test]
    fn test_skill_review_prompt() {
        let prompt =
            SkillReviewPrompt::build(TriggerReason::IterationCount, "Deployed app to production");

        assert!(prompt.contains("complex task was completed"));
        assert!(prompt.contains("Deployed app"));
        assert!(prompt.contains("SKILL_ACTION"));
    }
}
