# Expression Decision System

**Question:** Who decides which expression Wakey shows?

**Answer:** It's a **3-layer decision system** with automatic triggers, manual overrides, and customization support.

---

## 🎯 Decision Flow

```
WakeyEvent → ExpressionMapper → Sprite
     ↓              ↓              ↓
  (trigger)    (decision)    (display)
```

---

## 📋 Layer 1: Automatic Event Triggers (Primary)

**File:** `crates/wakey-overlay/src/trigger.rs`

The `ExpressionMapper` automatically maps `WakeyEvent` → `Expression`:

| Event | Expression | Why |
|-------|------------|-----|
| `AgentCompleted` | `celebrate` | Task finished successfully |
| `AgentFailed` | `worried` | Something went wrong |
| `AgentError` | `angry` | System error occurred |
| `UserIdle` (>5min) | `sleepy` | User has been away |
| `UserIdle` (>1min) | `thinking` | User paused briefly |
| `UserReturned` | `happy` | User came back |
| `NotificationReceived` | `surprised` | New notification |
| `SkillExtracted` | `idea` | New skill learned |
| `VoiceUserSpeaking` | `thinking` | Listening to user |
| `VoiceWakeySpeaking` | `happy` | Wakey is talking |
| `MoodChanged` | `from_mood(mood)` | Mood-based expression |

### How It Works

```rust
// In window.rs event handler:
WakeyEvent::AgentCompleted { .. } => {
    if let Some(trigger) = crate::trigger::event_to_trigger(&event) {
        if let Some(expr) = overlay_state.mapper.map(&trigger) {
            overlay_state.sprite.set_expression(expr);
        }
    }
}
```

**Benefits:**
- ✅ Zero manual intervention
- ✅ Consistent behavior
- ✅ Context-aware expressions
- ✅ Easy to extend (add new mappings in `trigger.rs`)

---

## 🎮 Layer 2: Manual Override (Debug/Custom)

**File:** `crates/wakey-overlay/src/window.rs`

Developers can directly set expressions:

```rust
// Direct override
overlay_state.sprite.set_expression(Expression::happy());

// Mood-based
overlay_state.sprite.set_expression(Expression::from_mood(mood));

// Custom expression from config
let custom = Expression::load_from_file("config/expressions/party.json")?;
overlay_state.sprite.set_expression(custom);
```

**Use Cases:**
- Debugging/testing
- Special UI states
- Custom integrations
- One-off expressions

---

## ⚙️ Layer 3: Custom Configs (Extensibility)

**Directory:** `config/expressions/`

Users can create custom expressions via JSON:

```json
{
  "name": "party",
  "glow_color": [0.9, 0.3, 0.9, 0.9],
  "eyes": "wide",
  "mouth": "tongue",
  "eyebrows": "none",
  "accessories": ["sparkle", "heart"]
}
```

**Load at runtime:**
```rust
let party = Expression::load_from_file("config/expressions/party.json")?;
sprite.set_expression(party);
```

**Use Cases:**
- Custom branding
- Seasonal themes (Halloween, Christmas)
- User preferences
- Community expression packs

---

## 🔄 Priority System

Expressions can interrupt based on priority:

```rust
#[derive(PartialOrd, Ord)]
pub enum Priority {
    Low = 0,      // Background animations
    Normal = 1,   // Standard expressions
    High = 2,     // Important events
    Critical = 3, // Errors, alerts
}
```

**Example:**
```rust
// Low priority idle animation
sprite.queue_expression(Expression::neutral());

// High priority interrupt - will override immediately
sprite.set_expression(Expression::angry()); // Critical priority
```

---

## 📊 Decision Matrix

| Who Decides? | When? | How? | Override? |
|--------------|-------|------|-----------|
| **Event Triggers** | Every WakeyEvent | Automatic mapping | Yes (higher priority) |
| **Mood System** | Mood changes | `from_mood()` | Yes (Critical events) |
| **Manual Code** | Developer choice | `set_expression()` | Yes (Critical events) |
| **User Config** | On load/custom action | JSON files | No (must be triggered) |

---

## 🛠️ Adding New Triggers

### Step 1: Add to `event_to_trigger()`

```rust
// In trigger.rs
pub fn event_to_trigger(event: &WakeyEvent) -> Option<ExpressionTrigger> {
    match event {
        // Existing mappings...
        
        // New trigger
        WakeyEvent::SystemLowBattery { percent } => {
            if *percent < 10 {
                Some(ExpressionTrigger::Custom("low_battery".into()))
            } else {
                None
            }
        }
        
        _ => None,
    }
}
```

### Step 2: Add to `ExpressionMapper::map()`

```rust
// In trigger.rs
pub fn map(&self, trigger: &ExpressionTrigger) -> Option<Expression> {
    match trigger {
        // Existing mappings...
        
        // New mapping
        ExpressionTrigger::Custom(name) if name == "low_battery" => {
            Some(Expression::worried()) // or custom expression
        }
        
        _ => None,
    }
}
```

### Step 3: (Optional) Add Custom Expression

```json
// config/expressions/low_battery.json
{
  "name": "low_battery",
  "glow_color": [0.9, 0.5, 0.0, 0.8],
  "eyes": "worried",
  "mouth": "flat",
  "eyebrows": "worried",
  "accessories": []
}
```

---

## 🎯 Current Implementation Status

| Component | Status | File |
|-----------|--------|------|
| Event-to-Trigger Conversion | ✅ Complete | `trigger.rs:95-150` |
| ExpressionMapper | ✅ Complete | `trigger.rs:19-90` |
| Integration in window.rs | ✅ Complete | `window.rs:260-320` |
| Priority System | ✅ Complete | `expressions.rs:40-50` |
| Custom JSON Configs | ✅ Complete | `expressions.rs:150-200` |
| Default Mappings (10+) | ✅ Complete | `trigger.rs:30-90` |

---

## 🦞 Example: Full Decision Flow

```
1. User says: "Hey Wakey, what's the weather?"
   ↓
2. VoiceUserSpeaking event fired
   ↓
3. event_to_trigger() → ExpressionTrigger::UserSpeaking
   ↓
4. mapper.map() → Expression::thinking()
   ↓
5. sprite.set_expression(thinking)
   ↓
6. Sprite renders thinking face (dot eyes, flat mouth)

7. Wakey finds answer successfully
   ↓
8. AgentCompleted event fired
   ↓
9. event_to_trigger() → ExpressionTrigger::AgentCompleted
   ↓
10. mapper.map() → Expression::celebrate()
    ↓
11. sprite.set_expression(celebrate) [interrupts thinking]
    ↓
12. Sprite renders celebration (wide eyes, smile, sparkles)
```

---

## 🔧 Customization Points

### For Developers
- Add new event triggers in `trigger.rs`
- Override expressions in `window.rs`
- Create custom expressions in `config/expressions/`

### For Users
- Edit JSON configs in `config/expressions/`
- Adjust mood-to-expression mapping (future feature)
- Create expression packs (shareable JSON bundles)

### For Future
- GUI expression editor
- Expression marketplace
- Community presets
- Machine learning (learn user preferences)

---

## 📝 Summary

**Who decides?**

1. **Automatic triggers** (90% of cases) - Event-driven, zero manual work
2. **Mood system** (5% of cases) - Context-aware fallbacks
3. **Manual override** (5% of cases) - Debug/special scenarios
4. **Custom configs** (extensibility) - User/developer customization

**The system is designed to be automatic by default, customizable when needed.** 🦞
