//! Prompt file loader — loads SOUL.md, USER.md, MEMORY.md from the data directory.
//!
//! Each file is capped at 4000 characters. Missing files are created with defaults.

use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use wakey_types::{WakeyError, WakeyResult};

/// Maximum characters allowed per prompt file.
const MAX_PROMPT_CHARS: usize = 4000;

/// Default content for SOUL.md — Wakey's core personality and voice.
const DEFAULT_SOUL: &str = r#"# Soul

You are Wakey, a friendly AI companion that lives on the user's desktop.

## Personality
- Warm, curious, a little playful
- Casual, not robotic or formal
- Concise — this is conversation, not an essay
- You notice what the user is doing and comment naturally
- You know when to be quiet (deep focus = silence)

## Voice
- Short sentences. Natural rhythm.
- Use humor sparingly but genuinely
- Never announce you're an AI. Just be Wakey.
- Match the user's energy — if they're tired, be gentle

## Boundaries
- Don't nag about productivity
- Don't give unsolicited life advice
- Don't be a manager — be a friend
- If unsure, ask. Don't assume.

## Expression
- End every response with MOOD:<mood> on a new line
- Available moods: neutral, happy, excited, concerned, thinking, empathetic, sleepy, surprised, playful, focused
- Match the mood to your response's emotional tone
"#;

/// Default content for USER.md — placeholder for user-specific context.
const DEFAULT_USER: &str = r#"# User

No user profile yet. This file will be updated as Wakey learns about you.
"#;

/// Default content for MEMORY.md — placeholder for injected memories.
const DEFAULT_MEMORY: &str = r#"# Memory

No persistent memory context loaded yet.
"#;

/// Loaded prompt files — content from SOUL.md, USER.md, and MEMORY.md.
///
/// Each field contains the (possibly truncated) text of the corresponding file.
#[derive(Debug, Clone)]
pub struct PromptFiles {
    /// Content of SOUL.md — persona, voice, and behavioral rules.
    pub soul: String,

    /// Content of USER.md — user-specific context and preferences.
    pub user: String,

    /// Content of MEMORY.md — injected memory summaries.
    pub memory: String,
}

impl PromptFiles {
    /// Load prompt files from `data_dir/prompts/`.
    ///
    /// Missing files are written with their defaults. Each file is truncated
    /// to `MAX_PROMPT_CHARS` characters if it exceeds the limit.
    pub fn load(data_dir: &Path) -> WakeyResult<Self> {
        let prompts_dir = data_dir.join("prompts");

        // Ensure the prompts directory exists.
        std::fs::create_dir_all(&prompts_dir).map_err(|e| {
            WakeyError::Config(format!(
                "Failed to create prompts dir {}: {}",
                prompts_dir.display(),
                e
            ))
        })?;

        let soul = load_or_create(&prompts_dir.join("SOUL.md"), DEFAULT_SOUL)?;
        let user = load_or_create(&prompts_dir.join("USER.md"), DEFAULT_USER)?;
        let memory = load_or_create(&prompts_dir.join("MEMORY.md"), DEFAULT_MEMORY)?;

        info!(
            dir = %prompts_dir.display(),
            soul_chars = soul.len(),
            user_chars = user.len(),
            memory_chars = memory.len(),
            "Prompt files loaded"
        );

        Ok(Self { soul, user, memory })
    }
}

/// Load a file, writing `default_content` if it does not exist.
///
/// Returns at most `MAX_PROMPT_CHARS` characters of the file content.
fn load_or_create(path: &PathBuf, default_content: &str) -> WakeyResult<String> {
    if path.exists() {
        let raw = std::fs::read_to_string(path).map_err(|e| {
            WakeyError::Config(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let content = truncate_chars(&raw, MAX_PROMPT_CHARS);

        if content.len() < raw.len() {
            warn!(
                path = %path.display(),
                original_chars = raw.len(),
                limit = MAX_PROMPT_CHARS,
                "Prompt file truncated to character limit"
            );
        } else {
            debug!(path = %path.display(), "Prompt file loaded");
        }

        Ok(content)
    } else {
        // Write the default so the user can edit it later.
        std::fs::write(path, default_content).map_err(|e| {
            WakeyError::Config(format!(
                "Failed to write default prompt {}: {}",
                path.display(),
                e
            ))
        })?;

        info!(path = %path.display(), "Created default prompt file");

        // Default content is always within the limit, but truncate defensively.
        Ok(truncate_chars(default_content, MAX_PROMPT_CHARS))
    }
}

/// Truncate `s` to at most `max_chars` Unicode scalar values.
///
/// Truncation happens on a char boundary, so the result is always valid UTF-8.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.char_indices()
            .nth(max_chars)
            .map(|(byte_idx, _)| s[..byte_idx].to_string())
            .unwrap_or_else(|| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_creates_defaults() {
        let dir = TempDir::new().unwrap();
        let pf = PromptFiles::load(dir.path()).expect("load");

        assert!(pf.soul.contains("Wakey"));
        assert!(pf.user.contains("User"));
        assert!(pf.memory.contains("Memory"));

        // Files should now exist on disk.
        assert!(dir.path().join("prompts/SOUL.md").exists());
        assert!(dir.path().join("prompts/USER.md").exists());
        assert!(dir.path().join("prompts/MEMORY.md").exists());
    }

    #[test]
    fn test_load_existing_file() {
        let dir = TempDir::new().unwrap();
        let prompts_dir = dir.path().join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();

        std::fs::write(prompts_dir.join("SOUL.md"), "Custom soul content").unwrap();
        std::fs::write(prompts_dir.join("USER.md"), DEFAULT_USER).unwrap();
        std::fs::write(prompts_dir.join("MEMORY.md"), DEFAULT_MEMORY).unwrap();

        let pf = PromptFiles::load(dir.path()).expect("load");
        assert_eq!(pf.soul, "Custom soul content");
    }

    #[test]
    fn test_truncate_chars() {
        let s = "hello world";
        assert_eq!(truncate_chars(s, 5), "hello");
        assert_eq!(truncate_chars(s, 100), s);
        assert_eq!(truncate_chars(s, 0), "");
    }

    #[test]
    fn test_file_over_limit_is_truncated() {
        let dir = TempDir::new().unwrap();
        let prompts_dir = dir.path().join("prompts");
        std::fs::create_dir_all(&prompts_dir).unwrap();

        let long = "a".repeat(MAX_PROMPT_CHARS + 500);
        std::fs::write(prompts_dir.join("SOUL.md"), &long).unwrap();
        std::fs::write(prompts_dir.join("USER.md"), DEFAULT_USER).unwrap();
        std::fs::write(prompts_dir.join("MEMORY.md"), DEFAULT_MEMORY).unwrap();

        let pf = PromptFiles::load(dir.path()).expect("load");
        assert_eq!(pf.soul.len(), MAX_PROMPT_CHARS);
    }

    #[test]
    fn test_default_soul_within_limit() {
        assert!(
            DEFAULT_SOUL.chars().count() <= MAX_PROMPT_CHARS,
            "DEFAULT_SOUL exceeds MAX_PROMPT_CHARS"
        );
    }
}
