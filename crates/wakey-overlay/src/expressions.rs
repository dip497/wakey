//! Tabbie-style facial expressions for Wakey sprite.
//!
//! Provides modular facial features (eyes, mouth, eyebrows, accessories)
//! that can be combined to create expressive animations.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;

/// Eye shape variants (Tabbie-style simple geometry)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EyeShape {
    /// Default rectangular eyes
    Rectangle,
    /// Circular eyes
    Circle,
    /// Upward curve (happy/closed)
    CurveUp,
    /// Downward curve (sad/droopy)
    CurveDown,
    /// Small dot eyes
    Dot,
    /// Large wide eyes
    Wide,
    /// One eye open, one closed
    Wink,
}

/// Mouth shape variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouthShape {
    /// No mouth (neutral)
    None,
    /// Upward smile
    Smile,
    /// Flat line (serious)
    Flat,
    /// Open circle (surprised)
    Open,
    /// Wavy line (stressed)
    Wavy,
    /// Smile with tongue out
    Tongue,
}

/// Eyebrow shape variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EyebrowShape {
    /// No eyebrows
    None,
    /// Angled inward (angry)
    Angry,
    /// Angled outward (worried)
    Worried,
}

/// Accessory items (props around the face)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accessory {
    /// Lightbulb (idea moment)
    Lightbulb,
    /// Hand pointing left
    PointingLeft,
    /// Hand pointing right
    PointingRight,
    /// Hand on chin (thinking)
    ThinkingHand,
    /// Coffee cup
    CoffeeCup,
    /// Zzz (sleepy)
    Zzz,
    /// Heart (love/appreciation)
    Heart,
    /// Sparkles (celebration)
    Sparkle,
}

/// Animation priority for interruption logic
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    #[default]
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Complete facial expression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expression {
    /// Expression name (for config files)
    pub name: String,
    /// Eye shape
    pub eyes: EyeShape,
    /// Mouth shape
    pub mouth: MouthShape,
    /// Eyebrow shape
    pub eyebrows: EyebrowShape,
    /// Accessories (up to 3 recommended)
    #[serde(default)]
    pub accessories: Vec<Accessory>,
    /// How long to show this expression (0 = indefinite)
    #[serde(default)]
    pub duration_ms: u64,
    /// Whether to loop the animation (not yet implemented)
    #[serde(default)]
    pub loops: bool,
    /// Priority level for interruption
    #[serde(default)]
    pub priority: Priority,
    /// Transition duration from previous expression
    #[serde(default = "default_transition")]
    pub transition_ms: u64,
    /// Glow color (RGBA, 0.0-1.0)
    #[serde(default = "default_glow")]
    pub glow_color: [f32; 4],
}

fn default_transition() -> u64 { 150 }
fn default_glow() -> [f32; 4] { [0.95, 0.65, 0.25, 0.85] }

impl Expression {
    /// Neutral/default expression
    pub fn neutral() -> Self {
        Self {
            name: "neutral".to_string(),
            eyes: EyeShape::Rectangle,
            mouth: MouthShape::None,
            eyebrows: EyebrowShape::None,
            accessories: Vec::new(),
            duration_ms: 0,
            loops: false,
            priority: Priority::Low,
            transition_ms: 150,
            glow_color: [0.95, 0.65, 0.25, 0.85],
        }
    }

    /// Happy expression
    pub fn happy() -> Self {
        Self {
            name: "happy".to_string(),
            eyes: EyeShape::Rectangle,
            mouth: MouthShape::Smile,
            eyebrows: EyebrowShape::None,
            accessories: Vec::new(),
            duration_ms: 2000,
            loops: false,
            priority: Priority::Normal,
            transition_ms: 150,
            glow_color: [0.95, 0.75, 0.35, 0.95],
        }
    }

    /// Celebration expression (task completed)
    pub fn celebrate() -> Self {
        Self {
            name: "celebrate".to_string(),
            eyes: EyeShape::CurveUp,
            mouth: MouthShape::Tongue,
            eyebrows: EyebrowShape::None,
            accessories: vec![Accessory::Sparkle],
            duration_ms: 3000,
            loops: false,
            priority: Priority::High,
            transition_ms: 100,
            glow_color: [0.95, 0.85, 0.45, 1.0],
        }
    }

