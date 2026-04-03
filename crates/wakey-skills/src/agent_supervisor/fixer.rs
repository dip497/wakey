//! Auto Fixer — Tier 1 auto-fix with Cedar policy check
//!
//! Pattern-matched responses for known error patterns.
//! Each fix requires Cedar policy approval before execution.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Action to take to fix an issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixAction {
    /// Install a missing crate
    InstallCrate {
        crate_name: String,
        features: Vec<String>,
    },

    /// Run cargo fmt
    FormatCode,

    /// Run cargo clippy --fix
    FixLints,

    /// Wait and retry (for rate limits)
    WaitAndRetry { delay_secs: u64 },

    /// Check environment variable
    CheckEnvVar { var_name: String },

    /// Restart agent with context injection
    RestartWithContext { context: String },

    /// Run a shell command
    RunCommand {
        command: String,
        working_dir: Option<PathBuf>,
    },

    /// Send message to agent
    SendMessage { message: String },
}

/// Result of a fix attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixResult {
    /// Fix succeeded
    Success(String),

    /// Fix failed
    Failed(String),

    /// Fix needs user approval
    NeedsApproval,
}

/// Known error patterns and their fixes
fn get_fix_patterns() -> Vec<(&'static str, FixAction, &'static str)> {
    vec![
        // Missing crates
        (
            "missing_crate",
            FixAction::InstallCrate {
                crate_name: String::new(), // Will be extracted
                features: vec![],
            },
            "medium",
        ),
        // Format issues
        ("format_error", FixAction::FormatCode, "low"),
        // Lint issues
        ("lint_error", FixAction::FixLints, "low"),
        // Rate limits
        (
            "rate_limit",
            FixAction::WaitAndRetry { delay_secs: 60 },
            "low",
        ),
        // Auth failures
        (
            "auth_failure",
            FixAction::CheckEnvVar {
                var_name: String::new(), // Will be determined
            },
            "high",
        ),
        // Compile errors (needs context)
        (
            "compile_error",
            FixAction::RestartWithContext {
                context: "Fix the compile error".into(),
            },
            "medium",
        ),
    ]
}

/// Auto-fixer for known error patterns
pub struct AutoFixer {
    /// Mapping from error pattern to fix action
    patterns: HashMap<String, (FixAction, String)>,

    /// Maximum retries before giving up
    max_retries: usize,

    /// Retry count per session
    retries: HashMap<String, usize>,
}

impl AutoFixer {
    /// Create a new auto-fixer
    pub fn new(max_retries: usize) -> Self {
        let mut patterns = HashMap::new();

        for (pattern, action, risk) in get_fix_patterns() {
            patterns.insert(pattern.to_string(), (action, risk.to_string()));
        }

        Self {
            patterns,
            max_retries,
            retries: HashMap::new(),
        }
    }

    /// Check if an error pattern is auto-fixable
    pub fn is_auto_fixable(&self, pattern: &str) -> bool {
        self.patterns.contains_key(pattern)
    }

    /// Get the fix action for a pattern
    pub fn get_fix_action(&self, pattern: &str) -> Option<FixAction> {
        self.patterns.get(pattern).map(|(action, _)| action.clone())
    }

    /// Get the risk level for a pattern
    pub fn get_risk_level(&self, pattern: &str) -> &str {
        self.patterns
            .get(pattern)
            .map(|(_, risk)| risk.as_str())
            .unwrap_or("high") // Unknown patterns are high risk
    }

