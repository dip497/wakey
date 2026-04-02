# Feature Brainstorm

Use this document to brainstorm, validate, and prioritize features.

## Validation Framework

Before adding any feature, answer these questions:

1. **Does it make Wakey more alive?** If it doesn't contribute to the feeling of a living companion, it's a tool feature, not a Wakey feature.
2. **Does it increase idle memory?** If yes, it must justify the cost. 24/7 running means every MB matters.
3. **Can it be a skill instead of core?** If yes, make it a skill. Keep the core minimal.
4. **Does it respect the user?** No dark patterns. No nagging. No guilt. Wakey is a friend, not a productivity cop.
5. **Does it work offline?** Core features should work without internet. Cloud features are optional enhancements.

## Priority Tiers

### P0 — Must Have (MVP)
- [ ] Event spine (crate-to-crate communication)
- [ ] Heartbeat tick (active window tracking)
- [ ] Heartbeat breath (screenshot + basic understanding)
- [ ] Basic overlay (always-on-top transparent window with sprite)
- [ ] Chat bubble (text communication)
- [ ] LLM integration (at least one provider)
- [ ] Basic memory (working memory for current session)
- [ ] Config system (TOML-based)

### P1 — Core Experience
- [ ] Accessibility API integration (Layer 0 vision)
- [ ] OCR integration (Layer 1 vision)
- [ ] VLM integration (Layer 2 vision)
- [ ] Proactive speaking (Wakey initiates conversations)
- [ ] Cedar safety policies
- [ ] Computer use: mouse/keyboard control
- [ ] Computer use: terminal commands
- [ ] Heartbeat reflect (15-min summaries)
- [ ] User model (basic preference tracking)
- [ ] Clipboard monitoring
- [ ] System vitals monitoring

### P2 — Growth
- [ ] Heartbeat dream (daily memory compression)
- [ ] Learning loop (auto-skill extraction)
- [ ] Personality evolution
- [ ] Mood system
- [ ] TTS (text-to-speech)
- [ ] STT (speech-to-text)
- [ ] File system watcher
- [ ] Git status watcher
- [ ] Notification listener
- [ ] Skill marketplace

### P3 — Moonshot
- [ ] Multi-companion support (family of characters)
- [ ] Companion-to-companion communication
- [ ] Browser automation
- [ ] Calendar integration
- [ ] Email/Slack/Discord integration
- [ ] Mobile companion app
- [ ] RL trajectory generation (self-training)
- [ ] Community skill ecosystem
- [ ] Custom sprite/character editor
- [ ] Voice cloning for TTS

## Feature Ideas (Unvalidated)

Add ideas here. They'll be validated against the framework above before moving to a priority tier.

- 
