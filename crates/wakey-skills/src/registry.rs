//! Skill registry — SQLite indexing + FTS5 search + directory scan
//!
//! Manages skill discovery, indexing, and retrieval.
//! Uses SQLite for metadata storage and FTS5 for full-text search.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

use wakey_types::{WakeyError, WakeyResult};

use crate::format::{
    SkillContent, SkillManifest, generate_abstract, generate_overview, parse_skill,
};

/// Skill match result from search
#[derive(Debug, Clone)]
pub struct SkillMatch {
    /// Skill name
    pub name: String,

    /// Description
    pub description: String,

    /// Relevance score (BM25)
    pub score: f64,

    /// L1 overview for selection context
    pub overview: String,
}

/// SQLite-backed skill registry
pub struct SkillRegistry {
    /// Skills directory (e.g., ~/.wakey/context/agent/skills/)
    skills_dir: PathBuf,

    /// SQLite connection for metadata + FTS5
    conn: Connection,

    /// Cached skills (L0 manifests only, loaded on scan)
    cache: Vec<SkillManifest>,

    /// Last scan timestamp for auto-rescan check
    last_scan: u64,
}

/// FTS5 table name
const FTS_TABLE: &str = "skills_fts";

/// Main skills table name
const MAIN_TABLE: &str = "skills";

/// Create a new skill registry
///
/// # Arguments
/// * `skills_dir` - Directory containing skill subdirectories with SKILL.md
/// * `index_db` - Path to SQLite database file (created if missing)
///
/// # Errors
/// Returns error if database can't be created or schema migration fails
pub fn new(skills_dir: &Path, index_db: &Path) -> WakeyResult<SkillRegistry> {
    // Ensure parent directory exists
    if let Some(parent) = index_db.parent() {
        fs::create_dir_all(parent).map_err(|e| WakeyError::Skill {
            skill: "registry".into(),
            message: format!("Failed to create db directory: {}", e),
        })?;
    }

    // Open or create database
    let conn = Connection::open(index_db).map_err(|e| WakeyError::Skill {
        skill: "registry".into(),
        message: format!("Failed to open database: {}", e),
    })?;

    // Initialize schema
    init_schema(&conn)?;

    Ok(SkillRegistry {
        skills_dir: skills_dir.to_path_buf(),
        conn,
        cache: Vec::new(),
        last_scan: 0,
    })
}

/// Initialize SQLite schema
fn init_schema(conn: &Connection) -> WakeyResult<()> {
    // Main skills table
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            skill_id     TEXT PRIMARY KEY,
            name         TEXT NOT NULL UNIQUE,
            description  TEXT NOT NULL,
            version      TEXT NOT NULL DEFAULT '1.0.0',
            dependencies TEXT NOT NULL DEFAULT '[]',
            tags         TEXT NOT NULL DEFAULT '[]',
            platforms    TEXT NOT NULL DEFAULT '[]',
            abstract     TEXT NOT NULL DEFAULT '',
            overview     TEXT NOT NULL DEFAULT '',
            body         TEXT NOT NULL DEFAULT '',
            source_path  TEXT NOT NULL,
            mtime        INTEGER NOT NULL DEFAULT 0,
            is_active    INTEGER NOT NULL DEFAULT 1,
            created_at   INTEGER NOT NULL DEFAULT 0,
            updated_at   INTEGER NOT NULL DEFAULT 0
        );
        
        -- FTS5 virtual table for full-text search
        CREATE VIRTUAL TABLE IF NOT EXISTS {} USING fts5(
            name,
            description,
            abstract,
            overview,
            tags,
            content='skills',
            content_rowid='rowid'
        );
        
        -- Triggers to keep FTS5 in sync
        CREATE TRIGGER IF NOT EXISTS skills_fts_insert AFTER INSERT ON skills BEGIN
            INSERT INTO skills_fts(rowid, name, description, abstract, overview, tags)
            VALUES (new.rowid, new.name, new.description, new.abstract, new.overview, new.tags);
        END;
        
        CREATE TRIGGER IF NOT EXISTS skills_fts_delete AFTER DELETE ON skills BEGIN
            INSERT INTO skills_fts(skills_fts, rowid, name, description, abstract, overview, tags)
            VALUES ('delete', old.rowid, old.name, old.description, old.abstract, old.overview, old.tags);
        END;
        
        CREATE TRIGGER IF NOT EXISTS skills_fts_update AFTER UPDATE ON skills BEGIN
            INSERT INTO skills_fts(skills_fts, rowid, name, description, abstract, overview, tags)
            VALUES ('delete', old.rowid, old.name, old.description, old.abstract, old.overview, old.tags);
            INSERT INTO skills_fts(rowid, name, description, abstract, overview, tags)
            VALUES (new.rowid, new.name, new.description, new.abstract, new.overview, new.tags);
        END;
        
        -- Index for dependency lookups
        CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name);
        ",
        MAIN_TABLE, FTS_TABLE
    ))
    .map_err(|e| WakeyError::Skill {
        skill: "registry".into(),
        message: format!("Failed to initialize schema: {}", e),
    })?;

    debug!("Initialized skill registry schema");
    Ok(())
}

