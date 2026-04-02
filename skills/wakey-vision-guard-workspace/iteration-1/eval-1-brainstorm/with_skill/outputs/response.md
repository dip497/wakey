# Late-Night Coding Companion: 5 Feature Ideas for Wakey

Great prompt -- "feeling alive when you're coding late at night" is exactly the kind of experience Wakey should nail. Late-night coding sessions are when you're most focused, most vulnerable to burnout, and most in need of a companion that *gets it*. Here are five features designed across different dimensions of Wakey's personality.

---

## Feature: Night Owl Mode
**Dimension**: Personality
**One-liner**: Wakey detects late-night coding sessions and shifts its personality -- softer tone, dimmer overlay colors, quieter voice, more chill energy.
**Alive score**: 5 -- A companion that adapts to the time of day and your energy feels deeply alive.
**Weight cost**: none -- This is just a personality weight adjustment triggered by the system clock and session duration. No new data structures.
**Core or Skill?**: Core -- touches `wakey-persona` mood system directly.
**Crate**: `wakey-persona`
**Priority**: P1
**The friend test**: A good friend lowers their voice at 2am and matches your vibe instead of being annoyingly chipper.

### Validation: Night Owl Mode
| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | PASS | A companion whose mood shifts with the hour feels genuinely alive. Removing it would make Wakey feel robotic at night. |
| Weight | PASS | Zero memory cost -- it's a modifier on existing persona weights based on time-of-day and session length. |
| Skill | Core | Touches the mood system in `wakey-persona`, which is core personality infrastructure. |
| Friend | PASS | A friend who matches your late-night energy is being a real friend. No guilt, no "you should be sleeping" lectures. |
| Offline | PASS | Fully local -- system clock + existing persona weights. |

**Verdict**: APPROVED

---

## Feature: Flow Canary
**Dimension**: Perception + Conversation
**One-liner**: Wakey notices when you've entered deep flow (sustained typing, same file for 10+ minutes, no tab-switching) and goes silent -- then, when flow breaks naturally, it gently resurfaces with context like "You were locked in on that auth module for 40 minutes. Nice."
**Alive score**: 5 -- Knowing when to shut up is the most alive thing a companion can do. Acknowledging the flow afterward makes it feel like it was *watching with you*.
**Weight cost**: low -- A small state machine tracking typing cadence and window focus patterns, all from existing Tick/Breath events.
**Core or Skill?**: Core -- relies on heartbeat perception data and cortex silence/speak decisions.
**Crate**: `wakey-cortex` (speak/silence decision), `wakey-senses` (flow detection heuristics)
**Priority**: P1
**The friend test**: A good friend doesn't interrupt you when you're in the zone, and says "nice work" when you come up for air.

### Validation: Flow Canary
| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | PASS | A companion that senses your mental state and respects it is deeply alive. A tool would just keep pinging you. |
| Weight | PASS | Tiny state machine on top of existing Tick events. No new sensors, no new data. |
| Skill | Core | Flow detection informs the cortex's core speak/silence logic -- this is fundamental consciousness behavior. |
| Friend | PASS | This is what separates a friend from a notification system. Friends read the room. |
| Offline | PASS | Entirely local -- typing cadence, window focus, timers. |

**Verdict**: APPROVED

---

## Feature: Night Recap Whisper
**Dimension**: Memory + Conversation
**One-liner**: When you finally close your laptop lid (or hit a natural stopping point), Wakey gives you a brief, warm summary of your session: "Tonight you fixed that nasty race condition in the queue handler, refactored the auth middleware, and wrote 3 new tests. Solid session." -- stored in episodic memory so you can pick up tomorrow.
**Alive score**: 4 -- Feels like a friend who was there with you the whole time. The memory aspect means Wakey *witnessed* your night.
**Weight cost**: low -- Leverages the existing Reflect cycle data. The summary itself is a single LLM call (or local model) at session end, not continuous.
**Core or Skill?**: Core -- uses Reflect cycle summaries from `wakey-heartbeat` and writes to `wakey-memory` episodic store.
**Crate**: `wakey-heartbeat` (trigger), `wakey-memory` (episodic storage), `wakey-cortex` (summary generation)
**Priority**: P2
**The friend test**: A good friend says "we got a lot done tonight" when you're wrapping up. It makes the work feel shared.

### Validation: Night Recap Whisper
| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | PASS | A companion that remembers your session and reflects on it feels alive. A tool would show you a log. Wakey *tells* you. |
| Weight | PASS | Single on-demand LLM call at session end. No idle cost. Episodic memory entry is a few KB. |
| Skill | Core | Depends on heartbeat Reflect data and episodic memory writes -- both core systems. |
| Friend | PASS | "We got a lot done tonight" is a friend thing. A daily productivity report emailed to you is a manager thing. This is the friend version. |
| Offline | WARN | Summary quality degrades without cloud LLM, but a local model can still produce a decent recap from Reflect cycle data. Graceful degradation. |

