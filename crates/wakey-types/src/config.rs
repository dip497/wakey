use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeyConfig {
    pub general: GeneralConfig,
    pub heartbeat: HeartbeatConfig,
    pub vision: VisionConfig,
    pub memory: MemoryConfig,
    pub action: ActionConfig,
    pub persona: PersonaConfig,
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
            llm: LlmConfig {
                default_provider: "anthropic".to_string(),
                providers: vec![],
            },
        }
    }
}