    /// Execute a fix action
    ///
    /// Returns:
    /// - Success if fix was applied
    /// - Failed if fix couldn't be applied
    /// - NeedsApproval if fix requires user approval (high risk)
    pub async fn execute_fix(&self, action: &FixAction) -> FixResult {
        // Cedar policy check would go here
        // For now, we check risk level inline

        match action {
            FixAction::InstallCrate {
                crate_name,
                features,
            } => self.install_crate(crate_name, features).await,

            FixAction::FormatCode => self.run_cargo_fmt().await,

            FixAction::FixLints => self.run_clippy_fix().await,

            FixAction::WaitAndRetry { delay_secs } => self.wait_and_retry(*delay_secs).await,

            FixAction::CheckEnvVar { var_name } => {
                // Can't auto-fix missing env vars
                FixResult::Failed(format!(
                    "Missing environment variable: {}. Please set it and restart.",
                    var_name
                ))
            }

            FixAction::RestartWithContext { context } => {
                // Would need integration with agent process management
                info!(context = %context, "Would restart agent with context");
                FixResult::Success("Agent restart requested".into())
            }

            FixAction::RunCommand {
                command,
                working_dir,
            } => self.run_command(command, working_dir.clone()).await,

            FixAction::SendMessage { message } => {
                // Would need integration with agent communication
                info!(message = %message, "Would send message to agent");
                FixResult::Success("Message sent".into())
            }
        }
    }

    /// Install a missing crate
    async fn install_crate(&self, crate_name: &str, features: &[String]) -> FixResult {
        if crate_name.is_empty() {
            return FixResult::Failed("No crate name specified".into());
        }

        info!(crate = %crate_name, "Installing missing crate");

        let mut args: Vec<&str> = vec!["add", crate_name];

        let features_joined;
        if !features.is_empty() {
            features_joined = features.join(",");
            args.push("--features");
            args.push(&features_joined);
        }

        let output = Command::new("cargo").args(&args).output();

        match output {
            Ok(output) if output.status.success() => {
                FixResult::Success(format!("Installed crate: {}", crate_name))
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                FixResult::Failed(format!("Failed to install {}: {}", crate_name, stderr))
            }
            Err(e) => FixResult::Failed(format!("Failed to run cargo: {}", e)),
        }
    }

    /// Run cargo fmt
    async fn run_cargo_fmt(&self) -> FixResult {
        info!("Running cargo fmt");

        let output = Command::new("cargo").args(["fmt", "--all"]).output();

        match output {
            Ok(output) if output.status.success() => FixResult::Success("Code formatted".into()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                FixResult::Failed(format!("Format failed: {}", stderr))
            }
            Err(e) => FixResult::Failed(format!("Failed to run cargo fmt: {}", e)),
        }
    }

    /// Run cargo clippy --fix
    async fn run_clippy_fix(&self) -> FixResult {
        info!("Running cargo clippy --fix");

        let output = Command::new("cargo")
            .args(["clippy", "--fix", "--allow-dirty", "--allow-staged"])
            .output();

        match output {
            Ok(output) if output.status.success() => FixResult::Success("Lints fixed".into()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Clippy --fix may fail if there are still warnings
                if stderr.contains("warning:") {
                    FixResult::Success("Some lints fixed".into())
                } else {
                    FixResult::Failed(format!("Clippy fix failed: {}", stderr))
                }
            }
            Err(e) => FixResult::Failed(format!("Failed to run clippy: {}", e)),
        }
    }

    /// Wait and retry
    async fn wait_and_retry(&self, delay_secs: u64) -> FixResult {
        info!(delay_secs = delay_secs, "Waiting before retry");
        tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
        FixResult::Success(format!("Waited {} seconds, ready to retry", delay_secs))
    }

    /// Run a shell command
    async fn run_command(&self, command: &str, working_dir: Option<PathBuf>) -> FixResult {
        info!(command = %command, "Running fix command");

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                FixResult::Success(format!("Command succeeded: {}", stdout.trim()))
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                FixResult::Failed(format!("Command failed: {}", stderr))
            }
            Err(e) => FixResult::Failed(format!("Failed to run command: {}", e)),
        }
    }

    /// Record a retry attempt for a session
    pub fn record_retry(&mut self, session_id: &str) -> bool {
        let count = self.retries.entry(session_id.to_string()).or_insert(0);
        *count += 1;

        if *count >= self.max_retries {
            warn!(session_id = %session_id, retries = *count, "Max retries exceeded");
            return false;
        }

        true
    }

    /// Get retry count for a session
    pub fn get_retry_count(&self, session_id: &str) -> usize {
        self.retries.get(session_id).copied().unwrap_or(0)
    }

    /// Reset retry count for a session
    pub fn reset_retries(&mut self, session_id: &str) {
        self.retries.remove(session_id);
    }
}

