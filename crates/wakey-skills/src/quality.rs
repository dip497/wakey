//! Quality tracking — OpenSpace pattern with SQLite persistence
//!
//! Tracks skill selection, application, completion, and fallback rates.
//! Quality metrics drive evolution triggers and skill ranking.

use rusqlite::{Connection, params};
use tracing::debug;

use wakey_types::{WakeyError, WakeyResult};

/// Skill quality metrics
#[derive(Debug, Clone, Default)]
pub struct SkillMetrics {
    /// Times skill was selected by LLM
    pub total_selections: u64,

    /// Times skill was actually applied
    pub total_applied: u64,

    /// Times task completed successfully with this skill
    pub total_completions: u64,

    /// Times skill failed and had to fall back
    pub total_fallbacks: u64,

    /// Last update timestamp (Unix epoch)
    pub last_updated: u64,
}

impl SkillMetrics {
    /// Applied rate: applied / selections
    pub fn applied_rate(&self) -> f64 {
        if self.total_selections == 0 {
            0.0
        } else {
            self.total_applied as f64 / self.total_selections as f64
        }
    }

    /// Completion rate: completions / applied
    pub fn completion_rate(&self) -> f64 {
        if self.total_applied == 0 {
            0.0
        } else {
            self.total_completions as f64 / self.total_applied as f64
        }
    }

    /// Effective rate: completions / selections (overall effectiveness)
    pub fn effective_rate(&self) -> f64 {
        if self.total_selections == 0 {
            0.0
        } else {
            self.total_completions as f64 / self.total_selections as f64
        }
    }

    /// Fallback rate: fallbacks / applied
    pub fn fallback_rate(&self) -> f64 {
        if self.total_applied == 0 {
            0.0
        } else {
            self.total_fallbacks as f64 / self.total_applied as f64
        }
    }

    /// Check if skill is degraded (completion rate < 50%)
    pub fn is_degraded(&self) -> bool {
        self.completion_rate() < 0.5 && self.total_applied >= 3
    }
}

/// Quality tracking table name
const QUALITY_TABLE: &str = "skill_quality";

/// Threshold for degraded skills
const DEGRADED_THRESHOLD: f64 = 0.5;

/// Minimum applications before degradation check
const MIN_APPLICATIONS_FOR_DEGRADATION: u64 = 3;

/// Initialize quality tracking schema
pub fn init_quality_schema(conn: &Connection) -> WakeyResult<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {} (
            skill_id        TEXT PRIMARY KEY,
            total_selections  INTEGER NOT NULL DEFAULT 0,
            total_applied     INTEGER NOT NULL DEFAULT 0,
            total_completions INTEGER NOT NULL DEFAULT 0,
            total_fallbacks   INTEGER NOT NULL DEFAULT 0,
            last_updated    INTEGER NOT NULL DEFAULT 0
        )",
        QUALITY_TABLE
    ))
    .map_err(|e| WakeyError::Skill {
        skill: "quality".into(),
        message: format!("Failed to create quality table: {}", e),
    })?;

    debug!("Initialized quality tracking schema");
    Ok(())
}

/// Quality tracker for skills
pub struct QualityTracker {
    /// SQLite connection
    conn: Connection,
}

