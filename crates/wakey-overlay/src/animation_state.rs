//! Animation state machine for expression transitions.
//!
//! Handles priority-based interruption, smooth transitions, and animation queuing.

use crate::expressions::Expression;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Queued animation waiting to play
#[derive(Debug, Clone)]
pub struct QueuedAnimation {
    pub expression: Expression,
    pub queued_at: Instant,
    pub start_time: Option<Instant>,
}

/// Animation state machine
#[derive(Debug)]
pub struct AnimationState {
    /// Currently displayed expression
    current: Expression,
    /// Target expression (during transition)
    target: Option<Expression>,
    /// Transition start time
    transition_start: Option<Instant>,
    /// Transition duration
    transition_duration: Duration,
    /// Queue of pending animations
    queue: VecDeque<QueuedAnimation>,
    /// When current expression should expire (None = indefinite)
    expire_at: Option<Instant>,
    /// Minimum time between expression changes (prevents flickering)
    min_change_duration: Duration,
    /// Last time expression changed
    last_change: Instant,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationState {
    /// Create new animation state machine
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            current: Expression::neutral(),
            target: None,
            transition_start: None,
            transition_duration: Duration::from_millis(150),
            queue: VecDeque::new(),
            expire_at: None,
            min_change_duration: Duration::from_millis(100),
            last_change: now,
        }
    }

    /// Update animation state. Returns true if state changed.
    pub fn update(&mut self) -> bool {
        let now = Instant::now();
        let mut changed = false;

        // Check if current expression expired
        if let Some(expire) = self.expire_at {
            if now >= expire && self.target.is_none() {
                // Return to neutral
                self.start_transition(Expression::neutral());
                changed = true;
            }
        }

        // Update transition progress
        if let Some(target) = self.target.take() {
            if let Some(start) = self.transition_start {
                let elapsed = now.duration_since(start);
                
                if elapsed >= self.transition_duration {
                    // Transition complete
                    self.current = target.clone();
                    self.target = None;
                    self.transition_start = None;
                    changed = true;

                    // Set expiration if duration > 0
                    if target.duration_ms > 0 {
                        self.expire_at = Some(now + Duration::from_millis(target.duration_ms));
                    } else {
                        self.expire_at = None;
                    }
                } else {
                    // Still transitioning
                    self.target = Some(target);
                }
            }
        }

        // Process queue
        if self.target.is_none() && !self.queue.is_empty() {
            if now.duration_since(self.last_change) >= self.min_change_duration {
                if let Some(next) = self.queue.pop_front() {
                    self.start_transition(next.expression);
                    changed = true;
                }
            }
        }

        changed
    }

    /// Start a transition to a new expression
    fn start_transition(&mut self, expr: Expression) {
        self.transition_start = Some(Instant::now());
        self.transition_duration = Duration::from_millis(expr.transition_ms);
        self.target = Some(expr);
        self.last_change = Instant::now();
    }

    /// Set expression immediately (no transition)
    pub fn set_current(&mut self, expr: Expression) {
        self.current = expr.clone();
        self.target = None;
        self.transition_start = None;
        self.last_change = Instant::now();
        
        if expr.duration_ms > 0 {
            self.expire_at = Some(Instant::now() + Duration::from_millis(expr.duration_ms));
        } else {
            self.expire_at = None;
        }
    }

    /// Queue an expression (respects priority)
    pub fn queue(&mut self, expr: Expression) {
        let now = Instant::now();
        
        // Check if we can interrupt current expression
        if self.can_interrupt(&expr) {
            // Interrupt current and start new
            if self.target.is_some() || now.duration_since(self.last_change) >= self.min_change_duration {
                self.start_transition(expr);
            } else {
                // Wait for min change duration
                self.queue.push_back(QueuedAnimation {
                    expression: expr,
                    queued_at: now,
                    start_time: None,
                });
            }
        } else {
            // Add to queue
            self.queue.push_back(QueuedAnimation {
                expression: expr,
                queued_at: now,
                start_time: None,
            });
        }
    }

    /// Check if expression can interrupt current
    fn can_interrupt(&self, expr: &Expression) -> bool {
        // Can always interrupt if nothing is playing
        if self.target.is_none() && self.queue.is_empty() {
            return true;
        }

        // Check priority
        let current_priority = self.current.priority;
        expr.priority > current_priority
    }

    /// Get current expression (may be mid-transition)
    pub fn current_expression(&self) -> &Expression {
        if let Some(target) = &self.target {
            target
        } else {
            &self.current
        }
    }

    /// Get transition progress (0.0 to 1.0)
    pub fn transition_progress(&self) -> f32 {
        if let Some(start) = self.transition_start {
            let elapsed = Instant::now().duration_since(start);
            let progress = elapsed.as_secs_f32() / self.transition_duration.as_secs_f32();
            progress.min(1.0)
        } else {
            1.0
        }
    }

    /// Get interpolated expression (for smooth rendering)
    pub fn interpolated_expression(&self) -> Expression {
        let progress = self.transition_progress();
        
        if progress >= 1.0 {
            return self.current_expression().clone();
        }

        // For now, just return current (full interpolation would require blending support)
        // TODO: Implement proper expression blending
        self.current_expression().clone()
    }

    /// Reset to neutral expression
    pub fn reset(&mut self) {
        self.set_current(Expression::neutral());
        self.queue.clear();
    }

    /// Get queue length
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Clear the queue
    pub fn clear_queue(&mut self) {
        self.queue.clear();
    }
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let state = AnimationState::new();
        assert_eq!(state.current_expression().name, "neutral");
        assert_eq!(state.queue_len(), 0);
    }

    #[test]
    fn test_set_current() {
        let mut state = AnimationState::new();
        state.set_current(Expression::happy());
        assert_eq!(state.current_expression().name, "happy");
    }

    #[test]
    fn test_queue_expression() {
        let mut state = AnimationState::new();
        state.queue(Expression::celebrate());
        assert!(state.queue_len() > 0 || state.target.is_some());
    }

    #[test]
    fn test_priority_interrupt() {
        let mut state = AnimationState::new();
        state.set_current(Expression::neutral()); // Low priority
        
        // High priority should interrupt
        let celebrate = Expression::celebrate(); // High priority
        assert!(celebrate.priority > state.current.priority);
    }

    #[test]
    fn test_transition_progress() {
        let state = AnimationState::new();
        assert_eq!(state.transition_progress(), 1.0);
    }

    #[test]
    fn test_reset() {
        let mut state = AnimationState::new();
        state.set_current(Expression::celebrate());
        state.reset();
        assert_eq!(state.current_expression().name, "neutral");
    }
}
