//! Animated sprite for the Wakey overlay.
//!
//! A simple breathing creature with eyes that blink occasionally.
//! Drawn using egui's painter (no sprite sheets for MVP).

use eframe::egui::{Color32, Pos2, Vec2};
use std::time::{Duration, Instant};

/// Animation state for the sprite.
#[derive(Debug)]
pub struct Sprite {
    /// Current animation phase (0.0 to 1.0 for sine wave)
    phase: f32,
    /// Time of last animation update
    last_update: Instant,
    /// Eye blink state (0.0 = closed, 1.0 = open)
    eye_state: f32,
    /// When the current blink started
    blink_start: Option<Instant>,
    /// Time until next random blink
    next_blink: Instant,
    /// Current expression (mood-based visual style)
    expression: Expression,
}

use crate::expressions::Expression;

impl Sprite {
    /// Create a new sprite with default neutral expression.
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            last_update: Instant::now(),
            eye_state: 1.0,
            blink_start: None,
            next_blink: Self::random_blink_time(),
            expression: Expression::default(),
        }
    }

    /// Update sprite animation state.
    /// Returns true if a redraw is needed.
    pub fn update(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_update);
        self.last_update = now;

        // Update breathing phase
        let breath_period = 3.0 / self.expression.anim_speed; // seconds per breath cycle
        let phase_delta = elapsed.as_secs_f32() / breath_period;
        self.phase = (self.phase + phase_delta) % 1.0;

        // Handle blinking
        self.update_blink(now)
    }

    /// Update blink animation.
    fn update_blink(&mut self, now: Instant) -> bool {
        // Check if it's time to start a new blink
        if self.blink_start.is_none() && now >= self.next_blink {
            self.blink_start = Some(now);
            self.next_blink = Self::random_blink_time();
            return true;
        }

        // Update ongoing blink
        if let Some(blink_start) = self.blink_start {
            let blink_duration = Duration::from_millis(150); // Quick blink
            let elapsed = now.duration_since(blink_start);

            if elapsed < blink_duration {
                // Blink in progress: animate eye closing then opening
                let t = elapsed.as_secs_f32() / blink_duration.as_secs_f32();
                // Close for first half, open for second half
                self.eye_state = if t < 0.5 {
                    1.0 - (t * 2.0)
                } else {
                    (t - 0.5) * 2.0
                };
                // Apply expression's base eye openness
                self.eye_state *= self.expression.eye_openness;
                return true;
            } else {
                // Blink finished
                self.blink_start = None;
                self.eye_state = self.expression.eye_openness;
                return true;
            }
        }

        false
    }

    /// Generate a random time for the next blink (2-8 seconds).
    fn random_blink_time() -> Instant {
        let delay_ms: u64 = 2000 + (rand_simple() % 6000) as u64;
        Instant::now() + Duration::from_millis(delay_ms)
    }

    /// Set the current expression from a mood.
    pub fn set_expression(&mut self, expression: Expression) {
        self.expression = expression;
        self.eye_state = expression.eye_openness;
    }

    /// Draw the sprite using egui's painter.
    pub fn draw(&self, painter: &eframe::egui::Painter, center: Pos2, base_size: f32) {
        // Calculate breathing scale using sine wave
        let breath_amplitude = 0.12; // 12% size variation
        let breath_scale = 1.0 + breath_amplitude * (self.phase * 2.0 * std::f32::consts::PI).sin();

        let size = base_size * breath_scale;

        // Convert expression color to Color32
        let glow_color = Color32::from_rgba_unmultiplied(
            (self.expression.glow_color[0] * 255.0) as u8,
            (self.expression.glow_color[1] * 255.0) as u8,
            (self.expression.glow_color[2] * 255.0) as u8,
            (self.expression.glow_color[3] * 255.0) as u8,
        );

        // Draw outer glow (larger, more transparent)
        let glow_size = size * 1.3;
        painter.circle_filled(
            center,
            glow_size,
            Color32::from_rgba_unmultiplied(
                glow_color.r(),
                glow_color.g(),
                glow_color.b(),
                (glow_color.a() as f32 * 0.3) as u8,
            ),
        );

        // Draw main body
        painter.circle_filled(center, size, glow_color);

        // Draw inner highlight (smaller, brighter)
        let highlight_offset = Vec2::new(-size * 0.15, -size * 0.15);
        painter.circle_filled(
            center + highlight_offset,
            size * 0.4,
            Color32::from_rgba_unmultiplied(255, 255, 240, 60),
        );

        // Draw eyes
        let eye_offset_x = size * 0.35;
        let eye_offset_y = -size * 0.15;
        let eye_spacing = size * 0.25;
        let eye_size = size * 0.15 * self.eye_state;

        // Only draw eyes if they're at least partially open
        if self.eye_state > 0.05 {
            let left_eye_center = center + Vec2::new(-eye_spacing + eye_offset_x, eye_offset_y);
            let right_eye_center = center + Vec2::new(eye_spacing + eye_offset_x, eye_offset_y);

            // Eyes are dark dots
            let eye_color = Color32::from_rgb(40, 35, 30);
            painter.circle_filled(left_eye_center, eye_size, eye_color);
            painter.circle_filled(right_eye_center, eye_size, eye_color);

            // Add tiny highlight dots in eyes
            if self.eye_state > 0.3 {
                let eye_highlight_size = eye_size * 0.35;
                let eye_highlight_offset = Vec2::new(-eye_size * 0.25, -eye_size * 0.25);
                painter.circle_filled(
                    left_eye_center + eye_highlight_offset,
                    eye_highlight_size,
                    Color32::from_rgb(80, 75, 65),
                );
                painter.circle_filled(
                    right_eye_center + eye_highlight_offset,
                    eye_highlight_size,
                    Color32::from_rgb(80, 75, 65),
                );
            }
        }

        // Draw sleepy "droopy" eyelids if expression is sleepy
        if self.expression.sleepy && self.eye_state > 0.1 {
            let eyelid_color = Color32::from_rgba_unmultiplied(
                glow_color.r(),
                glow_color.g(),
                glow_color.b(),
                200,
            );
            // Draw partial arcs over eyes
            let eyelid_y_offset = -size * 0.05;
            painter.circle_filled(
                center + Vec2::new(-eye_spacing + eye_offset_x, eye_offset_y + eyelid_y_offset),
                eye_size * 0.6,
                eyelid_color,
            );
            painter.circle_filled(
                center + Vec2::new(eye_spacing + eye_offset_x, eye_offset_y + eyelid_y_offset),
                eye_size * 0.6,
                eyelid_color,
            );
        }
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple pseudo-random number generator for blink timing.
/// Uses a static counter for determinism without full RNG dependency.
fn rand_simple() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    // Simple hash-like transformation
    (val * 1103515245 + 12345) % 6000
}
