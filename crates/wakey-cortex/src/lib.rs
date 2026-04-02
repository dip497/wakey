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
