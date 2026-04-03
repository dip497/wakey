//! Retrieval layer for context search.
//!
//! OpenViking pattern: directory-recursive retrieval with:
//! - Step 1: FTS5 search across l0_abstract + l1_overview
//! - Step 2: BM25 ranking
//! - Step 3: Return top-N with L0 summaries
//! - Step 4: Caller can request L2 (full content) for specific results

use std::sync::Arc;
use tracing::{debug, instrument};

use crate::filesystem::{ContextFs, ContextPath};
use crate::memory::{Memory, MemoryCategory, SqliteMemory};
use crate::tiers::{ContextLevel, TieredContent, Tiers};
use wakey_types::WakeyResult;

/// A single search result with relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Tiered content (L0/L1 populated, L2 loaded on demand)
    pub content: TieredContent,
    /// BM25 relevance score (higher = more relevant)
    pub score: f64,
    /// Category of the result
    pub category: String,
}

impl SearchResult {
    /// Get the L0 abstract.
    pub fn l0(&self) -> &str {
        self.content.get(ContextLevel::Abstract)
    }

    /// Get the L1 overview.
    pub fn l1(&self) -> &str {
        self.content.get(ContextLevel::Overview)
    }

    /// Get the L2 full content (loads from filesystem if needed).
    pub async fn detail(&mut self, fs: &ContextFs) -> WakeyResult<&str> {
        self.content.load_l2(fs).await?;
        Ok(self.content.get(ContextLevel::Detail))
    }
}

/// Search options for customizing retrieval.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Maximum results to return
    pub limit: usize,
    /// Filter by category (None = all categories)
    pub category: Option<String>,
    /// Minimum relevance score threshold
    pub min_score: f64,
    /// Include L2 content in results (loads full content)
    pub include_l2: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            limit: 10,
            category: None,
            min_score: 0.0,
            include_l2: false,
        }
    }
}

/// Context retriever with FTS5 + BM25 ranking.
pub struct Retriever {
    memory: Arc<SqliteMemory>,
    fs: Arc<ContextFs>,
    #[allow(dead_code)]
    tiers: Arc<Tiers>,
}

impl Retriever {
    /// Create a new retriever.
    pub fn new(memory: Arc<SqliteMemory>, fs: Arc<ContextFs>, tiers: Arc<Tiers>) -> Self {
        Self { memory, fs, tiers }
    }

    /// Search for context matching a query.
    #[instrument(skip(self))]
    pub async fn search(&self, query: &str, options: &SearchOptions) -> WakeyResult<Vec<SearchResult>> {
        debug!("Searching for '{}' with limit {}", query, options.limit);

        let entries = self.memory.recall(query, options.limit).await?;

        let mut results: Vec<SearchResult> = entries
            .into_iter()
            .enumerate()
            .map(|(idx, entry)| {
                let score = 1.0 / (1.0 + idx as f64);

                let tiered = TieredContent::new(
                    ContextPath::from_uri(&entry.key),
                    entry.l0_abstract,
                    entry.l1_overview,
                );

                SearchResult {
                    content: tiered,
                    score,
                    category: entry.category.as_str().to_string(),
                }
            })
            .filter(|r| r.score >= options.min_score)
            .collect();

        if let Some(ref cat) = options.category {
            results.retain(|r| &r.category == cat);
        }

        if options.include_l2 {
            for result in &mut results {
                result.content.load_l2(&self.fs).await?;
            }
        }

        debug!("Found {} results for query '{}'", results.len(), query);
        Ok(results)
    }

    /// Quick search with default options.
    pub async fn quick_search(&self, query: &str) -> WakeyResult<Vec<SearchResult>> {
        self.search(query, &SearchOptions::default()).await
    }

    /// Deep search with L2 content.
    pub async fn deep_search(&self, query: &str) -> WakeyResult<Vec<SearchResult>> {
        self.search(
            query,
            &SearchOptions {
                include_l2: true,
                ..Default::default()
            },
        )
        .await
    }

    /// Search within a specific category.
    pub async fn search_by_category(
        &self,
        query: &str,
        category: &MemoryCategory,
        limit: usize,
    ) -> WakeyResult<Vec<SearchResult>> {
        self.search(
            query,
            &SearchOptions {
                limit,
                category: Some(category.as_str().to_string()),
                ..Default::default()
            },
        )
        .await
    }

    /// Retrieve context entries by path prefix.
    #[instrument(skip(self))]
    pub async fn retrieve_by_prefix(&self, prefix: &str, limit: usize) -> WakeyResult<Vec<SearchResult>> {
        let all_entries = self.memory.list(None).await?;

        let results: Vec<SearchResult> = all_entries
            .into_iter()
            .filter(|e| e.key.starts_with(prefix))
            .take(limit)
            .map(|entry| {
                let tiered = TieredContent::new(
                    ContextPath::from_uri(&entry.key),
                    entry.l0_abstract,
                    entry.l1_overview,
                );

                SearchResult {
                    content: tiered,
                    score: 1.0,
                    category: entry.category.as_str().to_string(),
                }
            })
            .collect();

        debug!("Found {} entries under prefix '{}'", results.len(), prefix);
        Ok(results)
    }

