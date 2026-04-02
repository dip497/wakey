//! Chat bubble for displaying Wakey's speech.
//!
//! Rounded rectangle above the sprite with typewriter effect.
//! Auto-hides after 10 seconds.

use eframe::egui::{Color32, FontId, Pos2, Rect, Vec2};
use std::time::{Duration, Instant};

/// Chat bubble state.
#[derive(Debug)]
pub struct Bubble {
    /// Text to display
    text: String,
    /// How many characters are currently visible (typewriter effect)
    visible_chars: usize,
    /// When the typewriter started
    typewriter_start: Option<Instant>,
    /// When the bubble should auto-hide
    hide_after: Option<Instant>,
    /// Whether the bubble is currently visible
    visible: bool,
}

/// Configuration for bubble appearance.
pub struct BubbleConfig {
    /// Maximum width before wrapping
    pub max_width: f32,
    /// Padding inside the bubble
    pub padding: f32,
    /// Corner radius
    pub corner_radius: f32,
    /// Background color
    pub bg_color: Color32,
    /// Text color
    pub text_color: Color32,
    /// Characters per second for typewriter effect
    pub chars_per_sec: f32,
    /// Auto-hide delay
    pub auto_hide_delay: Duration,
}

impl Default for BubbleConfig {
    fn default() -> Self {
        Self {
            max_width: 250.0,
            padding: 12.0,
            corner_radius: 12.0,
            bg_color: Color32::from_rgba_unmultiplied(35, 30, 28, 230),
            text_color: Color32::from_rgb(240, 235, 220),
            chars_per_sec: 30.0, // Comfortable reading speed
            auto_hide_delay: Duration::from_secs(10),
        }
    }
}

impl Bubble {
    /// Create a new hidden bubble.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            visible_chars: 0,
            typewriter_start: None,
            hide_after: None,
            visible: false,
        }
    }

    /// Show a new message in the bubble.
    /// Starts the typewriter effect from the beginning.
    pub fn show(&mut self, text: &str, now: Instant) {
        self.text = text.to_string();
        self.visible_chars = 0;
        self.typewriter_start = Some(now);
        self.hide_after = Some(now + BubbleConfig::default().auto_hide_delay);
        self.visible = true;
    }

    /// Hide the bubble immediately.
    pub fn hide(&mut self) {
        self.visible = false;
        self.text.clear();
        self.visible_chars = 0;
        self.typewriter_start = None;
        self.hide_after = None;
    }

    /// Update bubble state (typewriter progress, auto-hide).
    /// Returns true if a redraw is needed.
    pub fn update(&mut self, now: Instant) -> bool {
        if !self.visible {
            return false;
        }

        let mut needs_redraw = false;

        // Update typewriter effect
        if self.visible_chars < self.text.len()
            && let Some(start) = self.typewriter_start
        {
            let elapsed = now.duration_since(start).as_secs_f32();
            let config = BubbleConfig::default();
            let target_chars = (elapsed * config.chars_per_sec) as usize;
            let new_chars = target_chars.min(self.text.len());

            if new_chars != self.visible_chars {
                self.visible_chars = new_chars;
                needs_redraw = true;
            }

            // Typewriter complete
            if self.visible_chars >= self.text.len() {
                self.typewriter_start = None;
            }
        }

        // Check auto-hide
        if let Some(hide_time) = self.hide_after
            && now >= hide_time
        {
            self.hide();
            needs_redraw = true;
        }

        needs_redraw
    }

    /// Check if the bubble is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Draw the bubble using egui's painter.
    pub fn draw(&self, painter: &eframe::egui::Painter, sprite_center: Pos2, sprite_size: f32) {
        if !self.visible || self.text.is_empty() {
            return;
        }

        let config = BubbleConfig::default();

        // Get visible text
        let visible_text = &self.text[..self.visible_chars.min(self.text.len())];

        // Calculate bubble position (above sprite)
        let bubble_gap = sprite_size * 0.8;
        let bubble_height_estimate = 60.0; // Approximate, will adjust

        // Position bubble above sprite, centered horizontally
        let bubble_center_y =
            sprite_center.y - sprite_size - bubble_gap - bubble_height_estimate / 2.0;

        // Create a rect for the bubble
        let bubble_rect = Rect::from_center_size(
            Pos2::new(sprite_center.x, bubble_center_y),
            Vec2::new(config.max_width, bubble_height_estimate),
        );

        // Draw rounded rectangle background
        painter.rect_filled(
            bubble_rect,
            eframe::egui::CornerRadius::from(config.corner_radius as u8),
            config.bg_color,
        );

        // Draw a subtle border
        painter.rect_stroke(
            bubble_rect,
            eframe::egui::CornerRadius::from(config.corner_radius as u8),
            eframe::egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(60, 55, 50, 180)),
            eframe::egui::StrokeKind::Outside,
        );

        // Draw text using galley (text layout)
        let font = FontId::proportional(14.0);
        let galley =
            painter.layout_no_wrap(visible_text.to_string(), font.clone(), config.text_color);

        // Center text in bubble
        let text_pos = Pos2::new(
            bubble_rect.center().x - galley.size().x / 2.0,
            bubble_rect.top() + config.padding,
        );

        // Draw galley (text)
        painter.galley(text_pos, galley, config.text_color);
    }
}

impl Default for Bubble {
    fn default() -> Self {
        Self::new()
    }
}
