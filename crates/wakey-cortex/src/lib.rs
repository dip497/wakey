//! Wakey Cortex — The Brain
//!
//! Based on:
//! - ZeroClaw agent loop (run_tool_call_loop with cancellation, budget, loop detection)
//! - OpenClaw heartbeat (configurable intervals, lightContext)
//! - ZeroClaw context assembly (recall → build prompt → history → trim → send)

pub mod agent_loop;
pub mod decision;
pub mod heartbeat;
pub mod llm;

#[cfg(feature = "voice")]
pub mod voice;

// Re-export main types for convenience
pub use llm::{LlmProvider, OpenAiCompatible};

#[cfg(feature = "voice")]
pub use voice::{PushToTalkHandler, VoiceError, VoiceSession};
