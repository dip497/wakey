//! Reporter — Tier 2 notifications via spine events
//!
//! Handles communication with the human when agent issues
//! can't be auto-fixed. Uses ShouldSpeak events for
//! conversational (not technical) messaging.

use wakey_spine::Spine;
use wakey_types::WakeyEvent;
use wakey_types::event::Urgency;

use crate::agent_supervisor::detector::StuckReason;
use crate::agent_supervisor::fixer::FixAction;

/// Reporter for Tier 2 notifications
pub struct Reporter {
    spine: Spine,
}

impl Reporter {
    /// Create a new reporter
    pub fn new(spine: Spine) -> Self {
        Self { spine }
    }

    /// Emit a warning about potential loop
    pub async fn warn(&self, session_id: &str, message: &str) {
        debug_log(session_id, "warning", message);

        // Don't bother the human with warnings yet
        // Just log for observability
    }

    /// Emit a blocked notification
    pub async fn blocked(&self, session_id: &str, message: &str) {
        debug_log(session_id, "blocked", message);

        // Still not human-level yet, but getting closer
        self.spine.emit(WakeyEvent::AgentProgress {
            agent_type: "GSD".into(),
            phase: "blocked".into(),
            detail: message.to_string(),
        });
    }

    /// Agent is stuck — notify human
    pub async fn stuck(&self, _session_id: &str, agent_type: &str, reason: &StuckReason) {
        let message = match reason {
            StuckReason::NoActivity => {
                format!(
                    "Your {} worker hasn't made progress in a while. Want me to check on it?",
                    agent_type
                )
            }
            StuckReason::LoopDetected(tool) => {
                format!(
                    "Your {} worker seems stuck doing the same thing over and over ({}). Should I restart it?",
                    agent_type, tool
                )
            }
            StuckReason::StateRegression => {
                format!(
                    "Your {} worker hasn't moved forward in a while. Want me to investigate?",
                    agent_type
                )
            }
            StuckReason::ErrorThreshold => {
                format!(
                    "Your {} worker keeps hitting errors. I've tried fixing it a few times. Your call.",
                    agent_type
                )
            }
            StuckReason::External(what) => {
                format!(
                    "Something went wrong with your {} worker: {}. Want me to restart it?",
                    agent_type, what
                )
            }
        };

        self.speak(&message, Urgency::Medium);
    }

    /// Agent needs user input
    pub async fn needs_input(&self, _session_id: &str, agent_type: &str, issue: &str) {
        let message = format!(
            "Your {} worker needs your input: {}",
            agent_type,
            summarize_issue(issue)
        );

        self.speak(&message, Urgency::High);
    }

    /// A fix was attempted but failed
    pub async fn fix_failed(&self, _session_id: &str, error: &str) {
        let message = format!(
            "I tried to fix the issue but it didn't work: {}. Want me to try something else?",
            summarize_issue(error)
        );

        self.speak(&message, Urgency::Medium);
    }

    /// A fix needs user approval
    pub async fn needs_approval(&self, _session_id: &str, action: &FixAction) {
        let message = match action {
            FixAction::InstallCrate { crate_name, .. } => {
                format!(
                    "Your worker needs a new dependency: {}. Should I add it?",
                    crate_name
                )
            }
            FixAction::RunCommand { command, .. } => {
                format!(
                    "I want to run this command to fix the issue: {}. Is that okay?",
                    command
                )
            }
            FixAction::RestartWithContext { context: _ } => {
                "I think restarting with some extra context might help. Mind if I do that?"
                    .to_string()
            }
            FixAction::CheckEnvVar { var_name } => {
                format!(
                    "Your worker is missing an environment variable: {}. Can you set it?",
                    var_name
                )
            }
            _ => {
                "I have a potential fix but want to check with you first. Should I proceed?".into()
            }
        };

        self.speak(&message, Urgency::Medium);
    }

    /// Agent completed successfully
    pub async fn completed(&self, _session_id: &str, agent_type: &str, summary: &str) {
        let message = format!("Your {} worker finished! {}", agent_type, summary);

        self.speak(&message, Urgency::Low);
    }

    /// Agent failed after retries
    pub async fn failed(&self, _session_id: &str, agent_type: &str, reason: &str) {
        let message = format!(
            "Your {} worker couldn't finish the task: {}. Want me to try a different approach?",
            agent_type,
            summarize_issue(reason)
        );

        self.speak(&message, Urgency::High);
    }

    /// Emit a ShouldSpeak event
    fn speak(&self, message: &str, urgency: Urgency) {
        self.spine.emit(WakeyEvent::ShouldSpeak {
            reason: "agent_supervisor".into(),
            urgency,
            suggested_text: Some(message.to_string()),
        });
    }
}

/// Summarize an issue for human consumption
fn summarize_issue(issue: &str) -> String {
    // Truncate long errors
    if issue.len() > 200 {
        format!("{}...", issue.chars().take(197).collect::<String>())
    } else {
        issue.to_string()
    }
}

/// Debug logging helper
fn debug_log(session_id: &str, event_type: &str, message: &str) {
    tracing::debug!(
        session_id = %session_id,
        event_type = %event_type,
        message = %message
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_issue() {
        let short = "Error: something went wrong";
        assert_eq!(summarize_issue(short), short);

        let long = "x".repeat(300);
        let summarized = summarize_issue(&long);
        assert!(summarized.len() <= 200);
        assert!(summarized.ends_with("..."));
    }
}
