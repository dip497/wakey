//! Wakey Action — Hands + Safety
//!
//! Based on:
//! - ZeroClaw SecurityPolicy::can_act() for pre-execution checks
//! - Sondera Cedar policy engine for declarative guardrails
//! - Platform-native input injection (xdotool on Linux)

pub mod browser;
pub mod files;
pub mod grounding;
pub mod input;
pub mod safety;
pub mod terminal;
