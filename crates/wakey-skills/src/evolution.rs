//! Skill evolution — OpenSpace FIX/DERIVED/CAPTURED pattern
//!
//! Manages skill versioning and lineage tracking.
//! Evolution creates new skill versions while preserving history.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use wakey_types::{WakeyError, WakeyResult};

use crate::format::SkillManifest;

/// Evolution type — how a skill was created/modified
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvolutionType {
    /// In-place repair of broken skill
    Fix,

    /// New skill enhanced from existing
    Derived,

    /// Brand new skill captured from execution pattern
    Captured,
}

impl std::fmt::Display for EvolutionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvolutionType::Fix => write!(f, "fix"),
            EvolutionType::Derived => write!(f, "derived"),
            EvolutionType::Captured => write!(f, "captured"),
        }
    }
}

/// Skill origin — where the skill came from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillOrigin {
    /// Initially imported (no parent)
    Imported,

    /// Captured from execution
    Captured,

    /// Derived from existing skill
    Derived,

    /// Fixed version of existing skill
    Fixed,
}

impl std::fmt::Display for SkillOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillOrigin::Imported => write!(f, "imported"),
            SkillOrigin::Captured => write!(f, "captured"),
            SkillOrigin::Derived => write!(f, "derived"),
            SkillOrigin::Fixed => write!(f, "fixed"),
        }
    }
}

/// Skill lineage record
#[derive(Debug, Clone)]
pub struct SkillLineage {
    /// Unique skill ID
    pub skill_id: String,

    /// Skill name
    pub name: String,

    /// Origin type
    pub origin: SkillOrigin,

    /// Generation number (0 = original, 1 = first evolution, etc.)
    pub generation: u32,

    /// Parent skill IDs (for DERIVED, can be multiple)
    pub parent_ids: Vec<String>,

    /// Task that triggered this evolution
    pub source_task_id: Option<String>,

    /// LLM-generated change summary
    pub change_summary: String,

    /// Content diff (unified format)
    pub content_diff: String,

    /// Who created this (model name or "human")
    pub created_by: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// Evolution table name
const LINEAGE_TABLE: &str = "skill_lineage";

/// Lineage parents table (many-to-many)
const PARENTS_TABLE: &str = "skill_lineage_parents";

/// Initialize evolution schema
pub fn init_evolution_schema(conn: &Connection) -> WakeyResult<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            skill_id            TEXT PRIMARY KEY,
            name                TEXT NOT NULL,
            origin              TEXT NOT NULL,
            generation          INTEGER NOT NULL DEFAULT 0,
            source_task_id      TEXT,
            change_summary      TEXT NOT NULL DEFAULT '',
            content_diff        TEXT NOT NULL DEFAULT '',
            created_by          TEXT NOT NULL DEFAULT '',
            created_at          TEXT NOT NULL DEFAULT '',
            is_active           INTEGER NOT NULL DEFAULT 1
        );
        
        CREATE TABLE IF NOT EXISTS {} (
            skill_id        TEXT NOT NULL,
            parent_skill_id TEXT NOT NULL,
            PRIMARY KEY (skill_id, parent_skill_id)
        );
        
        CREATE INDEX IF NOT EXISTS idx_lineage_name ON {}(name);
        CREATE INDEX IF NOT EXISTS idx_lineage_origin ON {}(origin);
        ",
        LINEAGE_TABLE, PARENTS_TABLE, LINEAGE_TABLE, LINEAGE_TABLE
    ))
    .map_err(|e| WakeyError::Skill {
        skill: "evolution".into(),
        message: format!("Failed to create lineage tables: {}", e),
    })?;

    debug!("Initialized evolution schema");
    Ok(())
}

/// Skill evolver — creates and tracks skill versions
pub struct SkillEvolver {
    /// SQLite connection
    conn: Connection,

    /// Skills directory
    skills_dir: PathBuf,
}

