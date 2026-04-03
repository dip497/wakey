//! Skill format parser — Hermes SKILL.md format with YAML frontmatter
//!
//! Parses skill files following the Hermes Agent format:
//! ```yaml
//! ---
//! name: skill-name
//! description: Brief description
//! version: 1.0.0
//! dependencies: [other-skill]
//! tags: [category, domain]
//! platforms: [linux]
//! ---
//!
//! # Skill content in markdown
//! ```

use std::fs;
use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::debug;

use wakey_types::{WakeyError, WakeyResult};

/// Skill manifest extracted from YAML frontmatter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Unique skill name (required, max 64 chars)
    pub name: String,

    /// Brief description for skill selection (required, max 1024 chars)
    pub description: String,

    /// Semantic version
    #[serde(default)]
    pub version: String,

    /// Other skills this skill depends on
    #[serde(default)]
    pub dependencies: Vec<String>,

    /// Tags for categorization and search
    #[serde(default)]
    pub tags: Vec<String>,

    /// Platform restrictions (empty = all platforms)
    #[serde(default)]
    pub platforms: Vec<String>,
}

/// Full parsed skill content
#[derive(Debug, Clone)]
pub struct SkillContent {
    /// Manifest from frontmatter
    pub manifest: SkillManifest,

    /// Markdown content (body after frontmatter)
    pub body: String,

    /// Source file path
    pub source_path: String,

    /// File modification time for cache invalidation
    pub mtime: u64,
}

/// Default skill version
const DEFAULT_VERSION: &str = "1.0.0";

/// Maximum allowed name length (Hermes spec)
const MAX_NAME_LEN: usize = 64;

/// Maximum allowed description length (Hermes spec)
const MAX_DESC_LEN: usize = 1024;

/// Parse a SKILL.md file at the given path
///
/// # Errors
/// Returns error if:
/// - File doesn't exist or can't be read
/// - Frontmatter is missing or invalid YAML
/// - Required fields (name, description) are missing
/// - Name or description exceeds length limits
pub fn parse_skill(path: &Path) -> WakeyResult<SkillContent> {
    let content = fs::read_to_string(path).map_err(|e| WakeyError::Skill {
        skill: path.display().to_string(),
        message: format!("Failed to read file: {}", e),
    })?;

    let (frontmatter, body) = split_frontmatter(&content)?;
    let manifest = parse_frontmatter(&frontmatter, path)?;

    // Get file mtime for cache invalidation
    let metadata = fs::metadata(path).map_err(|e| WakeyError::Skill {
        skill: path.display().to_string(),
        message: format!("Failed to get metadata: {}", e),
    })?;

    let mtime = metadata
        .modified()
        .map_err(|e| WakeyError::Skill {
            skill: path.display().to_string(),
            message: format!("Failed to get mtime: {}", e),
        })?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| WakeyError::Skill {
            skill: path.display().to_string(),
            message: format!("Invalid mtime: {}", e),
        })?
        .as_secs();

    Ok(SkillContent {
        manifest,
        body: body.trim().to_string(),
        source_path: path.display().to_string(),
        mtime,
    })
}

/// Split content into frontmatter and body
fn split_frontmatter(content: &str) -> WakeyResult<(String, String)> {
    // Pattern: ---\n<yaml>\n---\n<body>
    // Use (?s) for DOTALL mode so . matches newlines
    let pattern =
        Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)$").expect("Invalid frontmatter regex");

    let captures = pattern.captures(content).ok_or_else(|| WakeyError::Skill {
        skill: "unknown".into(),
        message: "Missing or malformed YAML frontmatter".into(),
    })?;

    let frontmatter = captures
        .get(1)
        .expect("Frontmatter capture group")
        .as_str()
        .to_string();

    let body = captures
        .get(2)
        .expect("Body capture group")
        .as_str()
        .to_string();

    Ok((frontmatter, body))
}

/// Parse YAML frontmatter into SkillManifest with validation
fn parse_frontmatter(yaml: &str, path: &Path) -> WakeyResult<SkillManifest> {
    // Parse raw YAML into a flexible structure
    let raw: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| WakeyError::Skill {
        skill: path.display().to_string(),
        message: format!("Invalid YAML frontmatter: {}", e),
    })?;

    // Extract required fields
    let name = raw
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| WakeyError::Skill {
            skill: path.display().to_string(),
            message: "Missing required field: name".into(),
        })?;

    let description = raw
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| WakeyError::Skill {
            skill: path.display().to_string(),
            message: "Missing required field: description".into(),
        })?;

    // Validate lengths
    if name.len() > MAX_NAME_LEN {
        return Err(WakeyError::Skill {
            skill: path.display().to_string(),
            message: format!("Name exceeds {} characters", MAX_NAME_LEN),
        });
    }

    if description.len() > MAX_DESC_LEN {
        return Err(WakeyError::Skill {
            skill: path.display().to_string(),
            message: format!("Description exceeds {} characters", MAX_DESC_LEN),
        });
    }

    // Extract optional fields with defaults
    let version = raw
        .get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_VERSION.to_string());

    let dependencies = raw
        .get("dependencies")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let tags = raw
        .get("tags")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let platforms = raw
        .get("platforms")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    debug!(
        name = %name,
        deps = ?dependencies,
        tags = ?tags,
        "Parsed skill manifest"
    );

    Ok(SkillManifest {
        name,
        description,
        version,
        dependencies,
        tags,
        platforms,
    })
}

