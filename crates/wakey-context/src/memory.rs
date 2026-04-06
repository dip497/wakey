//! Memory trait implementation with SQLite FTS5.
//!
//! Based on ZeroClaw Memory trait (store/recall/forget with hybrid search)
//! and OpenSpace quality metrics (selections/applied/completions/fallbacks).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{debug, instrument};

use wakey_types::WakeyResult;

/// Escape special characters for FTS5 search.
///
/// FTS5 interprets certain characters as operators: - + | & ! ( ) " ~ *
/// This escapes them to treat them as literal text.
fn escape_fts5(text: &str) -> String {
    let mut escaped = String::new();
    for c in text.chars() {
        match c {
            '-' | '+' | '|' | '&' | '!' | '(' | ')' | '"' | '~' | '*' | '\\' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    escaped
}

/// Memory category for classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCategory {
    Core,
    Daily,
    Conversation,
    Skill,
    Custom(String),
}

impl MemoryCategory {
    pub fn as_str(&self) -> &str {
        match self {
            MemoryCategory::Core => "core",
            MemoryCategory::Daily => "daily",
            MemoryCategory::Conversation => "conversation",
            MemoryCategory::Skill => "skill",
            MemoryCategory::Custom(s) => s.as_str(),
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "core" => MemoryCategory::Core,
            "daily" => MemoryCategory::Daily,
            "conversation" => MemoryCategory::Conversation,
            "skill" => MemoryCategory::Skill,
            other => MemoryCategory::Custom(other.to_string()),
        }
    }
}

/// A memory entry stored in the database.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub l0_abstract: String,
    pub l1_overview: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub mtime: i64,
    pub size: u64,
}

impl MemoryEntry {
    pub fn l0(&self) -> &str {
        &self.l0_abstract
    }

    pub fn l1(&self) -> &str {
        &self.l1_overview
    }

    pub fn detail(&self) -> &str {
        &self.content
    }
}

