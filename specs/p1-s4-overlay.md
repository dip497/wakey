# P1-S4: Overlay Window

## Goal
Always-on-top transparent window with animated breathing sprite and chat bubble.

## Crate
wakey-overlay (all files)

## What to implement

### Window (src/window.rs)
1. Use `eframe` (egui's framework) to create a window
2. Window properties: transparent, always-on-top, no decorations, no taskbar icon
3. Position: bottom-right corner, ~200x200px
4. On Linux/X11: set _NET_WM_STATE_ABOVE, _NET_WM_WINDOW_TYPE_DOCK hints
5. Click-through on transparent areas

### Sprite (src/sprite.rs)
1. Simple animated circle that "breathes" (scales up/down with sine wave)
2. Warm amber glow color
3. Two eyes (simple dots) that occasionally blink
4. 60fps when animating, 0 CPU when nothing changes
5. egui::painter for drawing (no sprite sheets for MVP)

### Chat Bubble (src/bubble.rs)
1. Rounded rectangle above the sprite
2. Text with typewriter effect (chars appear one by one)
3. Auto-hide after 10 seconds
4. Show on WakeyEvent::ShouldSpeak events from spine

### Integration
1. Subscribe to spine events (ShouldSpeak, MoodChanged, Shutdown)
2. Run egui event loop on main thread
3. Heartbeat tick data drives sprite animation speed

## Read first
- crates/wakey-overlay/AGENTS.md
- docs/VIBE.md (the alive loop concept)
- Research eframe/egui transparent window on Linux

## Verify
```bash
cargo run --package wakey-app  # should show overlay window
```

## Acceptance criteria
- Transparent always-on-top window appears
- Sprite breathes (smooth sine animation)
- Eyes blink periodically
- Chat bubble shows text when ShouldSpeak event received
- Window stays on top of other apps
- Ctrl+C shuts down cleanly
- cargo check passes