/// Generate L0 abstract (short summary) from skill content
///
/// L0 abstract is used for tiered loading to save tokens.
/// Extracts first paragraph or first 200 chars.
pub fn generate_abstract(content: &SkillContent) -> String {
    // Try to extract first paragraph after any "## When to Use" or "## Overview" heading
    let body = &content.body;

    // Look for overview/when-to-use sections
    let abstract_pattern =
        Regex::new(r"(?:##\s*(?:Overview|When to Use|Description)\s*\n+)([^\n#]+)")
            .expect("Invalid abstract regex");

    if let Some(caps) = abstract_pattern.captures(body) {
        let extracted = caps.get(1).expect("Abstract capture").as_str().trim();

        if extracted.len() > 200 {
            return format!("{}...", extracted.chars().take(197).collect::<String>());
        }
        return extracted.to_string();
    }

    // Fallback: first 200 chars of body
    if body.len() > 200 {
        format!("{}...", body.chars().take(197).collect::<String>())
    } else {
        body.clone()
    }
}

/// Generate L1 overview (medium-length summary) from skill content
///
/// L1 overview is used for skill selection context.
/// Extracts key sections: When to Use, Procedure, Pitfalls.
pub fn generate_overview(content: &SkillContent) -> String {
    let body = &content.body;

    let mut sections = Vec::new();

    // Extract key sections
    let section_pattern =
        Regex::new(r"(##\s*[^\n]+\n+[^\n#]+(?:\n+[^\n#]+)*)").expect("Invalid section regex");

    for cap in section_pattern.captures_iter(body) {
        let section = cap.get(0).expect("Section capture").as_str();

        // Only include certain key sections
        if section.starts_with("## When to Use")
            || section.starts_with("## Procedure")
            || section.starts_with("## Pitfalls")
            || section.starts_with("## Verification")
        {
            sections.push(section.trim());
        }
    }

    if sections.is_empty() {
        // Fallback: first 500 chars
        if body.len() > 500 {
            format!("{}...", body.chars().take(497).collect::<String>())
        } else {
            body.clone()
        }
    } else {
        sections.join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_skill_content(frontmatter: &str, body: &str) -> String {
        format!("---\n{}\n---\n{}", frontmatter, body)
    }

    #[test]
    fn test_split_frontmatter() {
        let content = make_skill_content(
            "name: test-skill\ndescription: A test",
            "# Test Skill\n\nSome content",
        );

        let (fm, b) = split_frontmatter(&content).unwrap();
        assert_eq!(fm, "name: test-skill\ndescription: A test");
        assert_eq!(b, "# Test Skill\n\nSome content");
    }

    #[test]
    fn test_split_frontmatter_missing() {
        let content = "No frontmatter here";
        let result = split_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_frontmatter_valid() {
        let yaml = "name: deploy-app\ndescription: Deploy to production\nversion: 1.0.0\ndependencies:\n  - fix-lint\ntags:\n  - devops";
        let path = PathBuf::from("test/SKILL.md");

        let manifest = parse_frontmatter(yaml, &path).unwrap();
        assert_eq!(manifest.name, "deploy-app");
        assert_eq!(manifest.description, "Deploy to production");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.dependencies, vec!["fix-lint"]);
        assert_eq!(manifest.tags, vec!["devops"]);
    }

    #[test]
    fn test_parse_frontmatter_missing_name() {
        let yaml = "description: Missing name";
        let path = PathBuf::from("test/SKILL.md");

        let result = parse_frontmatter(yaml, &path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Missing required field: name")
        );
    }

    #[test]
    fn test_parse_frontmatter_name_too_long() {
        let yaml = format!("name: {}\ndescription: Test", "x".repeat(MAX_NAME_LEN + 1));
        let path = PathBuf::from("test/SKILL.md");

        let result = parse_frontmatter(&yaml, &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds"));
    }

    #[test]
    fn test_generate_abstract() {
        let content = SkillContent {
            manifest: SkillManifest {
                name: "test".into(),
                description: "test".into(),
                version: "1.0.0".into(),
                dependencies: vec![],
                tags: vec![],
                platforms: vec![],
            },
            body: "## Overview\n\nThis skill does things.\n\n## Procedure\n\nStep 1.".into(),
            source_path: "test".into(),
            mtime: 0,
        };

        let skill_abstract = generate_abstract(&content);
        assert_eq!(skill_abstract, "This skill does things.");
    }

    #[test]
    fn test_generate_overview() {
        let content = SkillContent {
            manifest: SkillManifest {
                name: "test".into(),
                description: "test".into(),
                version: "1.0.0".into(),
                dependencies: vec![],
                tags: vec![],
                platforms: vec![],
            },
            body: "## When to Use\nWhen asked to test.\n\n## Procedure\n1. Do it.\n\n## Pitfalls\nDon't fail.".into(),
            source_path: "test".into(),
            mtime: 0,
        };

        let overview = generate_overview(&content);
        assert!(overview.contains("## When to Use"));
        assert!(overview.contains("## Procedure"));
        assert!(overview.contains("## Pitfalls"));
    }
}
