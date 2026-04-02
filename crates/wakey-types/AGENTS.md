# wakey-types — Agent Instructions

This is the **foundation crate**. It depends on NOTHING internal. Every other crate depends on it.

## What belongs here
- `WakeyEvent` enum (ALL event variants)
- `WakeyError` and `WakeyResult<T>`
- `WakeyConfig` and all config sub-structs
- Shared types: `Mood`, `Emotion`, `Urgency`, `Importance`, `ActionPlan`, etc.

## What does NOT belong here
- Business logic
- Async code
- Network calls
- Any dependency on other wakey-* crates

## Rules
- Every new event variant goes in `src/event.rs`
- Every new config section goes in `src/config.rs`
- All types must derive `Debug, Clone, Serialize, Deserialize`
- Error types use `thiserror`