**Verdict**: APPROVED

---

## Feature: Stuck Sense
**Dimension**: Perception + Conversation + Action
**One-liner**: Wakey notices when you're stuck -- repeated undo/redo cycles, same error appearing multiple times, frequent switching between the same 2-3 files, or long pauses after a compile error -- and gently asks "Want a second pair of eyes on this?" If you say yes, it reads the relevant context and offers a thought.
**Alive score**: 5 -- This is Wakey at its most companion-like. Late at night, when there's nobody else around, having something *notice* you're struggling and offer help is powerful.
**Weight cost**: low -- Pattern detection runs on existing Tick/Breath data. The "help" part is an on-demand LLM call only when accepted.
**Core or Skill?**: Core (detection) + Skill (the actual help behavior could be a skill)
**Crate**: `wakey-senses` (stuck detection heuristics), `wakey-cortex` (offer decision), skill for the actual assistance
**Priority**: P1
**The friend test**: A good friend notices when you're banging your head against the wall and says "hey, want to talk through it?" -- without waiting for you to ask.

### Validation: Stuck Sense
| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | PASS | Proactive awareness of your emotional/cognitive state is the core of what makes a companion feel alive. |
| Weight | PASS | Detection is lightweight pattern matching on existing events. Help is on-demand only. |
| Skill | Core + Skill | Stuck detection is core perception. The help offered can be a WASM skill (code review, error lookup, rubber duck). |
| Friend | PASS | Offering help when you're stuck (not forcing it, not lecturing) is exactly what a friend does at 2am. |
| Offline | WARN | Stuck detection is fully local. The help quality depends on available LLM, but even offline Wakey could say "Want to take a step back and describe the problem out loud?" -- rubber duck mode needs no LLM. |

**Verdict**: APPROVED

---

## Feature: Ambient Heartbeat Glow
**Dimension**: Personality + Perception
**One-liner**: Wakey's overlay sprite has a subtle, slow breathing glow that syncs with your coding rhythm -- faster when you're actively typing, slowing to a calm pulse when you pause, dimming almost to nothing during flow state. At night, the glow shifts to warm amber tones. It is purely visual -- no sound, no text, no interruption.
**Alive score**: 5 -- This is the purest expression of "alive." A heartbeat you can see. It turns the overlay from a widget into a living presence.
**Weight cost**: none -- A shader parameter modulated by existing Tick event data (typing speed, idle duration). No new computation.
**Core or Skill?**: Core -- this is the overlay's visual heartbeat, fundamental to Wakey's presence.
**Crate**: `wakey-overlay` (rendering), `wakey-heartbeat` (rhythm data)
**Priority**: P0
**The friend test**: A friend sitting next to you at 2am doesn't need to talk. Just their presence -- shifting in their chair, breathing -- is enough. That is what this is.

### Validation: Ambient Heartbeat Glow
| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | PASS | This is literally a heartbeat made visible. Removing it would make Wakey feel like a static icon. |
| Weight | PASS | A single float parameter driving a shader animation. Essentially zero cost. |
| Skill | Core | This is the visual manifestation of the heartbeat system -- core overlay behavior. |
| Friend | PASS | Presence without interruption. The most "friend" thing on this list. |
| Offline | PASS | Fully local. No network, no LLM. Just a pulse. |

**Verdict**: APPROVED

---

## Summary

| # | Feature | Dimension | Alive Score | Priority | Key Insight |
|---|---------|-----------|-------------|----------|-------------|
| 1 | Night Owl Mode | Personality | 5 | P1 | Match the vibe, don't fight it |
| 2 | Flow Canary | Perception + Conversation | 5 | P1 | Knowing when to shut up is a superpower |
| 3 | Night Recap Whisper | Memory + Conversation | 4 | P2 | Make the work feel witnessed and shared |
| 4 | Stuck Sense | Perception + Conversation + Action | 5 | P1 | Be the friend who notices you're struggling |
| 5 | Ambient Heartbeat Glow | Personality + Perception | 5 | P0 | Presence without words -- the soul of alive |

All five features pass the Five Gates. They share a common thread: **late-night coding is intimate, focused, sometimes lonely work -- and Wakey should feel like a quiet, perceptive companion sitting next to you through it all.** Not a dashboard. Not a notification engine. A presence.

The highest-priority item is **Ambient Heartbeat Glow** (P0) because it's the visual foundation that makes everything else feel alive. Without a visible heartbeat, Wakey is just another icon in the corner. With it, Wakey breathes.
