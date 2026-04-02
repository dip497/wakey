Thanks for the enthusiasm, but I want to be straightforward: this feature request falls outside what Wakey is and should be.

## Why This Doesn't Fit Wakey

**Wakey is an AI companion, not a productivity/time-tracking tool.** Its core identity is about being alive on your desktop — perceiving, remembering, learning, and acting. Adding app time-tracking with weekly chart reports and third-party integrations (Toggl, Clockify) would pull it in a fundamentally different direction.

Here are the specific concerns:

### 1. Scope Creep Into a Solved Problem Space

Dedicated time-tracking apps (Toggl, Clockify, RescueTime, ActivityWatch, WakaTime) already do this well. Rebuilding their functionality inside Wakey would be:

- A large engineering surface area (data collection, storage, aggregation, charting UI, OAuth integrations) with no unique value over existing tools.
- A distraction from Wakey's actual differentiators: alive presence, tiered vision, learning loops, personality evolution, Cedar safety policies.

### 2. Conflicts With Wakey's Architecture and Performance Goals

- **Idle RAM target is <20MB.** A charting/graphing subsystem with weekly report generation and persistent time-series data storage would bloat the binary and memory footprint significantly.
- **Wakey is Rust-first, single binary.** Adding chart rendering, HTTP client code for Toggl/Clockify REST APIs, OAuth token management, and a report UI would add substantial dependency weight.
- **Event spine is for internal subsystem communication.** Bolting on external API sync (Toggl/Clockify polling or webhooks) muddies the architecture.

### 3. Privacy and Safety Implications

Detailed per-app time tracking is surveillance-adjacent data. Wakey already has a careful safety model (Cedar policies, trust levels). Adding granular app-usage logging and syncing it to third-party cloud services raises privacy concerns that need very careful treatment — not a bolt-on feature.

## What Wakey Already Provides (and Could Refine)

Wakey's heartbeat system already captures context about what you're doing:

- **Tick (2s):** Active window and app name
- **Breath (30s):** Screenshot and OCR — "what is user doing?"
- **Reflect (15min):** Summarize activity, update working memory
- **Dream (daily):** Compress memories, learn patterns

This means Wakey already *knows* what apps you use and when. The right way to surface this is through Wakey's existing personality and memory systems — for example, Wakey might proactively say "You've been in your browser for 3 hours straight, want to take a break?" or recall "Last week you spent a lot of time in Figma — are you still working on that design project?"

That is the Wakey way: **alive, contextual, conversational** — not a dashboard with pie charts.

## Recommendation

If you need time tracking with charts and Toggl/Clockify integration, use those tools directly — they are purpose-built for it. Wakey should stay focused on what makes it unique: being an intelligent, always-present companion that understands your workflow through its own perception and memory systems, not by reimplementing existing productivity tools.

If there is a specific *companion-flavored* behavior you want (like Wakey reminding you when you've been stuck on one task too long, or noting patterns in your work habits conversationally), that fits naturally into the existing heartbeat + memory + persona architecture and would be worth exploring as a skill.
