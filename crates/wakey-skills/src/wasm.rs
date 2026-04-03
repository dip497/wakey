//! WASM sandbox — skill execution sandboxing (placeholder)
//!
//! Future: wasmtime-based skill execution with:
//! - Fuel metering for resource limits
//! - Capability-based security
//! - Isolated execution environment
//!
//! Currently a placeholder as WASM sandboxing is not yet implemented.

use wakey_types::{WakeyError, WakeyResult};

/// WASM sandbox configuration (placeholder)
#[derive(Debug, Clone)]
pub struct WasmConfig {
    /// Maximum fuel (instructions)
    pub max_fuel: u64,

    /// Maximum memory (bytes)
    pub max_memory: u64,

    /// Timeout for execution (ms)
    pub timeout_ms: u64,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            max_fuel: 1_000_000_000,      // 1B instructions
            max_memory: 16 * 1024 * 1024, // 16MB
            timeout_ms: 30_000,           // 30s
        }
    }
}

/// WASM sandbox executor (placeholder)
pub struct WasmSandbox {
    _config: WasmConfig,
}

impl WasmSandbox {
    /// Create a new WASM sandbox
    pub fn new(config: WasmConfig) -> WakeyResult<Self> {
        Ok(Self { _config: config })
    }

    /// Execute a skill in the sandbox
    ///
    /// Currently unimplemented — returns error
    pub fn execute(&self, _skill_wasm: &[u8], _input: &[u8]) -> WakeyResult<Vec<u8>> {
        Err(WakeyError::Skill {
            skill: "wasm".into(),
            message: "WASM sandbox not yet implemented".into(),
        })
    }

    /// Check if WASM execution is available
    pub fn is_available(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_config_default() {
        let config = WasmConfig::default();
        assert_eq!(config.max_fuel, 1_000_000_000);
        assert_eq!(config.max_memory, 16 * 1024 * 1024);
    }

    #[test]
    fn test_wasm_sandbox_unavailable() {
        let sandbox = WasmSandbox::new(WasmConfig::default()).expect("Create");
        assert!(!sandbox.is_available());
    }
}
