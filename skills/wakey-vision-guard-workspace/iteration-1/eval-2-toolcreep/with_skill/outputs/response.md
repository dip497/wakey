# Feature Request: App Time Tracking + Weekly Reports + Toggl/Clockify Integration

Thanks for thinking about how Wakey could help you understand your time! Let me break this into its component parts and run each through the validation gates.

---

## Part 1: Per-App Time Tracking with Weekly Reports (Charts & Graphs)

### Validation: App Time Tracking Dashboard

| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | **FAIL** | A weekly report with charts and graphs is a dashboard, not a companion behavior. It makes Wakey feel like a productivity analytics tool (think RescueTime or Screen Time), not a living friend. |
| Weight | **WARN** | Persistent per-app timing data, chart rendering libraries, and report generation would add meaningful weight to an always-on companion that targets <20MB idle. |
| Skill | Skill | If built at all, this would be a WASM skill, not core. It has nothing to do with the event spine or heartbeat. |
| Friend | **FAIL** | A good friend does not hand you a spreadsheet of how you spent your week. That is what a manager or a time-tracking tool does. "You spent 4h 23m in Slack this week" is surveillance energy, not friendship. |
| Offline | PASS | Time tracking itself could work offline. |

**Verdict**: **REJECTED** in its current form.

---

## Part 2: Toggl / Clockify Integration

### Validation: Third-Party Time Tracker Integration

| Gate | Result | Reasoning |
|------|--------|-----------|
| Alive | **FAIL** | Integrating with enterprise time-tracking SaaS products does not make your laptop feel more alive. It makes Wakey feel like a middleware connector. |
| Weight | **WARN** | HTTP clients, OAuth flows, API polling, and sync state for two external services add non-trivial complexity and dependency weight. |
| Skill | Skill | Definitely not core. |
| Friend | **FAIL** | A friend does not clock your hours and submit timesheets for you. This is a tool integration, not a companion behavior. |
| Offline | **FAIL** | Completely dependent on external cloud services. Broken offline. |

**Verdict**: **REJECTED**

---

## Course Correction: Tool Creep Detected

I love the intent here -- you want Wakey to help you be more aware of how you spend your time. That is a genuinely good instinct. But the way it is framed -- dashboards, charts, weekly reports, third-party integrations -- is pure **tool creep**. It turns Wakey from a living companion into a time-tracking app with a personality skin.

Ask yourself: would a good friend hand you a bar chart of your week? No. But a good friend *might*:

### What Wakey Could Do Instead (The Companion Way)

**"Hey, you've been in Slack for a while -- everything okay, or just a rabbit hole?"**
Wakey already has tiered vision (Layer 0 knows the active app). If it notices you have been in a single app for an unusually long time, it could gently mention it -- the way a friend sitting next to you might say "you've been on your phone for ages." No charts. No data. Just a nudge.

**"You seemed really focused today -- nice flow session this afternoon."**
During the Reflect cycle (every 15 minutes), Wakey is already summarizing activity. It could notice patterns and comment on them *conversationally* -- "You had a solid coding block today" or "Lots of context-switching this morning, that can be draining." This is a friend noticing your day, not a tool generating a report.

**"You've been jumping between apps a lot -- want me to close some distractions?"**
Instead of *reporting* behavior after the fact, Wakey can *act in the moment* -- offering to help when it notices a pattern, rather than passively logging data for a weekly review.

### A Companion-Aligned Mini-Proposal

```
## Feature: Activity Awareness (Conversational)
**Dimension**: Perception + Conversation
**One-liner**: Wakey notices app-usage patterns and mentions them naturally in conversation, like a friend would.
**Alive score**: 4 -- reacting to real behavior makes Wakey feel perceptive and present.
**Weight cost**: none -- reuses existing Layer 0 perception and Reflect cycle data. No new subsystems.
**Core or Skill?**: Core (uses heartbeat Reflect cycle + existing Layer 0 senses)
**Crate**: wakey-heartbeat (Reflect cycle) + wakey-persona (decides how/when to comment)
**Priority**: P2
**The friend test**: A good friend notices when you have been staring at one thing too long and says something. Yes, this passes.
```

### The Bottom Line

Wakey already has the *perception* to know what app you are in (Layer 0 accessibility APIs) and the *rhythm* to reflect on patterns (the 15-minute Reflect cycle). The data is there. The difference is **how it surfaces**: not as charts, graphs, and reports, but as natural, conversational observations from a companion who is paying attention.

No dashboards. No integrations. No weekly PDFs. Just a friend who notices.