impl SkillRegistry {
    /// Scan skills directory and index all SKILL.md files
    ///
    /// Walks the skills directory, parses each SKILL.md, and inserts/updates
    /// records in SQLite. Automatically skips invalid skills.
    ///
    /// # Returns
    /// Number of skills successfully indexed
    pub fn scan(&mut self) -> WakeyResult<usize> {
        info!(dir = %self.skills_dir.display(), "Scanning skills directory");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before epoch")
            .as_secs();

        let mut count = 0;
        let mut manifests = Vec::new();

        // Walk directory looking for SKILL.md files
        for entry in WalkDir::new(&self.skills_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Look for SKILL.md files
            if path.file_name().map(|n| n == "SKILL.md").unwrap_or(false) {
                // Parse and index
                match self.index_skill(path, now) {
                    Ok(manifest) => {
                        count += 1;
                        manifests.push(manifest);
                    }
                    Err(e) => {
                        warn!(error = %e, path = %path.display(), "Failed to index skill");
                    }
                }
            }
        }

        self.cache = manifests;
        self.last_scan = now;

        info!(count = count, "Skill scan complete");
        Ok(count)
    }

    /// Index a single skill file
    fn index_skill(&self, path: &Path, now: u64) -> WakeyResult<SkillManifest> {
        let content = parse_skill(path)?;

        // Generate L0/L1 abstractions
        let skill_abstract = generate_abstract(&content);
        let overview = generate_overview(&content);

        // Serialize JSON fields
        let deps_json = serde_json::to_string(&content.manifest.dependencies)
            .expect("Dependencies serialization");
        let tags_json = serde_json::to_string(&content.manifest.tags).expect("Tags serialization");
        let platforms_json =
            serde_json::to_string(&content.manifest.platforms).expect("Platforms serialization");

        // Generate unique skill ID (name + short UUID)
        let skill_id = format!(
            "{}__skl_{}",
            content.manifest.name,
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("unknown")
        );

        // Insert or replace (upsert)
        self.conn
            .execute(
                &format!(
                    "INSERT OR REPLACE INTO {} (
                    skill_id, name, description, version,
                    dependencies, tags, platforms,
                    abstract, overview, body,
                    source_path, mtime,
                    is_active, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 1, ?13, ?13)",
                    MAIN_TABLE
                ),
                params![
                    &skill_id,
                    &content.manifest.name,
                    &content.manifest.description,
                    &content.manifest.version,
                    &deps_json,
                    &tags_json,
                    &platforms_json,
                    &skill_abstract,
                    &overview,
                    &content.body,
                    &content.source_path,
                    content.mtime,
                    now as i64,
                ],
            )
            .map_err(|e| WakeyError::Skill {
                skill: content.manifest.name.clone(),
                message: format!("Failed to insert skill: {}", e),
            })?;