    /// Idea moment (lightbulb)
    pub fn idea() -> Self {
        Self {
            name: "idea".to_string(),
            eyes: EyeShape::Wide,
            mouth: MouthShape::Smile,
            eyebrows: EyebrowShape::None,
            accessories: vec![Accessory::Lightbulb, Accessory::PointingRight],
            duration_ms: 2500,
            loops: false,
            priority: Priority::High,
            transition_ms: 120,
            glow_color: [0.95, 0.75, 0.35, 0.95],
        }
    }

    /// Angry/frustrated expression
    pub fn angry() -> Self {
        Self {
            name: "angry".to_string(),
            eyes: EyeShape::Rectangle,
            mouth: MouthShape::Wavy,
            eyebrows: EyebrowShape::Angry,
            accessories: Vec::new(),
            duration_ms: 2000,
            loops: false,
            priority: Priority::High,
            transition_ms: 80,
            glow_color: [0.85, 0.35, 0.35, 0.80],
        }
    }

    /// Worried/concerned expression
    pub fn worried() -> Self {
        Self {
            name: "worried".to_string(),
            eyes: EyeShape::Circle,
            mouth: MouthShape::Wavy,
            eyebrows: EyebrowShape::Worried,
            accessories: Vec::new(),
            duration_ms: 2000,
            loops: false,
            priority: Priority::Normal,
            transition_ms: 100,
            glow_color: [0.85, 0.45, 0.35, 0.75],
        }
    }

    /// Meeting mode (DND)
    pub fn meeting() -> Self {
        Self {
            name: "meeting".to_string(),
            eyes: EyeShape::Wink,
            mouth: MouthShape::Flat,
            eyebrows: EyebrowShape::None,
            accessories: vec![Accessory::PointingRight],
            duration_ms: 0,
            loops: false,
            priority: Priority::Normal,
            transition_ms: 150,
            glow_color: [0.75, 0.65, 0.85, 0.70],
        }
    }

    /// Sleepy expression (idle >5min)
    pub fn sleepy() -> Self {
        Self {
            name: "sleepy".to_string(),
            eyes: EyeShape::CurveDown,
            mouth: MouthShape::None,
            eyebrows: EyebrowShape::None,
            accessories: vec![Accessory::Zzz],
            duration_ms: 0,
            loops: false,
            priority: Priority::Low,
            transition_ms: 200,
            glow_color: [0.60, 0.45, 0.25, 0.50],
        }
    }

    /// Focused/deep work expression
    pub fn focused() -> Self {
        Self {
            name: "focused".to_string(),
            eyes: EyeShape::Dot,
            mouth: MouthShape::Flat,
            eyebrows: EyebrowShape::None,
            accessories: Vec::new(),
            duration_ms: 0,
            loops: false,
            priority: Priority::Low,
            transition_ms: 200,
            glow_color: [0.30, 0.50, 0.85, 0.75],
        }
    }

    /// Love/appreciation expression
    pub fn love() -> Self {
        Self {
            name: "love".to_string(),
            eyes: EyeShape::CurveUp,
            mouth: MouthShape::Smile,
            eyebrows: EyebrowShape::None,
            accessories: vec![Accessory::Heart],
            duration_ms: 2000,
            loops: false,
            priority: Priority::Normal,
            transition_ms: 150,
            glow_color: [0.95, 0.55, 0.65, 0.90],
        }
    }

    /// Surprised/shocked expression
    pub fn surprised() -> Self {
        Self {
            name: "surprised".to_string(),
            eyes: EyeShape::Wide,
            mouth: MouthShape::Open,
            eyebrows: EyebrowShape::Worried,
            accessories: Vec::new(),
            duration_ms: 1500,
            loops: false,
            priority: Priority::High,
            transition_ms: 80,
            glow_color: [0.95, 0.65, 0.25, 0.85],
        }
    }

    /// Thinking/contemplating expression
    pub fn thinking() -> Self {
        Self {
            name: "thinking".to_string(),
            eyes: EyeShape::Dot,
            mouth: MouthShape::Flat,
            eyebrows: EyebrowShape::None,
            accessories: vec![Accessory::ThinkingHand],
            duration_ms: 0,
            loops: false,
            priority: Priority::Low,
            transition_ms: 150,
            glow_color: [0.75, 0.65, 0.85, 0.70],
        }
    }

