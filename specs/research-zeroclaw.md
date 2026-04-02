# Research: ZeroClaw Implementation Patterns

## Context
We're building Wakey — an open-source AI companion in Rust. Before implementing, we need to study how ZeroClaw (a similar Rust AI agent) solves key problems.

ZeroClaw repo is at: /home/dipendra-sharma/projects/zeroclaw/
If that doesn't exist, clone from: https://github.com/zeroclaw-labs/zeroclaw

## Goal
Extract concrete implementation patterns we can learn from. Not architecture docs — actual code patterns.

## Focus Areas

### 1. Trait System
- How are `Provider`, `Channel`, `Tool`, `Memory` traits defined?
- How are they registered at runtime (factory pattern)?
- How does config drive which implementation gets used?
- Find the actual trait definitions and paste key snippets.

### 2. Event/Message Routing
- How do messages flow between subsystems?
- Is there a central bus? Channels? Direct calls?
- How is async handled?

### 3. Provider Abstraction (LLM Client)
- How does the OpenAI-compatible client work?
- How is streaming handled?
- How are multiple providers configured and switched?
- What HTTP client do they use?

### 4. Memory Backend
- How does the SQLite memory work?
- How is vector search implemented in pure Rust?
- How is the Memory trait defined?

### 5. Build/Binary Optimization
- What Cargo profile settings do they use for size?
- How do they achieve <5MB binary?

## Output
Write a structured research report to: /home/dipendra-sharma/projects/wakey/docs/research/zeroclaw-impl.md

Include actual code snippets (not just descriptions). For each pattern, note:
- What they did
- Why it works
- What we should adopt/adapt for Wakey
- What we should do differently

## Acceptance Criteria
- [ ] Report contains actual code snippets from ZeroClaw source
- [ ] All 5 focus areas are covered
- [ ] Each pattern has a "for Wakey" recommendation
- [ ] Report is saved to the correct path
