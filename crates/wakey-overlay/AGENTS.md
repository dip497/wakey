# wakey-overlay — Agent Instructions

This is the **face** — what the user sees. Always-on-top transparent window.

## Requirements
- Transparent background, no window decorations
- Always-on-top (X11 _NET_WM_STATE_ABOVE hint)
- Click-through EXCEPT on the sprite area
- No taskbar icon
- Bottom-right of screen by default, draggable
- Sprite: simple animated character (breathing glow for MVP)
- Chat bubble: shows text when Wakey speaks

## UI Framework
- Use `iced` or `egui/eframe` — NO webview, NO Tauri, NO Electron
- Must be GPU-accelerated but lightweight
- Target: <16ms per frame (60fps) when animating, 0 CPU when idle

## Rules
- Subscribe to spine events for state changes
- Render based on persona mood (from `MoodChanged` events)
- Show chat bubble on `ShouldSpeak` events
- Hide bubble after timeout or user interaction
- Sprite animations: idle, talking, sleeping, thinking, celebrating
