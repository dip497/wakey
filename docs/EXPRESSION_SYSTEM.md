# 🎨 Wakey Expression System

Wakey now supports **Tabbie-style facial expressions** with smooth transitions, priority-based interruption, and custom user-defined expressions!

---

## 📚 What's New

### Facial Features
- **7 Eye Shapes**: Rectangle, Circle, CurveUp, CurveDown, Dot, Wide, Wink
- **6 Mouth Shapes**: None, Smile, Flat, Open, Wavy, Tongue
- **3 Eyebrow Shapes**: None, Angry, Worried
- **8 Accessories**: Lightbulb, PointingLeft, PointingRight, ThinkingHand, CoffeeCup, Zzz, Heart, Sparkle

### Animation System
- **Priority-based interruption**: Critical expressions interrupt low-priority ones
- **Smooth transitions**: 150ms lerp between expressions (configurable)
- **Animation queue**: Expressions queue up if not high enough priority to interrupt
- **Particle effects**: Sparkles, hearts, and Zzz float around the face

### Custom Expressions
- **JSON config files**: Define your own expressions in `config/expressions/`
- **Event triggers**: Map Wakey events to custom expressions
- **No code required**: Just edit JSON and restart Wakey

---

## 🎭 Built-in Expressions

| Name | Eyes | Mouth | Eyebrows | Accessories | When |
|------|------|-------|----------|-------------|------|
| `neutral` | Rectangle | None | None | - | Default idle |
| `happy` | Rectangle | Smile | None | - | Positive state |
| `celebrate` | CurveUp | Tongue | None | Sparkle | Task completed |
| `idea` | Wide | Smile | None | Lightbulb + PointingRight | Insight moment |
| `angry` | Rectangle | Wavy | Angry | - | Error/frustrated |
| `worried` | Circle | Wavy | Worried | - | Concerned |
| `meeting` | Wink | Flat | None | PointingRight | DND mode |
| `sleepy` | CurveDown | None | None | Zzz | Idle >5min |
| `focused` | Dot | Flat | None | - | Deep work |
| `love` | CurveUp | Smile | None | Heart | Appreciation |
| `surprised` | Wide | Open | Worried | - | Shock/news |
| `thinking` | Dot | Flat | None | ThinkingHand | Contemplating |

---

## 📝 Creating Custom Expressions

### Step 1: Create JSON File

Create a new file in `config/expressions/your_expression.json`:

```json
{
  "name": "my_custom_expression",
  "eyes": "rectangle",
  "mouth": "smile",
  "eyebrows": "none",
  "accessories": ["lightbulb"],
  "duration_ms": 3000,
  "loops": false,
  "priority": "normal",
  "transition_ms": 150
}
```

### Step 2: Choose Properties

#### Eye Shapes
| Value | Appearance | Emotion |
|-------|------------|---------|
| `rectangle` | ⬜ | Neutral, default |
| `circle` | ●● | Alert, focused |
| `curve_up` | ^^ | Happy, laughing |
| `curve_down` | ⌞ | Sad, disappointed |
| `dot` | •• | Small, concentrated |
| `wide` | ⬜ (large) | Surprised, excited |
| `wink` | ⬜— | Playful, knowing |

#### Mouth Shapes
| Value | Appearance | Emotion |
|-------|------------|---------|
| `none` | (empty) | Neutral, calm |
| `smile` | ⌒ | Happy, content |
| `flat` | — | Serious, neutral |
| `open` | O | Surprised, speaking |
| `wavy` | 〰️ | Stressed, uncomfortable |
| `tongue` | ᴗ👅 | Playful, cheeky |

#### Eyebrow Shapes
| Value | Appearance | Emotion |
|-------|------------|---------|
| `none` | (no eyebrows) | Neutral |
| `angry` | ⌞ (angled in) | Angry, annoyed |
| `worried` | ⌝ (angled out) | Worried, concerned |

#### Accessories (up to 3 recommended)
| Value | Appearance | Context |
|-------|------------|---------|
| `lightbulb` | 💡 | Idea, insight |
| `pointing_left` | ☜ | Direction left |
| `pointing_right` | ☞ | Direction right |
| `thinking_hand` | 🤔 | Contemplation |
| `coffee_cup` | ☕ | Break time |
| `zzz` | Zzz | Sleepy, bored |
| `heart` | ❤️ | Love, appreciation |
| `sparkle` | ✨ | Celebration |

#### Priority Levels
| Value | Interrupts | Interrupted By |
|-------|------------|----------------|
| `low` | Nothing | Everything |
| `normal` | Low | High, Critical |
| `high` | Low, Normal | Critical |
| `critical` | Everything | Nothing |

#### Timing
- `duration_ms`: How long to show (0 = indefinite)
- `loops`: Whether to loop the animation (not yet implemented)
- `transition_ms`: How long to transition from previous expression

---

## 🔌 Triggering Expressions

### Automatic Triggers (Built-in)

Wakey automatically shows expressions for these events:

| Event | Expression |
|-------|------------|
| Agent task completed | `celebrate` |
| Agent task failed | `worried` |
| Agent error | `angry` |
| User idle >1min | `thinking` |
| User idle >5min | `sleepy` |
| User returns | `happy` |
| Battery <10% | `worried` |
| Battery 100% | `happy` |
| Notification received | `surprised` |
| Skill extracted | `idea` |
| User speaking (voice) | `thinking` |
| Wakey speaking (voice) | `happy` |

### Custom Triggers (Advanced)

To map custom events to expressions, you'll need to modify the trigger system in code. This will be exposed via config in a future release.

---

## 🎬 Animation Behavior

### Interruption Logic

