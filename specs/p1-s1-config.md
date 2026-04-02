# P1-S1: Config Loader

## Goal
Load config/default.toml into WakeyConfig at startup. Expand ~ in paths. Fall back to defaults if file not found.

## Crate
wakey-types (extend src/config.rs)

## What to implement
1. Add `impl WakeyConfig { pub fn load(path: &Path) -> WakeyResult<Self> }` to config.rs
2. Add `fn expand_tilde(path: &Path) -> PathBuf` helper
3. Expand ~ in all PathBuf fields after loading
4. Fall back to Default::default() if file not found (log warning)
5. Add `tracing` to wakey-types dependencies if not present

## Read first
- crates/wakey-types/src/config.rs (existing config structs)
- crates/wakey-types/AGENTS.md
- config/default.toml (the actual config file)

## Verify
```bash
cargo check --workspace
cargo test --package wakey-types
```

## Acceptance criteria
- WakeyConfig::load("config/default.toml") returns Ok with correct values
- WakeyConfig::load("nonexistent.toml") returns Ok with defaults
- All ~ paths expanded to $HOME
- cargo check passes
