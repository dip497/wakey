//! Trigger system: Maps Wakey events to expressions.

use crate::expressions::Expression;
use wakey_types::{WakeyEvent, Mood};

/// Trigger type for expression changes
#[derive(Debug, Clone)]
pub enum ExpressionTrigger {
    /// Triggered by mood change (legacy)
    Mood(Mood),
    /// Triggered by specific Wakey event
    Event(WakeyEvent),
    /// Custom trigger by name
    Custom(String),
}

/// Maps triggers to expressions
#[derive(Debug)]
pub struct ExpressionMapper {
    custom_mappings: std::collections::HashMap<String, Expression>,
}

impl Default for ExpressionMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl ExpressionMapper {
    pub fn new() -> Self {
        Self {
            custom_mappings: std::collections::HashMap::new(),
        }
    }

    /// Register a custom trigger mapping
    pub fn register_custom(&mut self, name: String, expression: Expression) {
        self.custom_mappings.insert(name, expression);
    }

    /// Map a trigger to an expression
    pub fn map(&self, trigger: &ExpressionTrigger) -> Option<Expression> {
        match trigger {
            ExpressionTrigger::Mood(mood) => Some(Expression::from_mood(*mood)),
            ExpressionTrigger::Event(event) => Self::map_event(event),
            ExpressionTrigger::Custom(name) => self.custom_mappings.get(name).cloned(),
        }
    }

    /// Map WakeyEvent to expression
    fn map_event(event: &WakeyEvent) -> Option<Expression> {
        match event {
            WakeyEvent::AgentCompleted { .. } => Some(Expression::celebrate()),
            WakeyEvent::AgentFailed { .. } => Some(Expression::worried()),
            WakeyEvent::AgentError { .. } => Some(Expression::angry()),
            
            WakeyEvent::UserIdle { duration, .. } => {
                if duration.as_secs() > 300 {
                    Some(Expression::sleepy())
                } else if duration.as_secs() > 60 {
                    Some(Expression::thinking())
                } else {
                    None
                }
            }
            
            WakeyEvent::UserReturned { .. } => Some(Expression::happy()),
            
            WakeyEvent::SystemVitals { battery_percent, .. } => {
                if let Some(battery) = battery_percent {
                    if *battery < 10 {
                        return Some(Expression::worried());
                    } else if *battery == 100 {
                        return Some(Expression::happy());
                    }
                }
                None
            }
            
            WakeyEvent::NotificationReceived { .. } => Some(Expression::surprised()),
            WakeyEvent::VoiceUserSpeaking { is_final: true, .. } => Some(Expression::thinking()),
            WakeyEvent::VoiceWakeySpeaking { .. } => Some(Expression::happy()),
            WakeyEvent::SkillExtracted { .. } => Some(Expression::idea()),
            
            _ => None,
        }
    }

    pub fn default_expression(&self) -> Expression {
        Expression::neutral()
    }
}

/// Convert WakeyEvent to ExpressionTrigger
pub fn event_to_trigger(event: &WakeyEvent) -> Option<ExpressionTrigger> {
    match event {
        WakeyEvent::MoodChanged { to, .. } => Some(ExpressionTrigger::Mood(*to)),
        
        WakeyEvent::AgentCompleted { .. }
        | WakeyEvent::AgentFailed { .. }
        | WakeyEvent::AgentError { .. }
        | WakeyEvent::UserIdle { .. }
        | WakeyEvent::UserReturned { .. }
        | WakeyEvent::SystemVitals { .. }
        | WakeyEvent::NotificationReceived { .. }
        | WakeyEvent::VoiceUserSpeaking { .. }
        | WakeyEvent::VoiceWakeySpeaking { .. }
        | WakeyEvent::SkillExtracted { .. } => Some(ExpressionTrigger::Event(event.clone())),
        
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_mood_trigger() {
        let mapper = ExpressionMapper::new();
        let trigger = ExpressionTrigger::Mood(Mood::Happy);
        let expr = mapper.map(&trigger).unwrap();
        assert_eq!(expr.name, "happy");
    }

    #[test]
    fn test_agent_completed_trigger() {
        let event = WakeyEvent::AgentCompleted {
            agent_type: "test".to_string(),
            summary: "Done!".to_string(),
        };
        let trigger = event_to_trigger(&event).unwrap();
        let mapper = ExpressionMapper::new();
        let expr = mapper.map(&trigger).unwrap();
        assert_eq!(expr.name, "celebrate");
    }

    #[test]
    fn test_idle_trigger() {
        let event = WakeyEvent::UserIdle {
            duration: Duration::from_secs(400),
            timestamp: chrono::Utc::now(),
        };
        let trigger = event_to_trigger(&event).unwrap();
        let mapper = ExpressionMapper::new();
        let expr = mapper.map(&trigger).unwrap();
        assert_eq!(expr.name, "sleepy");
    }
}
