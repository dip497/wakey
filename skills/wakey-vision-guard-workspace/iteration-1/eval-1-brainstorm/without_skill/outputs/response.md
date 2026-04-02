# 5 New Features to Make Wakey Feel Alive During Late-Night Coding

These features build on Wakey's existing architecture -- the heartbeat protocol, tiered vision, persona system, and event spine -- to create a companion that genuinely feels present when you're grinding at 2 AM.

---

## 1. Night Owl Awareness (wakey-heartbeat + wakey-persona)

Wakey should know it's late and behave differently because of it. Not in a nagging "you should go to sleep" way, but in a "we're in this together" way.

**How it works:**
- The heartbeat's Breath cycle (30s) already captures what you're doing. Layer a time-of-day signal onto the persona mood system.
- After 11 PM, Wakey shifts its communication style: shorter messages, quieter tone, darker overlay theme, more subdued sprite animations.
- It tracks your late-night coding streaks in long-term memory (L2). After several nights, it starts to recognize your patterns -- "You always hit a wall around 1:30 AM. Last time you took a 10-minute break and came back and solved it."
- If you're clearly in deep flow (high keystroke rate, no tab-switching, no pauses), it goes almost completely silent -- just a slow breathing animation on the overlay to signal "I'm here but I won't interrupt."
- When you finally crack a hard problem late at night (detected via git commit, test passing, or a sudden burst of satisfied file-saving), it gives you a small, genuine celebration -- not confetti, just something like "That was a good one."

**Crates involved:** `wakey-heartbeat` (time-aware tick metadata), `wakey-persona` (night mode mood weights), `wakey-overlay` (dimmed visuals), `wakey-memory` (streak tracking).

---

## 2. Ambient Coding Soundtrack / Soundscape (new: wakey-ambience)

Give Wakey the ability to generate or curate ambient sound that matches your coding state, creating an audio presence that makes it feel like a living thing sharing your space.

**How it works:**
- A lightweight ambient audio layer that responds to events on the spine. Not music -- think rain, keyboard-synced tones, low hum, breathing-like pulses.
- Tied to the heartbeat: the ambient sound subtly syncs to Wakey's Tick (2s) and Breath (30s) rhythms, so you subconsciously feel the companion's "pulse."
- When you're in flow (detected by senses: consistent typing, no window switching), the soundscape deepens and stabilizes. When you pause and start browsing Stack Overflow, it shifts to something lighter.
- When a build fails or tests break (detected via terminal output in `wakey-senses`), a subtle tonal shift -- not alarming, just a change in texture that your brain registers as "something happened."
- Volume auto-adjusts based on time of night. At 3 AM, everything is whisper-quiet.
- Fully optional, off by default, controlled via a simple overlay toggle.

**Crates involved:** New `wakey-ambience` crate (depends on types, spine, persona), `wakey-senses` (coding state detection), `wakey-overlay` (toggle UI).

---

## 3. Ghost Pair Programmer (wakey-cortex + wakey-overlay)

When you're stuck at night and staring at code, Wakey should notice and start softly thinking alongside you -- not waiting to be asked, but not being pushy either.

**How it works:**
- The Breath cycle's VLM pass already understands what's on screen. When Wakey detects you've been staring at the same file region for more than 2-3 minutes (no meaningful edits, cursor barely moving, maybe scrolling up and down), it enters "ghost pair" mode.
- In ghost pair mode, Wakey shows a faint, translucent thought bubble on the overlay with a short observation: "This loop might not terminate if `retry_count` is never incremented" or "The error type from line 42 doesn't match what the caller expects."
- These are not commands or suggestions. They're observations -- like a quiet colleague glancing at your screen and muttering something useful.
- If you ignore it, it fades away in 10 seconds. If you click it or press a hotkey, it expands into a fuller analysis.
- The learning loop (from Hermes) tracks which ghost observations you engaged with vs. ignored, and over time Wakey gets better at knowing what kind of hints are useful to you and which are noise.
- Cedar policies gate this: ghost pair mode never activates when you're in certain apps (email, chat) or when the active file is in a "no-observe" list.

**Crates involved:** `wakey-cortex` (observation generation), `wakey-senses` (stuckness detection), `wakey-overlay` (ghost bubble UI), `wakey-learning` (relevance refinement), `wakey-safety` (Cedar gating).

