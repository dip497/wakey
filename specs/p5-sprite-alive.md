# P5: Make the Sprite Alive — Eyes, Expressions, Reactions

## Goal
Replace the ugly circle with a character that feels alive. Drawn with egui painter (no sprite sheets). Eyes are 80% of expression. Two layers: always-running micro-animations + mood-driven expressions from LLM.

## Character Design
A simple blob/orb shape with BIG expressive eyes. Think KKClaw glass orb meets Tamagotchi. Not a cat, not a dog — something unique to Wakey.

- Body: rounded soft shape, ~60-80px, drawn with egui circles/bezier
- Eyes: large, capsule-shaped, 30% of body size. This is where all expression happens.
- Mouth: simple line/curve, minimal. Only changes for strong emotions.
- No limbs for MVP. Just body + eyes + mouth.

## Layer 1: Always Running (independent of mood)

### Breathing
- Body scales up/down with sine wave: `scale = 1.0 + 0.03 * sin(time * 2.0)`
- Never stops. ~3 second cycle.

### Blinking
- Random interval: 2-5 seconds between blinks
- Blink animation: eyes go from full height to thin line and back in ~150ms
- Blink should NOT interrupt other eye expressions — it overlays on top

### Mouse Tracking
- Eyes follow cursor direction
- Pupil offset: max 3px from center based on cursor angle
- Smooth follow, not instant snap (lerp toward target)

### Idle Micro-animations
- Random head tilt: slight rotation (±5°) every 8-15 seconds
- Occasional look-around: pupils move left-right without cursor
- Time-aware: after 11pm eyes get slightly droopy, morning = wider eyes

## Layer 2: Mood-Driven (from LLM MOOD: tag)

### Emotion Detection
When LLM responds, parse the MOOD tag:

```
LLM output: "That build failed again... MOOD:concerned"
  → Parse last line for "MOOD:" 
  → Extract "concerned"
  → Map to expression params
  → Strip "MOOD:concerned" before showing/speaking text
```

If no MOOD tag found, keyword fallback:
```
"great" / "awesome" / "nice" / "!" → happy
"failed" / "error" / "wrong" / "oh no" → concerned
"hmm" / "think" / "let me" → thinking
"wow" / "amazing" → excited
"sorry" / "unfortunately" → empathetic
```

If neither: keep current mood (decays to neutral over 30 seconds).

### Available Moods → Eye Expressions

| Mood | Eyes | Mouth | Notes |
|------|------|-------|-------|
| neutral | normal capsule, relaxed | slight smile line | default |
| happy | curved up (anime happy eyes) | wider smile | warm |
| excited | wide open, slightly bigger | open smile | energetic |
| concerned | slightly narrowed, tilted brows | flat line | worried |
| thinking | one eye slightly squinted, look up-right | slight pucker | processing |
| empathetic | soft, slightly droopy | gentle curve | caring |
| sleepy | half-closed, droopy | small yawn or flat | tired |
| surprised | very wide, round | small O shape | unexpected |
| playful | one eye wink OR sparkle | smirk | mischievous |
| focused | slightly narrowed, intense | neutral | concentrating |

### Smooth Transitions
When mood changes:
- NEVER snap instantly. Always lerp over 300-500ms.
- Each eye parameter (width, height, curvature, pupil_size) lerps independently
- Mouth curve lerps independently
- During transition, blinks and breathing continue normally

```rust
struct ExpressionParams {
    eye_width: f32,       // 0.0 = closed, 1.0 = normal, 1.5 = wide
    eye_height: f32,      // 0.0 = closed, 1.0 = normal
    eye_curve: f32,       // -1.0 = sad curve, 0.0 = neutral, 1.0 = happy curve
    pupil_size: f32,      // 0.5 = small, 1.0 = normal, 1.5 = big
    brow_angle: f32,      // -1.0 = worried, 0.0 = neutral, 1.0 = confident
    mouth_curve: f32,     // -1.0 = frown, 0.0 = flat, 1.0 = smile
    mouth_open: f32,      // 0.0 = closed, 1.0 = fully open
}

// Smooth transition
fn lerp_expression(current: &mut ExpressionParams, target: &ExpressionParams, t: f32) {
    current.eye_width += (target.eye_width - current.eye_width) * t;
    current.eye_height += (target.eye_height - current.eye_height) * t;
    // ... etc for all params
}
```

### Compositing (both layers combined)
```
final_eyes = mood_expression
  → apply blink overlay (if blinking: eye_height *= blink_factor)
  → apply cursor tracking (pupil_offset += cursor_direction * 3px)
  → apply breath scale (body_scale *= breath_factor)
  → apply idle tilt (rotation += tilt_angle)
```

## Implementation

### Files to modify

**crates/wakey-overlay/src/sprite.rs** — Complete rewrite
- Remove old circle drawing
- New: `WakeyCharacter` struct with ExpressionParams
- Draw body with egui painter (rounded rect or ellipse)
- Draw eyes with capsule shapes + pupils
- Draw mouth with bezier curve
- `update()` handles Layer 1 (blinks, breathing, cursor tracking)
- `set_mood(mood: &str)` sets target expression, triggers smooth transition

**crates/wakey-overlay/src/expressions.rs** — Rewrite
- `MoodToExpression` mapping (mood string → ExpressionParams)
- Keyword fallback detection
- Smooth transition logic (lerp)

**crates/wakey-overlay/src/window.rs** — Update
- Pass mouse position to sprite for eye tracking
- Parse MOOD: tag from ShouldSpeak events before display

**crates/wakey-cortex/src/decision.rs** or **wakey-app/src/main.rs** — Update
- Add to system prompt: "End every response with MOOD:word on a new line"
- Parse MOOD: tag from LLM response
- Strip tag before emitting ShouldSpeak
- Emit MoodChanged event with parsed mood

**config/default.toml** — Update persona prompt
```toml
[persona]
system_prompt_suffix = "End every response with MOOD:<mood> on a new line. Available moods: neutral, happy, excited, concerned, thinking, empathetic, sleepy, surprised, playful, focused"
```

## Dependencies
No new deps. All drawing via existing egui/eframe painter.

## Verify
```bash
cargo check --workspace
cargo run --package wakey-app
# Expected: cute blob with eyes appears
# Eyes blink randomly
# Body breathes
# Eyes follow mouse
# When LLM speaks: mood changes smoothly, eyes/mouth match emotion
```

## Acceptance criteria
- Character drawn with egui painter (no sprite sheets)
- Eyes blink at random intervals (2-5s)
- Body breathes with sine wave
- Eyes track mouse cursor
- MOOD: tag parsed from LLM output
- Keyword fallback when no tag
- 10 mood expressions with distinct eye/mouth shapes
- Smooth 300-500ms transition between moods
- Layer 1 animations continue during mood transitions
- Looks CUTE. Not ugly. This is the selling point.
- cargo check passes