```
Current: celebrate (high priority, 3s duration)
Event: error (critical priority)
Result: error immediately interrupts celebrate

Current: neutral (low priority)
Event: idea (high priority)
Result: idea interrupts neutral

Current: angry (high priority)
Event: happy (low priority)
Result: happy queues behind angry
```

### Transition Timing

```
Expression A → Expression B
├─ 0-150ms: Smooth lerp transition
├─ 150ms+: Expression B fully visible
└─ B's duration starts after transition completes
```

### Minimum Change Duration

To prevent flickering, expressions can't change more often than every **100ms**. Rapid events within this window will be queued.

---

## 🧪 Testing Your Expressions

### Method 1: Visual Test

1. Create expression JSON in `config/expressions/`
2. Restart Wakey
3. Trigger the expression via an event (e.g., complete a task)
4. Watch the sprite animate!

### Method 2: Code Test

Add a test trigger in your code:

```rust
use wakey_overlay::{Sprite, Expression};

let mut sprite = Sprite::new();

// Queue custom expression
sprite.queue_expression(Expression::celebrate());

// Or load from file
use std::path::Path;
let custom = Expression::load_from_file(Path::new("config/expressions/party.json")).unwrap();
sprite.queue_expression(custom);
```

---

## 📐 Technical Details

### Rendering
- **FPS Target**: 60 FPS (drops to 30 if needed)
- **Breathing Animation**: 0.5 cycles/second, 8% amplitude
- **Transition Easing**: Smoothstep (ease-in-out)
- **Particle System**: Gravity-enabled, lifetime-based

### Performance
- **RAM Usage**: <1MB for animation system
- **CPU Usage**: Negligible (egui painter, no GPU shaders)
- **Asset Loading**: Lazy (expressions loaded on first use)

### File Locations
- **Built-in expressions**: `crates/wakey-overlay/src/expressions.rs`
- **Custom expressions**: `config/expressions/*.json`
- **Trigger mappings**: `crates/wakey-overlay/src/trigger.rs`

---

## 🚀 Example Use Cases

### 1. Celebrate Task Completion
```json
{
  "name": "task_done",
  "eyes": "curve_up",
  "mouth": "tongue",
  "accessories": ["sparkle"],
  "priority": "high",
  "duration_ms": 3000
}
```

### 2. Meeting Mode (DND)
```json
{
  "name": "in_meeting",
  "eyes": "wink",
  "mouth": "flat",
  "accessories": ["pointing_right"],
  "priority": "normal",
  "duration_ms": 0
}
```

### 3. Error State
```json
{
  "name": "error",
  "eyes": "wide",
  "mouth": "wavy",
  "eyebrows": "angry",
  "priority": "critical",
  "duration_ms": 2000
}
```

### 4. Idea Moment
```json
{
  "name": "idea",
  "eyes": "wide",
  "mouth": "smile",
  "accessories": ["lightbulb", "pointing_right"],
  "priority": "high",
  "duration_ms": 2500
}
```

---

## 🛠️ Troubleshooting

### Expression not showing
1. Check JSON syntax (use a JSON validator)
2. Verify file is in `config/expressions/`
3. Restart Wakey
4. Check logs for load errors

### Expression flickers
- Increase `duration_ms` (minimum 100ms between changes)
- Increase `priority` to prevent interruption
- Check if multiple events are firing rapidly

### Transition too slow/fast
- Adjust `transition_ms` (default 150ms)
- Range: 50ms (instant) to 500ms (slow fade)

### Accessories not visible
- Check `show_accessories` in SpriteConfig
- Verify accessory name spelling (snake_case)
- Max 3 accessories recommended

---

## 📚 For Developers

### API Reference

```rust
use wakey_overlay::{Sprite, Expression, AnimationState, ExpressionMapper};

// Create sprite
let mut sprite = Sprite::new();

// Set expression immediately
sprite.set_expression(Expression::celebrate());

// Queue expression (respects priority)
sprite.queue_expression(Expression::angry());

// Load custom expression
let custom = Expression::load_from_file(Path::new("config/expressions/my.json"))?;
sprite.queue_expression(custom);

// Create animation state machine
let mut anim = AnimationState::new();
anim.queue(Expression::happy());

// Map events to expressions
let mapper = ExpressionMapper::new();
let trigger = ExpressionTrigger::Event(my_wakey_event);
let expr = mapper.map(&trigger);
```

### Adding New Eye Shapes

1. Add variant to `EyeShape` enum in `expressions.rs`
2. Add drawing logic in `sprite.rs::draw_eyes()`
3. Update documentation

### Adding New Accessories

1. Add variant to `Accessory` enum in `expressions.rs`
2. Add drawing function in `sprite.rs` (e.g., `draw_my_accessory()`)
3. Call drawing function in `draw_accessories()` match arm

---

## 🎯 Future Enhancements

Planned for future releases:

- [ ] Visual expression editor (GUI)
- [ ] Expression blending (morph between shapes)
- [ ] Custom trigger mappings via config
- [ ] Animated accessories (waving hand, pulsing lightbulb)
- [ ] Full sprite sheet support (import PNG sequences)
- [ ] WASM sandbox for custom animation logic
- [ ] Expression marketplace (share/download community creations)

---

## 📖 Related Docs

- [Sprite Improvements](SPRITE_UX_IMPROVEMENTS.md)
- [Animation Architecture](research/ANIMATION_ARCHITECTURE.md)
- [Tabbie Analysis](research/TABBIE_DEEP_DIVE.md)
- [Action Plan](ANIMATION_ACTION_PLAN.md)

---

**Bottom Line:** You can now create **completely custom expressions** for Wakey using simple JSON files. No coding required! 🎨