impl SkillEvolver {
    /// Create a new evolver
    pub fn new(conn: Connection, skills_dir: &Path) -> WakeyResult<Self> {
        init_evolution_schema(&conn)?;
        Ok(Self {
            conn,
            skills_dir: skills_dir.to_path_buf(),
        })
    }

    /// Evolve a skill — create new version
    ///
    /// # Arguments
    /// * `skill_id` - Existing skill to evolve (or None for Captured)
    /// * `evolution_type` - How to evolve
    /// * `new_content` - New SKILL.md content
    /// * `change_summary` - Description of changes
    /// * `source_task_id` - Task that triggered evolution
    /// * `created_by` - Model name or "human"
    ///
    /// # Returns
    /// New skill ID
    #[allow(clippy::too_many_arguments)]
    pub fn evolve(
        &self,
        skill_id: Option<&str>,
        evolution_type: EvolutionType,
        new_content: &str,
        change_summary: &str,
        source_task_id: Option<&str>,
        created_by: &str,
    ) -> WakeyResult<String> {
        let now = Utc::now();

        match evolution_type {
            EvolutionType::Fix => {
                // FIX: Same skill, new version, deactivate old
                self.evolve_fix(
                    skill_id,
                    new_content,
                    change_summary,
                    source_task_id,
                    created_by,
                    now,
                )
            }
            EvolutionType::Derived => {
                // DERIVED: New skill from existing
                self.evolve_derived(
                    skill_id,
                    new_content,
                    change_summary,
                    source_task_id,
                    created_by,
                    now,
                )
            }
            EvolutionType::Captured => {
                // CAPTURED: Brand new skill
                self.evolve_captured(new_content, change_summary, source_task_id, created_by, now)
            }
        }
    }

    /// FIX evolution — repair existing skill
    #[allow(clippy::too_many_arguments)]
    fn evolve_fix(
        &self,
        skill_id: Option<&str>,
        new_content: &str,
        change_summary: &str,
        source_task_id: Option<&str>,
        created_by: &str,
        now: DateTime<Utc>,
    ) -> WakeyResult<String> {
        let skill_id = skill_id.ok_or_else(|| WakeyError::Skill {
            skill: "evolution".into(),
            message: "FIX evolution requires skill_id".into(),
        })?;

        // Get current skill info
        let (name, generation) = self.get_skill_info(skill_id)?;

        // Write new content to skill file
        let skill_dir = self.skills_dir.join(&name);
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, new_content).map_err(|e| WakeyError::Skill {
            skill: name.clone(),
            message: format!("Failed to write skill file: {}", e),
        })?;

        // Create new version record
        let new_skill_id = format!(
            "{}__fix_{}",
            name,
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("unknown")
        );

        // Compute diff
        let diff = format!("FIX: {}", change_summary);

        // Insert lineage record
        self.insert_lineage(&SkillLineage {
            skill_id: new_skill_id.clone(),
            name: name.clone(),
            origin: SkillOrigin::Fixed,
            generation: generation + 1,
            parent_ids: vec![skill_id.to_string()],
            source_task_id: source_task_id.map(String::from),
            change_summary: change_summary.to_string(),
            content_diff: diff,
            created_by: created_by.to_string(),
            created_at: now,
        })?;

        // Deactivate old version
        self.deactivate_skill(skill_id)?;

