# Wakey Architecture Overview

## The Living System

Wakey is not a chatbot with a GUI. It's an event-driven nervous system.

```
                    ┌─────────────────────┐
                    │    WAKEY CORTEX      │
                    │  (Decision Engine)   │
                    │  "Should I speak?    │
                    │   Should I act?      │
                    │   Should I wait?"    │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │    EVENT SPINE       │
                    │  (Central Bus)       │
                    │  Every subsystem     │
                    │  emits & consumes    │
                    │  typed events        │
                    └──────────┬──────────┘
                               │
        ┌──────────┬───────────┼───────────┬──────────┐
        ▼          ▼           ▼           ▼          ▼
   ┌─────────┐┌─────────┐┌─────────┐┌─────────┐┌─────────┐
   │ SENSES  ││ MEMORY  ││ ACTION  ││ PERSONA ││ SKILLS  │
   └─────────┘└─────────┘└─────────┘└─────────┘└─────────┘
```

## Event Spine

The spine is the central nervous system. ALL crate-to-crate communication goes through typed events on tokio broadcast channels.

```rust
enum WakeyEvent {
    // SENSES (input)
    WindowFocusChanged { app: String, title: String },
    ScreenCaptured { image: ImageBuffer, text: Vec<OcrResult> },
    ClipboardChanged { content: String },
    FileChanged { path: PathBuf, kind: ChangeKind },
    GitStatusChanged { repo: PathBuf, branch: String },
    NotificationReceived { app: String, title: String, body: String },
    SystemVitals { battery: u8, cpu: f32, ram: f32 },
    UserIdle { duration: Duration },
    UserReturned,

    // HEARTBEAT (rhythm)
    Tick,
    Breath,
    Reflect,
    Dream,

    // CORTEX (decisions)
    ShouldSpeak { reason: String, urgency: Urgency },
    ShouldAct { plan: ActionPlan },
    ShouldRemember { what: MemoryFragment },

    // ACTION (output)
    MouseMove { x: i32, y: i32 },
    MouseClick { x: i32, y: i32, button: Button },
    KeyType { text: String },
    RunCommand { cmd: String },
    Speak { text: String, emotion: Emotion },

    // PERSONA (state)
    MoodChanged { from: Mood, to: Mood },
    UserModelUpdated { field: String },
}
```

## Heartbeat Layers

Unlike Paperclip's wake/sleep heartbeat, Wakey's heartbeat is continuous consciousness with multiple simultaneous rhythms:

| Layer | Frequency | What It Does | Cost |
|---|---|---|---|
| **Tick** | 2s | Active window, system vitals, cursor | Near-zero |
| **Breath** | 30s | Screenshot → OCR/VLM → context | Low-medium |
| **Reflect** | 15min | Summarize activity, update memory | Medium |
| **Dream** | Daily | Compress memories, learn patterns | High (one-time) |

## Tiered Vision

Screen understanding escalates only when needed:

```
Layer 0 (always-on):  OS Accessibility APIs
                      → window focus, app name, basic text
                      → ~0 cost

Layer 1 (on-change):  Screenshot + Local OCR (Tesseract)
                      → all visible text
                      → ~50MB spike, CPU only

Layer 2 (periodic):   Cloud Vision LLM (Claude/Gemini)
                      → deep semantic understanding
                      → $0.003/screenshot
```

## Tiered Memory (OpenViking Pattern)

```
L0 Abstract:  Quick vector search, ~50 tokens
              "User is a developer who works late"

L1 Overview:  Reranked summary, ~500 tokens
              "User spent today debugging auth middleware,
               was frustrated around 3pm, shipped fix at 6pm"

L2 Detail:    Full content, on-demand
              Complete conversation logs, screenshots, actions
```

## Learning Loop (Hermes Pattern)

```
Task completed → Extract pattern → Create WASM skill
Skill reused   → Track success/failure → Refine
Over months    → LLM calls decrease → Speed increases → Cost → 0
```

## Safety (Cedar Policies)

Every action goes through deterministic policy evaluation:

```cedar
// Example: block destructive commands
forbid(principal, action, resource)
when {
  action == Wakey::Action::"terminal_exec" &&
  context has command &&
  (context.command like "*rm -rf*" || context.command like "*DROP TABLE*")
};

// Example: require confirmation for clicks in banking apps
forbid(principal, action, resource)
when {
  action == Wakey::Action::"mouse_click" &&
  context has app_name &&
  context.app_name like "*bank*"
}
unless { context has user_confirmed && context.user_confirmed == true };
```

## Crate Dependency Graph

```
                    wakey-app
                       │
        ┌──────┬───────┼───────┬──────────┐
        ▼      ▼       ▼       ▼          ▼
    overlay  action  cortex  persona   skills
        │      │       │       │          │
        │      ├───────┤       │          │
        │      ▼       ▼       ▼          │
        │   safety  memory  user-model    │
        │              │                  │
        ├──────────────┼──────────────────┤
        ▼              ▼                  ▼
     heartbeat      senses           learning
        │              │                  │
        ├──────────────┤                  │
        ▼              ▼                  ▼
      spine          spine             spine
        │              │                  │
        ▼              ▼                  ▼
      types          types             types
```
