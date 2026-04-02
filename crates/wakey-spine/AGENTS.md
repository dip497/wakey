# wakey-spine — Agent Instructions

This is the **central nervous system**. All inter-crate communication flows through here.

## What belongs here
- The `Spine` struct (tokio broadcast sender/receiver)
- Event routing utilities
- Subscriber helpers

## Architecture
- Uses `tokio::sync::broadcast` with capacity 1024
- `emit()` sends events (fire-and-forget, never blocks)
- `subscribe()` returns a receiver for any subsystem to listen
- Events that can't be delivered are dropped (acceptable for lossy subsystems)

## Rules
- Keep this crate TINY — it's just a typed event bus
- No business logic
- No filtering logic (subscribers filter their own events)
- Must never panic on send failure
