//! Tiered loading for context entries.
//!
//! Based on OpenViking L0/L1/L2 pattern:
//! - L0: One-line summary (~100 chars) — stored in index.db
//! - L1: Paragraph overview (~500 chars) — stored in index.db
//! - L2: Full content — read from filesystem on demand

use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, instrument};

use crate::filesystem::{ContextFs, ContextPath};
use crate::memory::{Memory, SqliteMemory};
use wakey_types::WakeyResult;

/// Context level for tiered loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContextLevel {
    /// L0: Abstract summary (~100 chars)
    Abstract = 0,
    /// L1: Overview (~500 chars)
    Overview = 1,
    /// L2: Full detail (complete content)
    Detail = 2,
}

impl ContextLevel {
    /// Get human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            ContextLevel::Abstract => "L0",
            ContextLevel::Overview => "L1",
            ContextLevel::Detail => "L2",
        }
    }
}

/// Tiered content container.
#[derive(Debug, Clone)]
pub struct TieredContent {
    /// Context path (URI)
    pub path: ContextPath,
    /// L0 abstract summary (~100 chars)
    pub l0_abstract: String,
    /// L1 overview (~500 chars)
    pub l1_overview: String,
    /// L2 full content (loaded on demand)
    pub l2_detail: Option<String>,
    /// Whether L2 has been loaded
    l2_loaded: bool,
}

impl TieredContent {
    /// Create new tiered content with L0/L1 summaries.
    pub fn new(path: ContextPath, l0_abstract: String, l1_overview: String) -> Self {
        Self {
            path,
            l0_abstract,
            l1_overview,
            l2_detail: None,
            l2_loaded: false,
        }
    }

    /// Get content at a specific level.
    pub fn get(&self, level: ContextLevel) -> &str {
        match level {
            ContextLevel::Abstract => &self.l0_abstract,
            ContextLevel::Overview => &self.l1_overview,
            ContextLevel::Detail => self.l2_detail.as_deref().unwrap_or(&self.l1_overview),
        }
    }

    /// Check if L2 is loaded.
    pub fn is_l2_loaded(&self) -> bool {
        self.l2_loaded
    }

    /// Load L2 content from filesystem.
    pub async fn load_l2(&mut self, fs: &ContextFs) -> WakeyResult<()> {
        if self.l2_loaded {
            return Ok(());
        }

        self.l2_detail = Some(fs.read(&self.path).await?);
        self.l2_loaded = true;
        debug!("Loaded L2 for {}", self.path.uri());
        Ok(())
    }

    /// Estimate token count for the content.
    pub fn estimate_tokens(&self, level: ContextLevel) -> usize {
        let content = self.get(level);
        content.len() / 4
    }
}

/// Tiered retrieval interface.
pub struct Tiers {
    fs: Arc<ContextFs>,
    memory: Arc<SqliteMemory>,
    cache: Arc<RwLock<LruCache<String, TieredContent>>>,
}

