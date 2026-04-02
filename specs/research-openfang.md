# Research: OpenFang Implementation Patterns

## Context
We're building Wakey — an open-source AI companion in Rust. OpenFang has the best multi-crate Rust workspace architecture with WASM sandboxed skills. We need to study their patterns.

OpenFang repo is at: /home/dipendra-sharma/projects/openfang/
If that doesn't exist, clone from: https://github.com/RightNow-AI/openfang

## Goal
Understand how OpenFang structures a 14-crate Rust workspace, handles inter-crate communication, and sandboxes WASM skills.

## Focus Areas

### 1. Workspace Structure
- How is Cargo.toml workspace organized?
- How do crates depend on each other?
- How are shared types handled?
- What's the dependency flow?

### 2. WASM Skill Sandbox
- How are skills compiled to WASM?
- How is the WASM runtime configured?
- How is fuel metering implemented?
- How do skills communicate with the host?

### 3. Inter-Crate Communication
- How do crates talk to each other?
- Is there an event bus or direct trait calls?
- How is the kernel/runtime boundary defined?

### 4. Safety/Security Layers
- How many security layers do they have?
- How is taint tracking implemented?
- How are permissions managed?

### 5. Desktop App (Tauri)
- How does openfang-desktop use Tauri?
- How does the UI communicate with the Rust backend?
- What's the overlay/window setup?

## Output
Write a structured research report to: /home/dipendra-sharma/projects/wakey/docs/research/openfang-impl.md

Include actual code snippets. For each pattern, note what we should adopt for Wakey.

## Acceptance Criteria
- [ ] Report contains actual code snippets from OpenFang source
- [ ] All 5 focus areas covered
- [ ] Each pattern has "for Wakey" recommendation
- [ ] Report saved to correct path