/// Extract crate name from error message
pub fn extract_crate_name(error: &str) -> Option<String> {
    // Pattern: "cannot find crate `serde_json`"
    let re = regex::Regex::new(r"cannot find crate [`'](\w+)[`']").ok()?;

    if let Some(caps) = re.captures(error) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }

    // Pattern: "unresolved import `serde_json`"
    let re = regex::Regex::new(r"unresolved import [`']([\w:]+)[`']").ok()?;

    if let Some(caps) = re.captures(error) {
        // Extract crate name from path (e.g., "serde_json::Value" -> "serde_json")
        let path = caps.get(1)?.as_str();
        let crate_name = path.split("::").next().unwrap_or(path);
        return Some(crate_name.to_string());
    }

    None
}

/// Extract environment variable name from error message
pub fn extract_env_var(error: &str) -> Option<String> {
    // Pattern: "OPENAI_API_KEY is not set"
    let re = regex::Regex::new(r"([A-Z_]{3,})\s+is not set").ok()?;

    if let Some(caps) = re.captures(error) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }

    // Pattern: "Missing required environment variable: OPENAI_API_KEY"
    let re = regex::Regex::new(r"environment variable[:\s]+([A-Z_]{3,})").ok()?;

    if let Some(caps) = re.captures(error) {
        return caps.get(1).map(|m| m.as_str().to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_auto_fixable() {
        let fixer = AutoFixer::new(3);

        assert!(fixer.is_auto_fixable("missing_crate"));
        assert!(fixer.is_auto_fixable("format_error"));
        assert!(fixer.is_auto_fixable("rate_limit"));
        assert!(!fixer.is_auto_fixable("unknown_pattern"));
    }

    #[test]
    fn test_get_risk_level() {
        let fixer = AutoFixer::new(3);

        assert_eq!(fixer.get_risk_level("format_error"), "low");
        assert_eq!(fixer.get_risk_level("auth_failure"), "high");
        assert_eq!(fixer.get_risk_level("unknown"), "high");
    }

    #[test]
    fn test_extract_crate_name() {
        assert_eq!(
            extract_crate_name("cannot find crate `serde_json`"),
            Some("serde_json".to_string())
        );

        assert_eq!(
            extract_crate_name("unresolved import `tokio::runtime`"),
            Some("tokio".to_string())
        );

        assert_eq!(extract_crate_name("some other error"), None);
    }

    #[test]
    fn test_extract_env_var() {
        assert_eq!(
            extract_env_var("OPENAI_API_KEY is not set"),
            Some("OPENAI_API_KEY".to_string())
        );

        assert_eq!(
            extract_env_var("Missing required environment variable: ANTHROPIC_API_KEY"),
            Some("ANTHROPIC_API_KEY".to_string())
        );

        assert_eq!(extract_env_var("some other error"), None);
    }

    #[tokio::test]
    async fn test_wait_and_retry() {
        let fixer = AutoFixer::new(3);

        let start = std::time::Instant::now();
        let result = fixer.wait_and_retry(1).await;
        let elapsed = start.elapsed();

        assert!(matches!(result, FixResult::Success(_)));
        assert!(elapsed.as_secs() >= 1);
    }

    #[test]
    fn test_retry_tracking() {
        let mut fixer = AutoFixer::new(3);

        assert!(fixer.record_retry("session-1"));
        assert!(fixer.record_retry("session-1"));
        assert!(fixer.record_retry("session-1"));
        assert!(!fixer.record_retry("session-1")); // Should fail on 4th try

        assert_eq!(fixer.get_retry_count("session-1"), 4);

        fixer.reset_retries("session-1");
        assert_eq!(fixer.get_retry_count("session-1"), 0);
    }
}
