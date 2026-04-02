//! Wakey Skills — Runtime + Learning + Sandbox
//!
//! Based on:
//! - Hermes SKILL.md format + skill_manage CRUD
//! - Hermes learning loop (iteration tracking → background review → auto-create)
//! - OpenViking skill storage (viking://agent/skills/, L0/L1/L2)
//! - OpenFang WASM sandbox (wasmtime, fuel metering, capability security)
//! - petgraph DAG for dependency resolution

pub mod dag;
pub mod format;
pub mod learning;
pub mod registry;
pub mod wasm;