/// Skill quality metrics (OpenSpace pattern).
#[derive(Debug, Clone, Default)]
pub struct SkillMetrics {
    pub skill_id: String,
    pub name: String,
    pub total_selections: u32,
    pub total_applied: u32,
    pub total_completions: u32,
    pub total_fallbacks: u32,
    pub last_used: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl SkillMetrics {
    pub fn applied_rate(&self) -> f64 {
        if self.total_selections == 0 {
            0.0
        } else {
            self.total_applied as f64 / self.total_selections as f64
        }
    }

    pub fn completion_rate(&self) -> f64 {
        if self.total_applied == 0 {
            0.0
        } else {
            self.total_completions as f64 / self.total_applied as f64
        }
    }

    pub fn effective_rate(&self) -> f64 {
        if self.total_selections == 0 {
            0.0
        } else {
            self.total_completions as f64 / self.total_selections as f64
        }
    }
}

/// Skill lineage record (OpenSpace version DAG).
#[derive(Debug, Clone)]
pub struct SkillLineage {
    pub skill_id: String,
    pub parent_skill_id: String,
    pub evolution_type: String,
    pub change_summary: String,
}

/// ZeroClaw Memory trait — the interface for all memory operations.
#[async_trait]
pub trait Memory: Send + Sync {
    async fn store(&self, key: &str, content: &str, category: &MemoryCategory) -> WakeyResult<()>;
    async fn recall(&self, query: &str, limit: usize) -> WakeyResult<Vec<MemoryEntry>>;
    async fn get(&self, key: &str) -> WakeyResult<Option<MemoryEntry>>;
    async fn forget(&self, key: &str) -> WakeyResult<bool>;
    async fn list(&self, category: Option<&MemoryCategory>) -> WakeyResult<Vec<MemoryEntry>>;
    async fn record_skill_metrics(
        &self,
        skill_id: &str,
        applied: bool,
        completed: bool,
        fallback: bool,
    ) -> WakeyResult<()>;
    async fn get_skill_metrics(&self, skill_id: &str) -> WakeyResult<Option<SkillMetrics>>;
}

/// SQLite-backed memory implementation.
pub struct SqliteMemory {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl SqliteMemory {
    pub fn new(db_path: PathBuf) -> WakeyResult<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&db_path)?;
        Self::initialize_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path,
        })
    }

    pub fn new_in_memory() -> WakeyResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::initialize_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            db_path: PathBuf::from(":memory:"),
        })
    }

    fn initialize_schema(conn: &Connection) -> WakeyResult<()> {
        conn.execute_batch(
            r"
            CREATE VIRTUAL TABLE IF NOT EXISTS context_fts USING fts5(
                path, title, content, category, l0_abstract, l1_overview,
                tokenize = 'porter unicode61'
            );
            CREATE TABLE IF NOT EXISTS context_meta (
                path TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                mtime_secs INTEGER NOT NULL,
                size_bytes INTEGER NOT NULL,
                l0_abstract TEXT DEFAULT '',
                l1_overview TEXT DEFAULT '',
                title TEXT DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS skill_metrics (
                skill_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                total_selections INTEGER DEFAULT 0,
                total_applied INTEGER DEFAULT 0,
                total_completions INTEGER DEFAULT 0,
                total_fallbacks INTEGER DEFAULT 0,
                last_used TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS skill_lineage (
                skill_id TEXT NOT NULL,
                parent_skill_id TEXT NOT NULL,
                evolution_type TEXT NOT NULL,
                change_summary TEXT DEFAULT '',
                PRIMARY KEY (skill_id, parent_skill_id)
            );
            CREATE INDEX IF NOT EXISTS idx_category ON context_meta(category);
            ",
        )?;
        debug!("Initialized memory schema");
        Ok(())
    }

    fn now() -> String {
        Utc::now().to_rfc3339()
    }

    fn generate_l0(content: &str) -> String {
        let first_line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let trimmed = first_line.trim();
        if trimmed.len() > 100 {
            trimmed[..100].to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn generate_l1(content: &str) -> String {
        let meaningful: String = content
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

    fn extract_title(key: &str) -> String {
        key.rsplit('/')
            .next()
            .unwrap_or(key)
            .replace(".md", "")
            .to_string()
    }

    fn run<F, T>(&self, f: F) -> WakeyResult<T>
    where
        F: FnOnce(&Connection) -> WakeyResult<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| wakey_types::WakeyError::Memory(e.to_string()))?;
        f(&conn)
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    #[instrument(skip(self))]
    async fn store(&self, key: &str, content: &str, category: &MemoryCategory) -> WakeyResult<()> {
        let key = key.to_string();
        let content = content.to_string();
        let category = category.clone();
        let now = Self::now();
        let l0 = Self::generate_l0(&content);
        let l1 = Self::generate_l1(&content);
        let title = Self::extract_title(&key);

        self.run(|conn| {
            conn.execute(
                r"INSERT INTO context_meta (path, category, mtime_secs, size_bytes, l0_abstract, l1_overview, title, created_at, updated_at)
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                  ON CONFLICT(path) DO UPDATE SET
                    category = excluded.category, mtime_secs = excluded.mtime_secs, size_bytes = excluded.size_bytes,
                    l0_abstract = excluded.l0_abstract, l1_overview = excluded.l1_overview, title = excluded.title, updated_at = excluded.updated_at",
                params![key, category.as_str(), Utc::now().timestamp(), content.len() as u64, l0, l1, title, now, now],
            )?;
            conn.execute("DELETE FROM context_fts WHERE path = ?1", params![key])?;
            conn.execute("INSERT INTO context_fts (path, title, content, category, l0_abstract, l1_overview) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![key, escape_fts5(&title), escape_fts5(&content), category.as_str(), escape_fts5(&l0), escape_fts5(&l1)])?;
            debug!("Stored memory: {}", key);
            Ok(())
        })
    }

    #[instrument(skip(self))]
    async fn recall(&self, query: &str, limit: usize) -> WakeyResult<Vec<MemoryEntry>> {
        let query = query.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                r"SELECT fts.path, meta.title, meta.l0_abstract, meta.l1_overview, fts.content, meta.category,
                         meta.created_at, meta.updated_at, meta.mtime_secs, meta.size_bytes
                  FROM context_fts fts JOIN context_meta meta ON fts.path = meta.path
                  WHERE context_fts MATCH ?1 ORDER BY bm25(context_fts) LIMIT ?2",
            )?;
            let entries = stmt.query_map(params![escape_fts5(&query), limit as i32], |row| {
                Ok(MemoryEntry {
                    key: row.get(0)?, title: row.get(1)?, l0_abstract: row.get(2)?, l1_overview: row.get(3)?,
                    content: row.get(4)?, category: MemoryCategory::parse(&row.get::<_, String>(5)?),
                    created_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: row.get::<_, String>(7)?.parse().unwrap_or_else(|_| Utc::now()),
                    mtime: row.get(8)?, size: row.get(9)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?;
            debug!("Recalled {} memories", entries.len());
            Ok(entries)
        })
    }

    #[instrument(skip(self))]
    async fn get(&self, key: &str) -> WakeyResult<Option<MemoryEntry>> {
        let key = key.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT path, title, l0_abstract, l1_overview, category, created_at, updated_at, mtime_secs, size_bytes FROM context_meta WHERE path = ?1",
            )?;
            let meta = stmt.query_row(params![key], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?, row.get::<_, i64>(7)?, row.get::<_, u64>(8)?))
            }).optional()?;
            if let Some(meta) = meta {
                let content: Option<String> = conn.query_row("SELECT content FROM context_fts WHERE path = ?1", params![&key], |r| r.get(0)).optional()?;
                Ok(Some(MemoryEntry {
                    key: meta.0, title: meta.1, l0_abstract: meta.2, l1_overview: meta.3,
                    content: content.unwrap_or_default(), category: MemoryCategory::parse(&meta.4),
                    created_at: meta.5.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: meta.6.parse().unwrap_or_else(|_| Utc::now()), mtime: meta.7, size: meta.8,
                }))
            } else { Ok(None) }
        })
    }

    #[instrument(skip(self))]
    async fn forget(&self, key: &str) -> WakeyResult<bool> {
        let key = key.to_string();
        self.run(move |conn| {
            let fts = conn.execute("DELETE FROM context_fts WHERE path = ?1", params![key])?;
            let meta = conn.execute("DELETE FROM context_meta WHERE path = ?1", params![key])?;
            Ok(fts > 0 || meta > 0)
        })
    }

    #[instrument(skip(self))]
    async fn list(&self, category: Option<&MemoryCategory>) -> WakeyResult<Vec<MemoryEntry>> {
        let cat = category.map(|c| c.as_str().to_string());
        self.run(move |conn| {
            let sql = if cat.is_some() {
                "SELECT meta.path, meta.title, meta.l0_abstract, meta.l1_overview, meta.category, meta.created_at, meta.updated_at, meta.mtime_secs, meta.size_bytes, fts.content FROM context_meta meta LEFT JOIN context_fts fts ON meta.path = fts.path WHERE meta.category = ?1 ORDER BY meta.updated_at DESC"
            } else {
                "SELECT meta.path, meta.title, meta.l0_abstract, meta.l1_overview, meta.category, meta.created_at, meta.updated_at, meta.mtime_secs, meta.size_bytes, fts.content FROM context_meta meta LEFT JOIN context_fts fts ON meta.path = fts.path ORDER BY meta.updated_at DESC"
            };
            let mut stmt = conn.prepare(sql)?;
            let entries = if let Some(ref c) = cat {
                stmt.query_map(params![c], |row| Ok(MemoryEntry {
                    key: row.get(0)?, title: row.get(1)?, l0_abstract: row.get(2)?, l1_overview: row.get(3)?,
                    content: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                    category: MemoryCategory::parse(&row.get::<_, String>(4)?),
                    created_at: row.get::<_, String>(5)?.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                    mtime: row.get(7)?, size: row.get(8)?,
                }))?.collect::<Result<Vec<_>, _>>()?
            } else {
                stmt.query_map([], |row| Ok(MemoryEntry {
                    key: row.get(0)?, title: row.get(1)?, l0_abstract: row.get(2)?, l1_overview: row.get(3)?,
                    content: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
                    category: MemoryCategory::parse(&row.get::<_, String>(4)?),
                    created_at: row.get::<_, String>(5)?.parse().unwrap_or_else(|_| Utc::now()),
                    updated_at: row.get::<_, String>(6)?.parse().unwrap_or_else(|_| Utc::now()),
                    mtime: row.get(7)?, size: row.get(8)?,
                }))?.collect::<Result<Vec<_>, _>>()?
            };
            Ok(entries)
        })
    }

    #[instrument(skip(self))]
    async fn record_skill_metrics(
        &self,
        skill_id: &str,
        applied: bool,
        completed: bool,
        fallback: bool,
    ) -> WakeyResult<()> {
        let skill_id = skill_id.to_string();
        let now = Self::now();
        self.run(move |conn| {
            conn.execute("INSERT OR IGNORE INTO skill_metrics (skill_id, name, created_at) VALUES (?1, ?2, ?3)", params![skill_id, skill_id, now])?;
            conn.execute("UPDATE skill_metrics SET total_selections = total_selections + 1, total_applied = total_applied + ?1, total_completions = total_completions + ?2, total_fallbacks = total_fallbacks + ?3, last_used = ?4 WHERE skill_id = ?5",
                params![if applied { 1 } else { 0 }, if completed { 1 } else { 0 }, if fallback { 1 } else { 0 }, now, skill_id])?;
            Ok(())
        })
    }

    #[instrument(skip(self))]
    async fn get_skill_metrics(&self, skill_id: &str) -> WakeyResult<Option<SkillMetrics>> {
        let skill_id = skill_id.to_string();
        self.run(move |conn| {
            let mut stmt = conn.prepare("SELECT skill_id, name, total_selections, total_applied, total_completions, total_fallbacks, last_used, created_at FROM skill_metrics WHERE skill_id = ?1")?;
            Ok(stmt.query_row(params![skill_id], |row| Ok(SkillMetrics {
                skill_id: row.get(0)?, name: row.get(1)?, total_selections: row.get(2)?, total_applied: row.get(3)?,
                total_completions: row.get(4)?, total_fallbacks: row.get(5)?,
                last_used: row.get::<_, Option<String>>(6)?.map(|s| s.parse().unwrap_or_else(|_| Utc::now())),
                created_at: row.get::<_, String>(7)?.parse().unwrap_or_else(|_| Utc::now()),
            })).optional()?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_store_and_recall() {
        let memory = SqliteMemory::new_in_memory().unwrap();
        memory
            .store(
                "user/memories/test.md",
                "This is a test memory about preferences.",
                &MemoryCategory::Core,
            )
            .await
            .unwrap();
        let results = memory.recall("test", 10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_get_and_forget() {
        let memory = SqliteMemory::new_in_memory().unwrap();
        memory
            .store("test.md", "content", &MemoryCategory::Daily)
            .await
            .unwrap();
        assert!(memory.get("test.md").await.unwrap().is_some());
        assert!(memory.forget("test.md").await.unwrap());
        assert!(memory.get("test.md").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_skill_metrics() {
        let memory = SqliteMemory::new_in_memory().unwrap();
        memory
            .record_skill_metrics("test-skill", true, true, false)
            .await
            .unwrap();
        memory
            .record_skill_metrics("test-skill", true, false, true)
            .await
            .unwrap();
        let m = memory
            .get_skill_metrics("test-skill")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(m.total_selections, 2);
        assert_eq!(m.total_completions, 1);
    }
}
