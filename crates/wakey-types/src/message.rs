//! Chat message types for LLM communication.
//!
//! Simple structure matching OpenAI's chat completions API format.

use serde::{Deserialize, Serialize};

/// A single message in a chat conversation.
///
/// Matches the OpenAI `/v1/chat/completions` message format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: "system", "user", or "assistant"
    pub role: String,
    /// The message content
    pub content: String,
}

impl ChatMessage {
    /// Create a system message (sets behavior and context)
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    /// Create a user message (human input)
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    /// Create an assistant message (AI response)
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}