        debug!(name = %content.manifest.name, "Indexed skill");
        Ok(content.manifest)
    }

    /// Full-text search for skills by query
    ///
    /// Uses FTS5 BM25 ranking. Returns matches with L1 overview
    /// for selection context (not full L2 body).
    ///
    /// # Arguments
    /// * `query` - Search query (keyword-based)
    /// * `limit` - Maximum results to return
    ///
    /// # Returns
    /// Vec of SkillMatch sorted by relevance score
    pub fn find(&self, query: &str, limit: usize) -> WakeyResult<Vec<SkillMatch>> {
        // FTS5 search with BM25 ranking
        let sql = format!(
            "SELECT name, description, overview, bm25({}) as score
             FROM {} 
             WHERE {} MATCH ?1
             ORDER BY score DESC
             LIMIT ?2",
            FTS_TABLE, FTS_TABLE, FTS_TABLE
        );

        let mut stmt = self.conn.prepare(&sql).map_err(|e| WakeyError::Skill {
            skill: "registry".into(),
            message: format!("Failed to prepare search: {}", e),
        })?;

        let matches: Vec<SkillMatch> = stmt
            .query_map(params![query, limit as i64], |row| {
                Ok(SkillMatch {
                    name: row.get(0)?,
                    description: row.get(1)?,
                    overview: row.get(2)?,
                    score: row.get::<_, f64>(3)?,
                })
            })
            .map_err(|e| WakeyError::Skill {
                skill: "registry".into(),
                message: format!("Failed to execute search: {}", e),
            })?
            .filter_map(|m| m.ok())
            .collect();

        debug!(query = %query, count = matches.len(), "FTS5 search complete");
        Ok(matches)
    }

    /// Get a specific skill by name (full L2 content)
    ///
    /// Loads the complete skill from SQLite, including full body.
    ///
    /// # Arguments
    /// * `name` - Skill name (exact match)
    ///
    /// # Returns
    /// Full SkillContent if found, None otherwise
    pub fn get(&self, name: &str) -> WakeyResult<Option<SkillContent>> {
        let sql = format!(
            "SELECT name, description, version, dependencies, tags, platforms,
                    abstract, overview, body, source_path, mtime
             FROM {} WHERE name = ?1 AND is_active = 1",
            MAIN_TABLE
        );

        let mut stmt = self.conn.prepare(&sql).map_err(|e| WakeyError::Skill {
            skill: "registry".into(),
            message: format!("Failed to prepare get: {}", e),
        })?;

        let result = stmt.query_row(params![name], |row| {
            let deps_json: String = row.get(3)?;
            let tags_json: String = row.get(4)?;
            let platforms_json: String = row.get(5)?;

            let deps: Vec<String> = serde_json::from_str(&deps_json).unwrap_or_default();
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let platforms: Vec<String> = serde_json::from_str(&platforms_json).unwrap_or_default();

            Ok(SkillContent {
                manifest: SkillManifest {
                    name: row.get(0)?,
                    description: row.get(1)?,
                    version: row.get(2)?,
                    dependencies: deps,
                    tags,
                    platforms,
                },
                body: row.get(8)?,
                source_path: row.get(9)?,
                mtime: row.get::<_, i64>(10)? as u64,
            })
        });

        match result {
            Ok(content) => Ok(Some(content)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(WakeyError::Skill {
                skill: name.into(),
                message: format!("Failed to get skill: {}", e),
            }),
        }
    }

    /// List all registered skills (L0 manifests only)
    ///
    /// Returns cached manifests from last scan. Use for skill catalog
    /// building without loading full content.
    pub fn list(&self) -> Vec<SkillManifest> {
        self.cache.clone()
    }

    /// Check if rescan is needed based on file modification times
    ///
    /// Compares current file mtimes against stored mtimes.
    /// Returns true if any skill file changed since last scan.
    pub fn needs_rescan(&self) -> bool {
        // Check a few sample files
        for entry in WalkDir::new(&self.skills_dir)
            .follow_links(true)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry
                .path()
                .file_name()
                .map(|n| n == "SKILL.md")
                .unwrap_or(false)
                && let Ok(metadata) = fs::metadata(entry.path())
                && let Ok(mtime) = metadata.modified()
            {
                let mtime_secs = mtime
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                if mtime_secs > self.last_scan {
                    debug!(path = %entry.path().display(), "Skill file modified, needs rescan");
                    return true;
                }
            }
        }
        false
    }

    /// Refresh cache if files changed since last scan
    pub fn refresh_if_needed(&mut self) -> WakeyResult<usize> {
        if self.needs_rescan() {
            self.scan()
        } else {
            Ok(self.cache.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_skill(dir: &Path, name: &str, description: &str) -> PathBuf {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).expect("Create skill dir");

        let skill_md = skill_dir.join("SKILL.md");
        let content = format!(
            "---\nname: {}\ndescription: {}\n---\n# {}\n\n## When to Use\nTest skill.\n",
            name, description, name
        );
        fs::write(&skill_md, content).expect("Write skill");
        skill_md
    }

    #[test]
    fn test_registry_scan() {
        let temp = TempDir::new().expect("Temp dir");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).expect("Skills dir");

        let db_path = temp.path().join("skills.db");

        make_test_skill(&skills_dir, "test-skill", "A test skill");
        make_test_skill(&skills_dir, "deploy-app", "Deploy to production");

        let mut registry = new(&skills_dir, &db_path).expect("Registry");
        let count = registry.scan().expect("Scan");

        assert_eq!(count, 2);
        assert_eq!(registry.list().len(), 2);
    }

    #[test]
    fn test_registry_find() {
        let temp = TempDir::new().expect("Temp dir");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).expect("Skills dir");

        let db_path = temp.path().join("skills.db");

        make_test_skill(
            &skills_dir,
            "deploy-app",
            "Deploy application to production",
        );
        make_test_skill(&skills_dir, "fix-lint", "Fix linting errors in code");

        let mut registry = new(&skills_dir, &db_path).expect("Registry");
        registry.scan().expect("Scan");

        let matches = registry.find("deploy", 5).expect("Find");
        assert!(!matches.is_empty());
        assert!(matches[0].name == "deploy-app");
    }

    #[test]
    fn test_registry_get() {
        let temp = TempDir::new().expect("Temp dir");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).expect("Skills dir");

        let db_path = temp.path().join("skills.db");

        make_test_skill(&skills_dir, "test-skill", "A test skill");

        let mut registry = new(&skills_dir, &db_path).expect("Registry");
        registry.scan().expect("Scan");

        let skill = registry.get("test-skill").expect("Get");
        assert!(skill.is_some());
        assert_eq!(skill.unwrap().manifest.name, "test-skill");

        let missing = registry.get("nonexistent").expect("Get missing");
        assert!(missing.is_none());
    }
}