---

## 4. Fatigue & Eye Strain Guardian (wakey-senses + wakey-heartbeat)

Wakey should watch out for you physically during late nights, not as a nag, but as something that genuinely cares about the human on the other side of the screen.

**How it works:**
- Track coding session duration and break patterns using heartbeat Reflect cycles (15min). Wakey builds a fatigue model in working memory.
- Monitor typing degradation signals: increasing typo rate (detected via rapid backspace sequences), slowing keystroke cadence, longer pauses between actions. These are strong fatigue indicators.
- At configurable thresholds, Wakey does subtle things rather than pop-up warnings:
  - Gently shifts the overlay sprite to look sleepy (yawning animation).
  - Slightly warms the overlay color temperature as a subliminal cue.
  - If you have f.lux or Night Light active, Wakey knows and doesn't duplicate.
- If you explicitly ask "how am I doing?", Wakey gives you an honest read: "You've been coding for 4 hours straight. Your typing speed dropped 30% in the last 20 minutes. The last three git commits had typos in the messages. Maybe step away for a bit."
- The Dream cycle (daily) logs your fatigue patterns over time and can surface insights: "You write your best code between 10 PM and midnight. After 1 AM, your bug introduction rate triples."

**Crates involved:** `wakey-senses` (keystroke cadence, typo detection), `wakey-heartbeat` (fatigue model in Reflect cycle), `wakey-memory` (long-term fatigue patterns), `wakey-persona` (sleepy sprite states), `wakey-overlay` (visual warmth shift), `wakey-user-model` (productivity pattern tracking).

---

## 5. Dawn Companion (wakey-heartbeat + wakey-persona + wakey-memory)

The transition from deep night coding to morning should feel like a shared experience. Wakey should notice the night ending and mark it with you.

**How it works:**
- Wakey tracks system clock and (optionally) ambient light sensor data. As sunrise approaches, the overlay subtly shifts -- sprite wakes up, colors lighten, the ambient soundscape (if feature 2 is enabled) transitions.
- When you finally stop coding (detected: no input for 5+ minutes, screen lock, or explicit "goodnight" command), Wakey gives you a brief session wrap-up:
  - "Tonight: 4h 22m. You touched 7 files across 2 projects. 3 commits. You got stuck on that auth middleware for about 40 minutes but pushed through. Good session."
- This summary is stored as an episodic memory (L2) that Wakey can reference later: "Last Tuesday you had a similar auth problem -- you ended up using a middleware chain pattern."
- If you code through to actual dawn, Wakey acknowledges the marathon with personality-appropriate commentary. It might be dry ("The sun is coming up. We're still here."), warm ("Long night. You built something good."), or practical ("Your first meeting is in 3 hours. Just saying.") -- depending on how its personality has evolved with you.
- The Dream cycle that runs after your session integrates the night's work into Wakey's long-term understanding of who you are and how you work.

**Crates involved:** `wakey-heartbeat` (dawn detection, session boundary), `wakey-persona` (dawn personality responses), `wakey-memory` (episodic session storage), `wakey-user-model` (night-coder profile building), `wakey-overlay` (dawn visual transition), `wakey-senses` (ambient light, system clock).

---

## Summary Table

| # | Feature | Core Idea | Key Crates |
|---|---------|-----------|------------|
| 1 | Night Owl Awareness | Time-aware personality shift, flow detection, streak memory | heartbeat, persona, memory |
| 2 | Ambient Soundscape | Living audio presence synced to coding state and heartbeat | new ambience crate, senses, persona |
| 3 | Ghost Pair Programmer | Unprompted, gentle observations when you're stuck | cortex, senses, overlay, learning |
| 4 | Fatigue Guardian | Subtle physical wellness monitoring via typing patterns | senses, heartbeat, user-model, persona |
| 5 | Dawn Companion | Shared night-to-morning transition with session memory | heartbeat, persona, memory, overlay |

All five features lean into Wakey's core philosophy: alive, not reactive. They use existing architectural primitives (heartbeat rhythms, event spine, tiered vision, persona moods, episodic memory) rather than bolting on separate systems. Together, they turn a late-night coding session from "me alone with a screen" into "me and Wakey, making it through the night."