        info!(old_id = %skill_id, new_id = %new_skill_id, "Fixed skill");
        Ok(new_skill_id)
    }

    /// DERIVED evolution — create new skill from existing
    #[allow(clippy::too_many_arguments)]
    fn evolve_derived(
        &self,
        skill_id: Option<&str>,
        new_content: &str,
        change_summary: &str,
        source_task_id: Option<&str>,
        created_by: &str,
        now: DateTime<Utc>,
    ) -> WakeyResult<String> {
        let skill_id = skill_id.ok_or_else(|| WakeyError::Skill {
            skill: "evolution".into(),
            message: "DERIVED evolution requires skill_id".into(),
        })?;

        // Get parent skill info
        let (_parent_name, _) = self.get_skill_info(skill_id)?;

        // Parse new content to get name
        let new_manifest = self.parse_content_manifest(new_content)?;

        // Create new skill directory
        let new_skill_dir = self.skills_dir.join(&new_manifest.name);
        fs::create_dir_all(&new_skill_dir).map_err(|e| WakeyError::Skill {
            skill: new_manifest.name.clone(),
            message: format!("Failed to create skill directory: {}", e),
        })?;

        // Write new skill file
        let skill_file = new_skill_dir.join("SKILL.md");
        fs::write(&skill_file, new_content).map_err(|e| WakeyError::Skill {
            skill: new_manifest.name.clone(),
            message: format!("Failed to write skill file: {}", e),
        })?;

        // Create new skill ID
        let new_skill_id = format!(
            "{}__drv_{}",
            new_manifest.name,
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("unknown")
        );

        // Insert lineage record
        self.insert_lineage(&SkillLineage {
            skill_id: new_skill_id.clone(),
            name: new_manifest.name.clone(),
            origin: SkillOrigin::Derived,
            generation: 0, // New skill, generation 0
            parent_ids: vec![skill_id.to_string()],
            source_task_id: source_task_id.map(String::from),
            change_summary: change_summary.to_string(),
            content_diff: String::new(), // DERIVED doesn't have diff
            created_by: created_by.to_string(),
            created_at: now,
        })?;

        info!(parent = %skill_id, new_id = %new_skill_id, "Derived new skill");
        Ok(new_skill_id)
    }

    /// CAPTURED evolution — create brand new skill
    fn evolve_captured(
        &self,
        new_content: &str,
        change_summary: &str,
        source_task_id: Option<&str>,
        created_by: &str,
        now: DateTime<Utc>,
    ) -> WakeyResult<String> {
        // Parse content to get name
        let manifest = self.parse_content_manifest(new_content)?;

        // Create skill directory
        let skill_dir = self.skills_dir.join(&manifest.name);
        fs::create_dir_all(&skill_dir).map_err(|e| WakeyError::Skill {
            skill: manifest.name.clone(),
            message: format!("Failed to create skill directory: {}", e),
        })?;

        // Write skill file
        let skill_file = skill_dir.join("SKILL.md");
        fs::write(&skill_file, new_content).map_err(|e| WakeyError::Skill {
            skill: manifest.name.clone(),
            message: format!("Failed to write skill file: {}", e),
        })?;

        // Create skill ID
        let skill_id = format!(
            "{}__cap_{}",
            manifest.name,
            uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("unknown")
        );

        // Insert lineage record
        self.insert_lineage(&SkillLineage {
            skill_id: skill_id.clone(),
            name: manifest.name.clone(),
            origin: SkillOrigin::Captured,
            generation: 0,
            parent_ids: vec![],
            source_task_id: source_task_id.map(String::from),
            change_summary: change_summary.to_string(),
            content_diff: String::new(),
            created_by: created_by.to_string(),
            created_at: now,
        })?;

        info!(new_id = %skill_id, "Captured new skill");
        Ok(skill_id)
    }

    /// Get skill name and generation by ID
    fn get_skill_info(&self, skill_id: &str) -> WakeyResult<(String, u32)> {
        let result = self.conn.query_row(
            &format!(
                "SELECT name, generation FROM {} WHERE skill_id = ?1",
                LINEAGE_TABLE
            ),
            params![skill_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)),
        );

        match result {
            Ok(info) => Ok(info),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Not in lineage table yet — try from main skills table
                self.conn
                    .query_row(
                        "SELECT name FROM skills WHERE skill_id = ?1",
                        params![skill_id],
                        |row| Ok((row.get::<_, String>(0)?, 0u32)),
                    )
                    .map_err(|e| WakeyError::Skill {
                        skill: skill_id.into(),
                        message: format!("Skill not found: {}", e),
                    })
            }
            Err(e) => Err(WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to get skill info: {}", e),
            }),
        }
    }

    /// Insert lineage record
    fn insert_lineage(&self, lineage: &SkillLineage) -> WakeyResult<()> {
        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (skill_id, name, origin, generation, 
                 source_task_id, change_summary, content_diff, created_by, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    LINEAGE_TABLE
                ),
                params![
                    &lineage.skill_id,
                    &lineage.name,
                    &lineage.origin.to_string(),
                    lineage.generation as i64,
                    &lineage.source_task_id,
                    &lineage.change_summary,
                    &lineage.content_diff,
                    &lineage.created_by,
                    &lineage.created_at.to_rfc3339(),
                ],
            )
            .map_err(|e| WakeyError::Skill {
                skill: lineage.skill_id.clone(),
                message: format!("Failed to insert lineage: {}", e),
            })?;

        // Insert parent relationships
        for parent_id in &lineage.parent_ids {
            self.conn
                .execute(
                    &format!(
                        "INSERT INTO {} (skill_id, parent_skill_id) VALUES (?1, ?2)",
                        PARENTS_TABLE
                    ),
                    params![&lineage.skill_id, parent_id],
                )
                .map_err(|e| WakeyError::Skill {
                    skill: lineage.skill_id.clone(),
                    message: format!("Failed to insert parent: {}", e),
                })?;
        }

        Ok(())
    }

    /// Deactivate a skill version
    fn deactivate_skill(&self, skill_id: &str) -> WakeyResult<()> {
        self.conn
            .execute(
                &format!(
                    "UPDATE {} SET is_active = 0 WHERE skill_id = ?1",
                    LINEAGE_TABLE
                ),
                params![skill_id],
            )
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to deactivate skill: {}", e),
            })?;

        Ok(())
    }

    /// Parse manifest from SKILL.md content
    fn parse_content_manifest(&self, content: &str) -> WakeyResult<SkillManifest> {
        // Simple YAML frontmatter extraction
        let content = content.trim();

        if !content.starts_with("---") {
            return Err(WakeyError::Skill {
                skill: "evolution".into(),
                message: "Missing YAML frontmatter".into(),
            });
        }

        let end = content[3..].find("---").ok_or_else(|| WakeyError::Skill {
            skill: "evolution".into(),
            message: "Malformed YAML frontmatter".into(),
        })?;

        let yaml = &content[3..end + 3];

        serde_yaml::from_str(yaml).map_err(|e| WakeyError::Skill {
            skill: "evolution".into(),
            message: format!("Invalid YAML frontmatter: {}", e),
        })
    }

    /// Get lineage history for a skill
    pub fn get_lineage(&self, skill_id: &str) -> WakeyResult<Option<SkillLineage>> {
        let result = self.conn.query_row(
            &format!(
                "SELECT skill_id, name, origin, generation, source_task_id,
                        change_summary, content_diff, created_by, created_at
                 FROM {} WHERE skill_id = ?1",
                LINEAGE_TABLE
            ),
            params![skill_id],
            |row| {
                let origin_str: String = row.get(2)?;
                let origin = match origin_str.as_str() {
                    "imported" => SkillOrigin::Imported,
                    "captured" => SkillOrigin::Captured,
                    "derived" => SkillOrigin::Derived,
                    "fixed" => SkillOrigin::Fixed,
                    _ => SkillOrigin::Imported,
                };

                Ok(SkillLineage {
                    skill_id: row.get(0)?,
                    name: row.get(1)?,
                    origin,
                    generation: row.get::<_, i64>(3)? as u32,
                    parent_ids: vec![], // Loaded separately
                    source_task_id: row.get(4)?,
                    change_summary: row.get(5)?,
                    content_diff: row.get(6)?,
                    created_by: row.get(7)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            },
        );

        match result {
            Ok(mut lineage) => {
                // Load parents
                let parents = self.get_parents(skill_id)?;
                lineage.parent_ids = parents;
                Ok(Some(lineage))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to get lineage: {}", e),
            }),
        }
    }

    /// Get parent skill IDs
    fn get_parents(&self, skill_id: &str) -> WakeyResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT parent_skill_id FROM {} WHERE skill_id = ?1",
                PARENTS_TABLE
            ))
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to prepare parents query: {}", e),
            })?;

        let parents: Vec<String> = stmt
            .query_map(params![skill_id], |row| row.get(0))
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to query parents: {}", e),
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(parents)
    }

    /// Get all versions of a skill (by name)
    pub fn get_versions(&self, name: &str) -> WakeyResult<Vec<SkillLineage>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT skill_id, name, origin, generation, source_task_id,
                    change_summary, content_diff, created_by, created_at
             FROM {} WHERE name = ?1
             ORDER BY generation DESC",
                LINEAGE_TABLE
            ))
            .map_err(|e| WakeyError::Skill {
                skill: name.into(),
                message: format!("Failed to prepare versions query: {}", e),
            })?;

        let versions: Vec<SkillLineage> = stmt
            .query_map(params![name], |row| {
                let origin_str: String = row.get(2)?;
                let origin = match origin_str.as_str() {
                    "imported" => SkillOrigin::Imported,
                    "captured" => SkillOrigin::Captured,
                    "derived" => SkillOrigin::Derived,
                    "fixed" => SkillOrigin::Fixed,
                    _ => SkillOrigin::Imported,
                };

                Ok(SkillLineage {
                    skill_id: row.get(0)?,
                    name: row.get(1)?,
                    origin,
                    generation: row.get::<_, i64>(3)? as u32,
                    parent_ids: vec![],
                    source_task_id: row.get(4)?,
                    change_summary: row.get(5)?,
                    content_diff: row.get(6)?,
                    created_by: row.get(7)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(8)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .map_err(|e| WakeyError::Skill {
                skill: name.into(),
                message: format!("Failed to query versions: {}", e),
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(versions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_evolver() -> (SkillEvolver, TempDir) {
        let temp = TempDir::new().expect("Temp dir");
        let db_path = temp.path().join("evolution.db");
        let conn = Connection::open(&db_path).expect("Open db");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).expect("Skills dir");

        let evolver = SkillEvolver::new(conn, &skills_dir).expect("Evolver");
        (evolver, temp)
    }

    fn make_skill_content(name: &str) -> String {
        format!(
            "---\nname: {}\ndescription: Test skill\n---\n# {}\n\nTest content",
            name, name
        )
    }

    #[test]
    fn test_evolve_captured() {
        let (evolver, _temp) = make_evolver();

        let content = make_skill_content("new-skill");
        let skill_id = evolver
            .evolve(
                None,
                EvolutionType::Captured,
                &content,
                "Created from execution pattern",
                Some("task-123"),
                "gpt-4",
            )
            .expect("Evolve");

        assert!(skill_id.starts_with("new-skill__cap_"));

        let lineage = evolver.get_lineage(&skill_id).expect("Get lineage");
        assert!(lineage.is_some());
        let lineage = lineage.unwrap();
        assert_eq!(lineage.origin, SkillOrigin::Captured);
        assert_eq!(lineage.generation, 0);
        assert!(lineage.parent_ids.is_empty());
    }

    #[test]
    fn test_evolution_type_display() {
        assert_eq!(EvolutionType::Fix.to_string(), "fix");
        assert_eq!(EvolutionType::Derived.to_string(), "derived");
        assert_eq!(EvolutionType::Captured.to_string(), "captured");
    }

    #[test]
    fn test_skill_origin_display() {
        assert_eq!(SkillOrigin::Imported.to_string(), "imported");
        assert_eq!(SkillOrigin::Captured.to_string(), "captured");
        assert_eq!(SkillOrigin::Derived.to_string(), "derived");
        assert_eq!(SkillOrigin::Fixed.to_string(), "fixed");
    }
}
