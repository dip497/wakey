use crate::{WakeyError, WakeyResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeyConfig {
    pub general: GeneralConfig,
    pub heartbeat: HeartbeatConfig,
    pub vision: VisionConfig,
    pub memory: MemoryConfig,
    pub action: ActionConfig,
    pub persona: PersonaConfig,
    pub voice: VoiceConfig,
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub data_dir: PathBuf,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub tick_interval_ms: u64,
    pub breath_interval_ms: u64,
    pub reflect_interval_ms: u64,
    pub dream_hour: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    pub a11y_enabled: bool,
    pub ocr_enabled: bool,
    pub vlm_enabled: bool,
    pub vlm_interval_secs: u64,
    pub capture_resolution: (u32, u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub backend: String,
    pub viking_root: PathBuf,
    pub max_working_memory_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    pub enabled: bool,
    pub require_confirmation: bool,
    pub policy_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaConfig {
    pub name: String,
    pub style: String,
    pub proactive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// Enable voice mode
    pub enabled: bool,
    /// Voice provider: "deepgram" (future: "elevenlabs", "openai")
    pub provider: String,
    /// API key environment variable name
    pub api_key_env: String,
    /// STT model (deepgram: "nova-3")
    pub stt_model: String,
    /// TTS model (deepgram: "aura-2-theia-en")
    pub tts_model: String,
    /// Audio sample rate for ASR (16000 Hz)
    pub asr_sample_rate: u32,
    /// Audio sample rate for TTS (24000 Hz)
    pub tts_sample_rate: u32,
    /// Language for ASR (e.g., "en", "zh")
    pub language: String,
    /// Push-to-talk key (e.g., "space", "ctrl+space")
    pub push_to_talk_key: String,
    /// Endpointing timeout in ms (speech end detection)
    pub endpointing_ms: u32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: "deepgram".to_string(),
            api_key_env: "DEEPGRAM_API_KEY".to_string(),
            stt_model: "nova-3".to_string(),
            tts_model: "aura-2-theia-en".to_string(),
            asr_sample_rate: 16000,
            tts_sample_rate: 24000,
            language: "en".to_string(),
            push_to_talk_key: "space".to_string(),
            endpointing_ms: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub default_provider: String,
    pub providers: Vec<LlmProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub name: String,
    pub api_base: String,
    pub model: String,
    pub api_key_env: String,
}

impl Default for WakeyConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                data_dir: PathBuf::from("~/.wakey"),
                log_level: "info".to_string(),
            },
            heartbeat: HeartbeatConfig {
                tick_interval_ms: 2000,
                breath_interval_ms: 30_000,
                reflect_interval_ms: 900_000,
                dream_hour: 4,
            },
            vision: VisionConfig {
                a11y_enabled: true,
                ocr_enabled: true,
                vlm_enabled: true,
                vlm_interval_secs: 60,
                capture_resolution: (1024, 768),
            },
            memory: MemoryConfig {
                backend: "viking".to_string(),
                viking_root: PathBuf::from("~/.wakey/viking"),
                max_working_memory_tokens: 4096,
            },
            action: ActionConfig {
                enabled: true,
                require_confirmation: true,
                policy_dir: PathBuf::from("~/.wakey/policies"),
            },
            persona: PersonaConfig {
                name: "Buddy".to_string(),
                style: "casual".to_string(),
                proactive: true,
            },
            voice: VoiceConfig::default(),
            llm: LlmConfig {
                default_provider: "anthropic".to_string(),
                providers: vec![],
            },
        }
    }
}

impl WakeyConfig {
    /// Load configuration from a TOML file.
    ///
    /// Falls back to defaults if the file doesn't exist (logs a warning).
    /// Expands `~` in all path fields to the user's home directory.
    pub fn load(path: &Path) -> WakeyResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let config: Self = toml::from_str(&contents).map_err(|e| {
                    WakeyError::Config(format!("Failed to parse {}: {}", path.display(), e))
                })?;
                Ok(config.expand_paths())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(
                    "Config file not found at {}, using defaults",
                    path.display()
                );
                Ok(Self::default())
            }
            Err(e) => Err(WakeyError::Config(format!(
                "Failed to read {}: {}",
                path.display(),
                e
            ))),
        }
    }

    /// Expand `~` in all PathBuf fields to the user's home directory.
    fn expand_paths(self) -> Self {
        Self {
            general: GeneralConfig {
                data_dir: expand_tilde(&self.general.data_dir),
                log_level: self.general.log_level,
            },
            heartbeat: self.heartbeat,
            vision: self.vision,
            memory: MemoryConfig {
                backend: self.memory.backend,
                viking_root: expand_tilde(&self.memory.viking_root),
                max_working_memory_tokens: self.memory.max_working_memory_tokens,
            },
            action: ActionConfig {
                enabled: self.action.enabled,
                require_confirmation: self.action.require_confirmation,
                policy_dir: expand_tilde(&self.action.policy_dir),
            },
            persona: self.persona,
            voice: self.voice,
            llm: self.llm,
        }
    }
}

