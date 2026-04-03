use thiserror::Error;

#[derive(Debug, Error)]
pub enum WakeyError {
    #[error("Spine error: {0}")]
    Spine(String),

    #[error("Sense error ({sensor}): {message}")]
    Sense { sensor: String, message: String },

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("Action error: {0}")]
    Action(String),

    #[error("Safety denied: {action} — {reason}")]
    SafetyDenied { action: String, reason: String },

    #[error("LLM error ({provider}): {message}")]
    Llm { provider: String, message: String },

    #[error("Config error: {0}")]
    Config(String),

    #[error("Skill error ({skill}): {message}")]
    Skill { skill: String, message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),

    #[error("Database error: {0}")]
    Database(String),
}

impl From<rusqlite::Error> for WakeyError {
    fn from(err: rusqlite::Error) -> Self {
        WakeyError::Database(err.to_string())
    }
}

pub type WakeyResult<T> = Result<T, WakeyError>;