impl QualityTracker {
    /// Create a new quality tracker
    pub fn new(conn: Connection) -> WakeyResult<Self> {
        init_quality_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Record that a skill was selected by LLM
    ///
    /// Increments total_selections counter atomically.
    pub fn record_selection(&self, skill_id: &str) -> WakeyResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before epoch")
            .as_secs() as i64;

        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (skill_id, total_selections, last_updated)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(skill_id) DO UPDATE SET
                     total_selections = total_selections + 1,
                     last_updated = ?2",
                    QUALITY_TABLE
                ),
                params![skill_id, now],
            )
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to record selection: {}", e),
            })?;

        debug!(skill_id = %skill_id, "Recorded selection");
        Ok(())
    }

    /// Record that a skill was actually applied
    ///
    /// Increments total_applied counter atomically.
    pub fn record_applied(&self, skill_id: &str) -> WakeyResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before epoch")
            .as_secs() as i64;

        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (skill_id, total_applied, last_updated)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(skill_id) DO UPDATE SET
                     total_applied = total_applied + 1,
                     last_updated = ?2",
                    QUALITY_TABLE
                ),
                params![skill_id, now],
            )
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to record applied: {}", e),
            })?;

        debug!(skill_id = %skill_id, "Recorded applied");
        Ok(())
    }

    /// Record that a task completed successfully with this skill
    ///
    /// Increments total_completions counter atomically.
    pub fn record_completion(&self, skill_id: &str) -> WakeyResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before epoch")
            .as_secs() as i64;

        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (skill_id, total_completions, last_updated)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(skill_id) DO UPDATE SET
                     total_completions = total_completions + 1,
                     last_updated = ?2",
                    QUALITY_TABLE
                ),
                params![skill_id, now],
            )
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to record completion: {}", e),
            })?;

        debug!(skill_id = %skill_id, "Recorded completion");
        Ok(())
    }

    /// Record that a skill failed and had to fall back
    ///
    /// Increments total_fallbacks counter atomically.
    pub fn record_fallback(&self, skill_id: &str) -> WakeyResult<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before epoch")
            .as_secs() as i64;

        self.conn
            .execute(
                &format!(
                    "INSERT INTO {} (skill_id, total_fallbacks, last_updated)
                 VALUES (?1, 1, ?2)
                 ON CONFLICT(skill_id) DO UPDATE SET
                     total_fallbacks = total_fallbacks + 1,
                     last_updated = ?2",
                    QUALITY_TABLE
                ),
                params![skill_id, now],
            )
            .map_err(|e| WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to record fallback: {}", e),
            })?;

        debug!(skill_id = %skill_id, "Recorded fallback");
        Ok(())
    }

    /// Get quality metrics for a skill
    pub fn get_metrics(&self, skill_id: &str) -> WakeyResult<SkillMetrics> {
        let result = self.conn.query_row(
            &format!(
                "SELECT total_selections, total_applied, total_completions, 
                        total_fallbacks, last_updated
                 FROM {} WHERE skill_id = ?1",
                QUALITY_TABLE
            ),
            params![skill_id],
            |row| {
                Ok(SkillMetrics {
                    total_selections: row.get::<_, i64>(0)? as u64,
                    total_applied: row.get::<_, i64>(1)? as u64,
                    total_completions: row.get::<_, i64>(2)? as u64,
                    total_fallbacks: row.get::<_, i64>(3)? as u64,
                    last_updated: row.get::<_, i64>(4)? as u64,
                })
            },
        );

        match result {
            Ok(metrics) => Ok(metrics),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(SkillMetrics::default()),
            Err(e) => Err(WakeyError::Skill {
                skill: skill_id.into(),
                message: format!("Failed to get metrics: {}", e),
            }),
        }
    }

    /// Get all skills with degraded quality (completion rate < 50%)
    ///
    /// Returns skill IDs that have been applied at least
    /// MIN_APPLICATIONS_FOR_DEGRADATION times and have completion rate < 50%.
    pub fn get_degraded(&self) -> WakeyResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT skill_id FROM {} 
             WHERE total_applied >= ?
             AND CAST(total_completions AS REAL) / total_applied < ?",
                QUALITY_TABLE
            ))
            .map_err(|e| WakeyError::Skill {
                skill: "quality".into(),
                message: format!("Failed to prepare degraded query: {}", e),
            })?;

        let degraded: Vec<String> = stmt
            .query_map(
                params![MIN_APPLICATIONS_FOR_DEGRADATION as i64, DEGRADED_THRESHOLD],
                |row| row.get(0),
            )
            .map_err(|e| WakeyError::Skill {
                skill: "quality".into(),
                message: format!("Failed to query degraded: {}", e),
            })?
            .filter_map(|r| r.ok())
            .collect();

        debug!(count = degraded.len(), "Found degraded skills");
        Ok(degraded)
    }

    /// Get top-performing skills by effective rate
    ///
    /// Returns skills with highest completions/selections ratio.
    pub fn get_top_performers(&self, limit: usize) -> WakeyResult<Vec<(String, f64)>> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT skill_id, 
                    CAST(total_completions AS REAL) / total_selections as effective_rate
             FROM {} 
             WHERE total_selections >= 3
             ORDER BY effective_rate DESC
             LIMIT ?",
                QUALITY_TABLE
            ))
            .map_err(|e| WakeyError::Skill {
                skill: "quality".into(),
                message: format!("Failed to prepare top performers query: {}", e),
            })?;

        let top: Vec<(String, f64)> = stmt
            .query_map(params![limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })
            .map_err(|e| WakeyError::Skill {
                skill: "quality".into(),
                message: format!("Failed to query top performers: {}", e),
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(top)
    }

    /// Record a complete skill usage event
    ///
    /// Convenience method to record selection, applied, completion/fallback in one call.
    pub fn record_usage(
        &self,
        skill_id: &str,
        was_applied: bool,
        was_completed: bool,
        had_fallback: bool,
    ) -> WakeyResult<()> {
        self.record_selection(skill_id)?;

        if was_applied {
            self.record_applied(skill_id)?;

            if was_completed {
                self.record_completion(skill_id)?;
            }

            if had_fallback {
                self.record_fallback(skill_id)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_tracker() -> (QualityTracker, TempDir) {
        let temp = TempDir::new().expect("Temp dir");
        let db_path = temp.path().join("quality.db");
        let conn = Connection::open(&db_path).expect("Open db");
        let tracker = QualityTracker::new(conn).expect("Tracker");
        (tracker, temp)
    }

    #[test]
    fn test_record_selection() {
        let (tracker, _temp) = make_tracker();

        tracker.record_selection("test-skill").expect("Record");

        let metrics = tracker.get_metrics("test-skill").expect("Get metrics");
        assert_eq!(metrics.total_selections, 1);
    }

    #[test]
    fn test_record_applied() {
        let (tracker, _temp) = make_tracker();

        tracker.record_applied("test-skill").expect("Record");

        let metrics = tracker.get_metrics("test-skill").expect("Get metrics");
        assert_eq!(metrics.total_applied, 1);
    }

    #[test]
    fn test_record_completion() {
        let (tracker, _temp) = make_tracker();

        tracker.record_completion("test-skill").expect("Record");

        let metrics = tracker.get_metrics("test-skill").expect("Get metrics");
        assert_eq!(metrics.total_completions, 1);
    }

    #[test]
    fn test_record_fallback() {
        let (tracker, _temp) = make_tracker();

        tracker.record_fallback("test-skill").expect("Record");

        let metrics = tracker.get_metrics("test-skill").expect("Get metrics");
        assert_eq!(metrics.total_fallbacks, 1);
    }

    #[test]
    fn test_metrics_rates() {
        let metrics = SkillMetrics {
            total_selections: 10,
            total_applied: 8,
            total_completions: 6,
            total_fallbacks: 2,
            last_updated: 0,
        };

        assert!((metrics.applied_rate() - 0.8).abs() < 0.001);
        assert!((metrics.completion_rate() - 0.75).abs() < 0.001);
        assert!((metrics.effective_rate() - 0.6).abs() < 0.001);
        assert!((metrics.fallback_rate() - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_is_degraded() {
        // Not degraded: good completion rate
        let good = SkillMetrics {
            total_selections: 10,
            total_applied: 5,
            total_completions: 4,
            total_fallbacks: 1,
            last_updated: 0,
        };
        assert!(!good.is_degraded());

        // Degraded: low completion rate
        let bad = SkillMetrics {
            total_selections: 10,
            total_applied: 5,
            total_completions: 1,
            total_fallbacks: 4,
            last_updated: 0,
        };
        assert!(bad.is_degraded());

        // Not degraded: not enough applications
        let insufficient = SkillMetrics {
            total_selections: 10,
            total_applied: 2,
            total_completions: 0,
            total_fallbacks: 2,
            last_updated: 0,
        };
        assert!(!insufficient.is_degraded());
    }

    #[test]
    fn test_get_degraded() {
        let (tracker, _temp) = make_tracker();

        // Create degraded skill (low completion rate)
        tracker.record_selection("bad-skill").expect("Record");
        tracker.record_applied("bad-skill").expect("Record");
        tracker.record_applied("bad-skill").expect("Record");
        tracker.record_applied("bad-skill").expect("Record");
        tracker.record_fallback("bad-skill").expect("Record");
        tracker.record_fallback("bad-skill").expect("Record");

        // Create good skill
        tracker.record_selection("good-skill").expect("Record");
        tracker.record_applied("good-skill").expect("Record");
        tracker.record_applied("good-skill").expect("Record");
        tracker.record_applied("good-skill").expect("Record");
        tracker.record_completion("good-skill").expect("Record");
        tracker.record_completion("good-skill").expect("Record");
        tracker.record_completion("good-skill").expect("Record");

        let degraded = tracker.get_degraded().expect("Get degraded");
        assert!(degraded.contains(&"bad-skill".into()));
        assert!(!degraded.contains(&"good-skill".into()));
    }

    #[test]
    fn test_record_usage() {
        let (tracker, _temp) = make_tracker();

        tracker
            .record_usage("test-skill", true, true, false)
            .expect("Record");

        let metrics = tracker.get_metrics("test-skill").expect("Get metrics");
        assert_eq!(metrics.total_selections, 1);
        assert_eq!(metrics.total_applied, 1);
        assert_eq!(metrics.total_completions, 1);
        assert_eq!(metrics.total_fallbacks, 0);
    }
}