    /// Load expression from JSON file
    pub fn load_from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        let expr: Expression = serde_json::from_str(&content)?;
        Ok(expr)
    }

    /// Load all expressions from a directory
    pub fn load_directory(dir: &Path) -> Result<Vec<Self>, Box<dyn std::error::Error>> {
        let mut expressions = Vec::new();
        
        if !dir.exists() {
            return Ok(expressions);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(expr) = Self::load_from_file(&path) {
                    expressions.push(expr);
                }
            }
        }

        Ok(expressions)
    }

    /// Legacy: Create expression from Mood (backwards compatibility)
    pub fn from_mood(mood: wakey_types::Mood) -> Self {
        match mood {
            wakey_types::Mood::Neutral => Self::neutral(),
            wakey_types::Mood::Happy => Self::happy(),
            wakey_types::Mood::Empathetic => Self::love(),
            wakey_types::Mood::Focused => Self::focused(),
            wakey_types::Mood::Playful => Self::celebrate(),
            wakey_types::Mood::Concerned => Self::worried(),
            wakey_types::Mood::Sleepy => Self::sleepy(),
        }
    }
}

impl Default for Expression {
    fn default() -> Self {
        Self::neutral()
    }
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neutral_expression() {
        let expr = Expression::neutral();
        assert_eq!(expr.eyes, EyeShape::Rectangle);
        assert_eq!(expr.mouth, MouthShape::None);
        assert_eq!(expr.eyebrows, EyebrowShape::None);
        assert!(expr.accessories.is_empty());
    }

    #[test]
    fn test_celebrate_expression() {
        let expr = Expression::celebrate();
        assert_eq!(expr.eyes, EyeShape::CurveUp);
        assert_eq!(expr.mouth, MouthShape::Tongue);
        assert!(expr.accessories.contains(&Accessory::Sparkle));
        assert_eq!(expr.priority, Priority::High);
    }

    #[test]
    fn test_expression_serialization() {
        let expr = Expression::happy();
        let json = serde_json::to_string(&expr).unwrap();
        assert!(json.contains("happy"));
        assert!(json.contains("smile"));
    }

    #[test]
    fn test_expression_deserialization() {
        let json = r#"{
            "name": "test",
            "eyes": "wide",
            "mouth": "open",
            "eyebrows": "worried",
            "accessories": ["heart"],
            "duration_ms": 1000,
            "priority": "high"
        }"#;
        let expr: Expression = serde_json::from_str(json).unwrap();
        assert_eq!(expr.eyes, EyeShape::Wide);
        assert_eq!(expr.mouth, MouthShape::Open);
        assert_eq!(expr.eyebrows, EyebrowShape::Worried);
        assert!(expr.accessories.contains(&Accessory::Heart));
    }
}

// ── MOOD Tag Parsing ──

/// Parse MOOD: tag from LLM response. Returns (clean_text, mood_string).
pub fn parse_mood_tag(text: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    for i in (0..lines.len()).rev().take(3) {
        let line = lines[i].trim();
        if let Some(mood) = line.strip_prefix("MOOD:") {
            let mood = mood.trim().to_lowercase();
            let clean: Vec<&str> = lines[..i].iter().copied()
                .chain(lines[i+1..].iter().copied())
                .collect();
            return (clean.join("\n").trim().to_string(), Some(mood));
        }
    }
    (text.to_string(), None)
}

/// Detect mood from keywords when no MOOD: tag present.
pub fn detect_mood_from_keywords(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if lower.contains("error") || lower.contains("failed") || lower.contains("wrong") {
        Some("concerned".to_string())
    } else if lower.contains("great") || lower.contains("awesome") || lower.contains("nice work") {
        Some("happy".to_string())
    } else if lower.contains("wow") || lower.contains("amazing") {
        Some("excited".to_string())
    } else if lower.contains("hmm") || lower.contains("let me think") {
        Some("thinking".to_string())
    } else if lower.contains("sorry") || lower.contains("unfortunately") {
        Some("empathetic".to_string())
    } else {
        None
    }
}

/// Get mood from text — tries MOOD: tag first, then keywords.
pub fn extract_mood(text: &str) -> (String, String) {
    let (clean, tag_mood) = parse_mood_tag(text);
    if let Some(mood) = tag_mood {
        return (clean, mood);
    }
    if let Some(mood) = detect_mood_from_keywords(&clean) {
        return (clean, mood);
    }
    (clean, "neutral".to_string())
}
