//! Wakey Context — Memory + Skills Storage
//!
//! Based on:
//! - ZeroClaw Memory trait (store/recall/forget with hybrid search)
//! - OpenViking filesystem paradigm (wakey:// URIs, L0/L1/L2 tiers)
//! - OpenSpace quality metrics (selections/applied/completions/fallbacks)
//!
//! # Architecture
//!
//! ```text
//! ~/.wakey/
//! ├── context/                    # Filesystem = source of truth
//! │   ├── user/
//! │   │   └── memories/           # User preferences, patterns
//! │   ├── agent/
//! │   │   ├── skills/             # SKILL.md files live here
//! │   │   └── memories/           # Agent's learned patterns
//! │   ├── session/                # Current session working memory
//! │   └── resources/              # External knowledge (docs, repos)
//! └── index.db                    # SQLite FTS5 index (rebuilt from filesystem)
//! ```
//!
//! # Modules
//!
//! - [`filesystem`] - Filesystem layer with `wakey://` URIs
//! - [`memory`] - ZeroClaw Memory trait with SQLite FTS5
//! - [`tiers`] - L0/L1/L2 tiered loading for token efficiency
//! - [`retrieval`] - FTS5 search with BM25 ranking

pub mod filesystem;
pub mod memory;
pub mod retrieval;
pub mod tiers;

// Re-export main types for convenience
pub use filesystem::{ContextEntry, ContextFs, ContextPath, URI_SCHEME};
pub use memory::{
    Memory, MemoryCategory, MemoryEntry, SkillLineage, SkillMetrics, SqliteMemory,
};
pub use retrieval::{Retriever, SearchOptions, SearchResult};
pub use tiers::{ContextLevel, TieredContent, Tiers};