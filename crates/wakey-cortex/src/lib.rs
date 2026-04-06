//! Wakey Cortex — The Brain
//!
//! Based on:
//! - ZeroClaw agent loop (run_tool_call_loop with cancellation, budget, loop detection)
//! - OpenClaw heartbeat (configurable intervals, lightContext)
//! - ZeroClaw context assembly (recall → build prompt → history → trim → send)
//!
//! Voice is now a PLUGIN (see plugin_host.rs):
//! - Core emits ShouldSpeak → plugin speaks
//! - Plugin emits Voice* events → core handles them
//! - No audio dependencies in core

pub mod agent_loop;
pub mod decision;
pub mod heartbeat;
pub mod llm;
pub mod plugin_host;
pub mod prompt_loader;

// Re-export main types for convenience
pub use agent_loop::{AgentLoop, init_memory_db, init_skills_dir};
pub use decision::{DecisionContext, assemble_context, handle_reflect, store_conversation_fact};
pub use llm::{LlmProvider, OpenAiCompatible};
pub use plugin_host::{PluginConfig, PluginError, PluginHost};
pub use prompt_loader::PromptFiles;