/// Expand `~` at the start of a path to the user's home directory.
///
/// Returns the path unchanged if:
/// - It doesn't start with `~`
/// - The home directory cannot be determined
fn expand_tilde(path: &Path) -> PathBuf {
    if !path.starts_with("~") {
        return path.to_path_buf();
    }

    // Get home directory from $HOME env var (Linux)
    // Fall back to /home if not set
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home"));

    // Strip the `~` prefix and append to home
    let stripped = path.strip_prefix("~").unwrap_or(path);
    home.join(stripped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_expand_tilde_with_tilde() {
        // Use actual HOME from environment
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());

        let path = PathBuf::from("~/.wakey");
        let expanded = expand_tilde(&path);

        // Should expand to HOME/.wakey
        assert!(expanded.starts_with(&home));
        assert!(expanded.ends_with(".wakey"));
        assert!(!expanded.starts_with("~"));
    }

    #[test]
    fn test_expand_tilde_without_tilde() {
        let path = PathBuf::from("/absolute/path");
        let expanded = expand_tilde(&path);

        assert_eq!(expanded, path);
    }

    #[test]
    fn test_expand_tilde_nested_path() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());

        let path = PathBuf::from("~/.wakey/viking/memory");
        let expanded = expand_tilde(&path);

        assert!(expanded.starts_with(&home));
        assert!(expanded.ends_with(".wakey/viking/memory"));
        assert!(!expanded.starts_with("~"));
    }

    #[test]
    fn test_load_existing_config() {
        // Create a temp file with valid config
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let config_content = r#"
[general]
data_dir = "~/.wakey"
log_level = "debug"

[heartbeat]
tick_interval_ms = 1000
breath_interval_ms = 5000
reflect_interval_ms = 60000
dream_hour = 3

[vision]
a11y_enabled = false
ocr_enabled = true
vlm_enabled = false
vlm_interval_secs = 30
capture_resolution = [800, 600]

[memory]
backend = "sqlite"
viking_root = "~/.wakey/data"
max_working_memory_tokens = 8192

[action]
enabled = false
require_confirmation = false
policy_dir = "~/.wakey/rules"

[persona]
name = "TestBot"
style = "formal"
proactive = false

[voice]
enabled = true
provider = "deepgram"
api_key_env = "DEEPGRAM_API_KEY"
stt_model = "nova-3"
tts_model = "aura-2-theia-en"
asr_sample_rate = 16000
tts_sample_rate = 24000
language = "en"
push_to_talk_key = "space"
endpointing_ms = 300

[llm]
default_provider = "test"
providers = []
"#;
        temp_file
            .write_all(config_content.as_bytes())
            .expect("Failed to write config");
        temp_file.flush().expect("Failed to flush");

        let config = WakeyConfig::load(temp_file.path()).expect("Failed to load config");

        // Verify loaded values
        assert_eq!(config.general.log_level, "debug");
        assert_eq!(config.heartbeat.tick_interval_ms, 1000);
        assert_eq!(config.vision.a11y_enabled, false);
        assert_eq!(config.memory.backend, "sqlite");
        assert_eq!(config.memory.max_working_memory_tokens, 8192);
        assert_eq!(config.action.enabled, false);
        assert_eq!(config.persona.name, "TestBot");
        assert_eq!(config.voice.enabled, true);
        assert_eq!(config.voice.provider, "deepgram");
        assert_eq!(config.llm.default_provider, "test");

        // Verify path expansion - paths should not contain ~
        assert!(!config.general.data_dir.starts_with("~"));
        assert!(!config.memory.viking_root.starts_with("~"));
        assert!(!config.action.policy_dir.starts_with("~"));
    }

    #[test]
    fn test_load_missing_config_returns_defaults() {
        let missing_path = PathBuf::from("/nonexistent/path/config.toml");

        let config =
            WakeyConfig::load(&missing_path).expect("Should return defaults for missing file");

        // Verify defaults are used
        assert_eq!(config.general.log_level, "info");
        assert_eq!(config.heartbeat.tick_interval_ms, 2000);
        assert_eq!(config.vision.a11y_enabled, true);
        assert_eq!(config.persona.name, "Buddy");
    }

    #[test]
    fn test_load_invalid_toml_returns_error() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"invalid [[[toml")
            .expect("Failed to write invalid config");
        temp_file.flush().expect("Failed to flush");

        let result = WakeyConfig::load(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_actual_default_config() {
        // Load the actual project default config
        let config_path = PathBuf::from("config/default.toml");

        if config_path.exists() {
            let config = WakeyConfig::load(&config_path).expect("Failed to load default config");

            // Verify expected values from default.toml
            assert_eq!(config.general.log_level, "info");
            assert_eq!(config.heartbeat.tick_interval_ms, 2000);
            assert_eq!(config.persona.name, "Buddy");
            assert_eq!(config.llm.default_provider, "qwen");

            // Verify voice config
            assert_eq!(config.voice.enabled, true);
            assert_eq!(config.voice.provider, "deepgram");
            assert_eq!(config.voice.language, "en");

            // Verify paths are expanded (no ~ prefix)
            assert!(!config.general.data_dir.starts_with("~"));
        }
    }
}
