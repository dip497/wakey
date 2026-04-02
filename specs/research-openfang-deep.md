# Deep Research: OpenFang Internals

## Goal
Read the ACTUAL OpenFang Rust codebase and extract implementation details for multi-crate workspace, WASM sandbox, inter-crate communication, security layers, skill system, and desktop app.

## Repo
Located at /home/dipendra-sharma/projects/openfang/
If not exists, clone https://github.com/RightNow-AI/openfang

## Focus Areas (read actual .rs files)

### 1. Multi-Crate Workspace
- How is Cargo.toml workspace organized?
- How do 14 crates depend on each other (exact dependency graph)?
- How are shared types handled across crates?
- Find the actual workspace Cargo.toml and key crate boundaries

### 2. WASM Skill Sandbox
- Which WASM runtime? wasmtime? wasmer?
- How are skills compiled to WASM?
- How does fuel metering work (prevent infinite loops)?
- How do WASM skills communicate with the host (imports/exports)?
- Find the actual sandbox implementation code

### 3. Inter-Crate Communication
- Do crates use an event bus, trait objects, or direct calls?
- How does the kernel assemble subsystems?
- How is the runtime ↔ kernel boundary defined?
- Find the actual wiring code

### 4. Security Layers
- How many security layers and what are they?
- How is taint tracking implemented?
- How are permissions managed per-skill?
- How does RBAC work?
- Find the actual security policy code

### 5. Desktop App (Tauri)
- How does openfang-desktop use Tauri 2.0?
- How does frontend communicate with Rust backend?
- How is the window configured (overlay, always-on-top)?
- Find the actual Tauri config and IPC code

### 6. Skill System + FangHub
- How are skills structured on disk?
- How does the marketplace (FangHub) work?
- How are skills discovered, loaded, and executed?
- How does skill versioning work?

## Output
Save to: /home/dipendra-sharma/projects/wakey/docs/research/openfang-deep.md
Include ACTUAL Rust code snippets with file paths.