    /// Get a single context entry by path.
    #[instrument(skip(self))]
    pub async fn get(&self, path: &str) -> WakeyResult<Option<SearchResult>> {
        if let Some(entry) = self.memory.get(path).await? {
            let tiered = TieredContent::new(
                ContextPath::from_uri(&entry.key),
                entry.l0_abstract,
                entry.l1_overview,
            );

            Ok(Some(SearchResult {
                content: tiered,
                score: 1.0,
                category: entry.category.as_str().to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Index a file into the memory store.
    #[instrument(skip(self))]
    pub async fn index_file(
        &self,
        path: &ContextPath,
        category: &MemoryCategory,
    ) -> WakeyResult<()> {
        let content = self.fs.read(path).await?;
        self.memory.store(&path.uri(), &content, category).await?;
        debug!("Indexed file: {}", path.uri());
        Ok(())
    }

    /// Index all files under a directory.
    #[instrument(skip(self))]
    pub async fn index_directory(
        &self,
        dir_path: &ContextPath,
        category: &MemoryCategory,
    ) -> WakeyResult<usize> {
        let files = self.fs.list_all_files(dir_path).await?;
        let mut indexed = 0;

        for file_entry in files {
            let content = self.fs.read(file_entry.path()).await?;
            self.memory
                .store(&file_entry.path().uri(), &content, category)
                .await?;
            indexed += 1;
        }

        debug!("Indexed {} files under {}", indexed, dir_path.uri());
        Ok(indexed)
    }

    /// Rebuild the entire index from filesystem.
    #[instrument(skip(self))]
    pub async fn rebuild_index(&self) -> WakeyResult<usize> {
        let mut total_indexed = 0;

        let user_memories = ContextPath::new("user/memories");
        total_indexed += self
            .index_directory(&user_memories, &MemoryCategory::Core)
            .await?;

        let agent_memories = ContextPath::new("agent/memories");
        total_indexed += self
            .index_directory(&agent_memories, &MemoryCategory::Core)
            .await?;

        let skills = ContextPath::new("agent/skills");
        total_indexed += self
            .index_directory(&skills, &MemoryCategory::Skill)
            .await?;

        debug!("Rebuilt index with {} entries", total_indexed);
        Ok(total_indexed)
    }

    /// Get the underlying memory store.
    pub fn memory(&self) -> Arc<SqliteMemory> {
        self.memory.clone()
    }

    /// Get the underlying filesystem.
    pub fn filesystem(&self) -> Arc<ContextFs> {
        self.fs.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_search_basic() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(ContextFs::new(temp_dir.path().join("context")));
        let memory = Arc::new(SqliteMemory::new_in_memory().unwrap());
        let tiers = Arc::new(Tiers::new(fs.clone(), memory.clone(), 100));

        memory
            .store(
                "user/memories/prefs.md",
                "# User Preferences\n\nUser likes dark mode and prefers Rust over Python.",
                &MemoryCategory::Core,
            )
            .await
            .unwrap();

        memory
            .store(
                "agent/memories/patterns.md",
                "# Learned Patterns\n\nUser often works on Rust projects.",
                &MemoryCategory::Core,
            )
            .await
            .unwrap();

        let retriever = Retriever::new(memory, fs, tiers);

        let results = retriever.quick_search("Rust").await.unwrap();
        assert!(!results.is_empty());

        for result in &results {
            assert!(!result.l0().is_empty());
        }
    }

    #[tokio::test]
    async fn test_search_by_category() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(ContextFs::new(temp_dir.path().join("context")));
        let memory = Arc::new(SqliteMemory::new_in_memory().unwrap());
        let tiers = Arc::new(Tiers::new(fs.clone(), memory.clone(), 100));

        memory
            .store("skill1.md", "Skill content", &MemoryCategory::Skill)
            .await
            .unwrap();

        memory
            .store("core1.md", "Core content", &MemoryCategory::Core)
            .await
            .unwrap();

        let retriever = Retriever::new(memory, fs, tiers);

        let results = retriever
            .search_by_category("content", &MemoryCategory::Skill, 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "skill");
    }

    #[tokio::test]
    async fn test_retrieve_by_prefix() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(ContextFs::new(temp_dir.path().join("context")));
        let memory = Arc::new(SqliteMemory::new_in_memory().unwrap());
        let tiers = Arc::new(Tiers::new(fs.clone(), memory.clone(), 100));

        memory
            .store("user/memories/a.md", "content a", &MemoryCategory::Core)
            .await
            .unwrap();

        memory
            .store("user/memories/b.md", "content b", &MemoryCategory::Core)
            .await
            .unwrap();

        memory
            .store("agent/skills/c.md", "content c", &MemoryCategory::Skill)
            .await
            .unwrap();

        let retriever = Retriever::new(memory, fs, tiers);

        let results = retriever
            .retrieve_by_prefix("user/memories", 10)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_index_file() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(ContextFs::new(temp_dir.path().join("context")));
        let memory = Arc::new(SqliteMemory::new_in_memory().unwrap());
        let tiers = Arc::new(Tiers::new(fs.clone(), memory.clone(), 100));

        let path = ContextPath::new("user/memories/test.md");
        fs.write(&path, "# Test\n\nThis is test content.")
            .await
            .unwrap();

        let retriever = Retriever::new(memory, fs, tiers);

        retriever
            .index_file(&path, &MemoryCategory::Core)
            .await
            .unwrap();

        let results = retriever.quick_search("test").await.unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_search_options_default() {
        let opts = SearchOptions::default();
        assert_eq!(opts.limit, 10);
        assert!(opts.category.is_none());
        assert_eq!(opts.min_score, 0.0);
        assert!(!opts.include_l2);
    }
}