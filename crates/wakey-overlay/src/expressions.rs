//! Expression states for the Wakey sprite.
//!
//! Maps Wakey's Mood to visual properties like color, animation speed, and eye state.

use wakey_types::event::Mood;

/// Visual expression derived from Mood.
#[derive(Debug, Clone, Copy)]
pub struct Expression {
    /// Primary glow color (RGBA)
    pub glow_color: [f32; 4],
    /// Animation speed multiplier (1.0 = normal breathing)
    pub anim_speed: f32,
    /// Eye openness (0.0 = closed/blinking, 1.0 = fully open)
    pub eye_openness: f32,
    /// Whether to show "sleepy" droopy eyes
    pub sleepy: bool,
    /// Whether to show a happy bounce effect
    pub bounce: bool,
}

impl Expression {
    /// Create expression from a Mood enum.
    pub fn from_mood(mood: Mood) -> Self {
        match mood {
            Mood::Neutral => Self {
                glow_color: [0.95, 0.65, 0.25, 0.85], // Warm amber
                anim_speed: 1.0,
                eye_openness: 1.0,
                sleepy: false,
                bounce: false,
            },
            Mood::Happy => Self {
                glow_color: [0.95, 0.75, 0.35, 0.95], // Brighter amber
                anim_speed: 1.4,
                eye_openness: 1.0,
                sleepy: false,
                bounce: true,
            },
            Mood::Empathetic => Self {
                glow_color: [0.85, 0.55, 0.35, 0.80], // Softer amber
                anim_speed: 0.85,
                eye_openness: 0.9,
                sleepy: false,
                bounce: false,
            },
            Mood::Focused => Self {
                glow_color: [0.30, 0.50, 0.85, 0.75], // Cool blue (coding vibe)
                anim_speed: 0.7,
                eye_openness: 1.0,
                sleepy: false,
                bounce: false,
            },
            Mood::Playful => Self {
                glow_color: [0.70, 0.50, 0.90, 0.90], // Playful purple
                anim_speed: 1.6,
                eye_openness: 1.0,
                sleepy: false,
                bounce: true,
            },
            Mood::Concerned => Self {
                glow_color: [0.85, 0.35, 0.35, 0.80], // Concerned reddish
                anim_speed: 0.8,
                eye_openness: 0.85,
                sleepy: false,
                bounce: false,
            },
            Mood::Sleepy => Self {
                glow_color: [0.60, 0.45, 0.25, 0.50], // Dim amber
                anim_speed: 0.4,
                eye_openness: 0.4,
                sleepy: true,
                bounce: false,
            },
        }
    }

    /// Default (neutral) expression.
    pub fn default_neutral() -> Self {
        Self::from_mood(Mood::Neutral)
    }
}

impl Default for Expression {
    fn default() -> Self {
        Self::default_neutral()
    }
}
