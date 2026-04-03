//! Wakey Skills — Runtime + Learning + Sandbox
//!
//! Based on:
//! - Hermes SKILL.md format + skill_manage CRUD
//! - Hermes learning loop (iteration tracking → background review → auto-create)
//! - OpenSpace skill evolution (FIX/DERIVED/CAPTURED with lineage tracking)
//! - OpenSpace quality metrics (selection/applied/completion rates)
//! - OpenViking skill storage (L0/L1/L2 tiered abstraction)
//! - petgraph DAG for dependency resolution

pub mod agent_supervisor;
pub mod dag;
pub mod evolution;
pub mod format;
pub mod learning;
pub mod quality;
pub mod registry;
pub mod wasm;

// Re-export main types
pub use agent_supervisor::{AgentSupervisor, AgentType, SupervisorConfig};
pub use dag::{DagStats, SkillDag, SkillNode};
pub use evolution::{EvolutionType, SkillEvolver, SkillLineage, SkillOrigin};
pub use format::{SkillContent, SkillManifest, generate_abstract, generate_overview, parse_skill};
pub use learning::{LearningStats, LearningTracker, SkillReviewPrompt, TriggerReason};
pub use quality::{QualityTracker, SkillMetrics};
pub use registry::{SkillMatch, SkillRegistry};

/// Skills module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