impl Tiers {
    /// Create a new tiered loader.
    pub fn new(fs: Arc<ContextFs>, memory: Arc<SqliteMemory>, cache_size: usize) -> Self {
        let cache_size = NonZeroUsize::new(cache_size).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            fs,
            memory,
            cache: Arc::new(RwLock::new(LruCache::new(cache_size))),
        }
    }

    /// Get L0 abstract for a path.
    #[instrument(skip(self))]
    pub async fn get_l0(&self, path: &ContextPath) -> WakeyResult<String> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.peek(&path.uri()) {
                return Ok(cached.l0_abstract.clone());
            }
        }

        if let Some(entry) = self.memory.get(&path.uri()).await? {
            Ok(entry.l0_abstract)
        } else {
            Ok(String::new())
        }
    }

    /// Get L1 overview for a path.
    #[instrument(skip(self))]
    pub async fn get_l1(&self, path: &ContextPath) -> WakeyResult<String> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.peek(&path.uri()) {
                return Ok(cached.l1_overview.clone());
            }
        }

        if let Some(entry) = self.memory.get(&path.uri()).await? {
            Ok(entry.l1_overview)
        } else {
            Ok(String::new())
        }
    }

    /// Get L2 full content for a path.
    #[instrument(skip(self))]
    pub async fn get_l2(&self, path: &ContextPath) -> WakeyResult<String> {
        {
            let mut cache = self.cache.write().await;
            if let Some(cached) = cache.get_mut(&path.uri())
                && cached.is_l2_loaded()
            {
                return Ok(cached.l2_detail.clone().unwrap_or_default());
            }
        }

        let content = self.fs.read(path).await?;
        Ok(content)
    }

    /// Get tiered content for a path.
    #[instrument(skip(self))]
    pub async fn get_tiered(&self, path: ContextPath) -> WakeyResult<TieredContent> {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.peek(&path.uri()) {
                return Ok(cached.clone());
            }
        }

        if let Some(entry) = self.memory.get(&path.uri()).await? {
            let tiered = TieredContent::new(path.clone(), entry.l0_abstract, entry.l1_overview);

            let mut cache = self.cache.write().await;
            cache.put(path.uri(), tiered.clone());

            Ok(tiered)
        } else if self.fs.exists(&path).await? {
            let content = self.fs.read(&path).await?;
            let l0 = Self::generate_l0(&content);
            let l1 = Self::generate_l1(&content);

            let tiered = TieredContent::new(path.clone(), l0, l1);

            let mut cache = self.cache.write().await;
            cache.put(path.uri(), tiered.clone());

            Ok(tiered)
        } else {
            Ok(TieredContent::new(path, String::new(), String::new()))
        }
    }

    /// Get multiple tiered entries at once.
    #[instrument(skip(self))]
    pub async fn get_tiered_batch(
        &self,
        paths: Vec<ContextPath>,
    ) -> WakeyResult<Vec<TieredContent>> {
        let mut results = Vec::with_capacity(paths.len());

        for path in paths {
            results.push(self.get_tiered(path).await?);
        }

        Ok(results)
    }

    /// Clear the tiered cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        debug!("Cleared tiered cache");
    }

    /// Generate L0 abstract from content.
    fn generate_l0(content: &str) -> String {
        let first_line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let trimmed = first_line.trim();
        if trimmed.len() > 100 {
            trimmed[..100].to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Generate L1 overview from content.
    fn generate_l1(content: &str) -> String {
        let meaningful = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(5)
            .collect::<Vec<_>>()
            .join("\n");
        if meaningful.len() > 500 {
            meaningful[..500].to_string()
        } else {
            meaningful
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryCategory;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_tiered_content() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(ContextFs::new(temp_dir.path().join("context")));
        let memory = Arc::new(SqliteMemory::new_in_memory().unwrap());

        let path = ContextPath::new("user/memories/test.md");

        memory
            .store(
                &path.uri(), // Use URI format for consistency
                "# Test Memory\n\nThis is the full content of the test memory.",
                &MemoryCategory::Core,
            )
            .await
            .unwrap();

        let tiers = Tiers::new(fs.clone(), memory.clone(), 100);

        let l0 = tiers.get_l0(&path).await.unwrap();
        assert!(!l0.is_empty());
        assert!(l0.len() <= 100);

        let l1 = tiers.get_l1(&path).await.unwrap();
        assert!(!l1.is_empty());
        assert!(l1.len() <= 500);

        let tiered = tiers.get_tiered(path.clone()).await.unwrap();
        assert!(!tiered.is_l2_loaded());
        assert!(!tiered.l0_abstract.is_empty());
        assert!(!tiered.l1_overview.is_empty());
    }

    #[tokio::test]
    async fn test_tiered_l2_lazy_load() {
        let temp_dir = tempdir().unwrap();
        let fs = Arc::new(ContextFs::new(temp_dir.path().join("context")));
        let memory = Arc::new(SqliteMemory::new_in_memory().unwrap());

        let path = ContextPath::new("user/memories/lazy.md");
        let content = "# Lazy Content\n\nFull content for lazy loading test.";
        fs.write(&path, content).await.unwrap();

        memory
            .store(&path.uri(), content, &MemoryCategory::Core)
            .await
            .unwrap();

        let tiers = Tiers::new(fs.clone(), memory.clone(), 100);

        let mut tiered = tiers.get_tiered(path.clone()).await.unwrap();
        assert!(!tiered.is_l2_loaded());

        tiered.load_l2(&fs).await.unwrap();
        assert!(tiered.is_l2_loaded());
        assert!(tiered.l2_detail.is_some());
        assert!(tiered.l2_detail.unwrap().contains("Lazy Content"));
    }

    #[test]
    fn test_context_level_ordering() {
        assert!(ContextLevel::Abstract < ContextLevel::Overview);
        assert!(ContextLevel::Overview < ContextLevel::Detail);
        assert_eq!(ContextLevel::Abstract.name(), "L0");
        assert_eq!(ContextLevel::Overview.name(), "L1");
        assert_eq!(ContextLevel::Detail.name(), "L2");
    }

    #[test]
    fn test_tiered_content_estimate_tokens() {
        let tiered = TieredContent::new(
            ContextPath::new("test.md"),
            "Short abstract".to_string(),
            "This is a longer overview that contains more information about the content."
                .to_string(),
        );

        let l0_tokens = tiered.estimate_tokens(ContextLevel::Abstract);
        let l1_tokens = tiered.estimate_tokens(ContextLevel::Overview);

        assert!(l0_tokens < l1_tokens);
    }
}
