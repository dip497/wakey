# Research: Hermes Agent Implementation Patterns

## Context
We're building Wakey — an open-source AI companion in Rust. Hermes Agent (Python, by Nous Research) has the best self-improving learning loop in open source. We need to understand HOW it works at code level before building our Rust version.

Hermes Agent repo: https://github.com/NousResearch/hermes-agent
Clone to /tmp/hermes-agent if needed.

## Goal
Understand the learning loop, skill extraction, and user modeling implementation in detail.

## Focus Areas

### 1. Learning Loop (Skill Extraction)
- How does Hermes detect that a task is worth turning into a skill?
- What format are skills stored in?
- How are skills loaded and matched to future tasks?
- How does skill refinement work on reuse?
- Find the actual code that does extraction.

### 2. User Modeling (Honcho Integration)
- How is the user model structured?
- What data points does it track?
- How does it influence agent behavior?
- How is it persisted?

### 3. Memory System
- How does SQLite FTS5 memory work?
- How is summarization done?
- How are MEMORY.md and USER.md maintained?
- What triggers memory writes vs reads?

### 4. Agent Loop
- What is the core orchestration loop?
- How does it decide what tool to use?
- How does prompt building work?
- How does context compression work?

### 5. Skill Format & Registry
- What does a skill file look like?
- How is the skills directory structured?
- How are skills discovered and loaded?

## Output
Write a structured research report to: /home/dipendra-sharma/projects/wakey/docs/research/hermes-impl.md

Include actual code snippets. For each pattern, note what we should adapt for Wakey's Rust implementation.

## Acceptance Criteria
- [ ] Report contains actual code snippets from Hermes source
- [ ] All 5 focus areas covered
- [ ] Each pattern has "for Wakey (Rust)" adaptation notes
- [ ] Report saved to correct path
