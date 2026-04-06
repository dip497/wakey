//! Overlay window for Wakey.
//!
//! Always-on-top transparent window positioned in bottom-right corner.
//! Uses eframe (egui framework) for rendering.

use crate::{bubble::Bubble, expressions::Expression, sprite::Sprite, trigger::ExpressionMapper};
use eframe::egui::Pos2;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use wakey_spine::Spine;
use wakey_types::WakeyEvent;
use wakey_types::event::Mood;

/// Configuration for the overlay window.
pub struct OverlayConfig {
    /// Window size (width, height)
    pub size: (f32, f32),
    /// Gap from screen edge
    pub edge_gap: f32,
    /// Base sprite size
    pub sprite_size: f32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            size: (220.0, 220.0),
            edge_gap: 20.0,
            sprite_size: 50.0,
        }
    }
}

/// State shared between the async event loop and the GUI thread.
pub struct OverlayState {
    /// The animated sprite
    pub sprite: Sprite,
    /// The chat bubble
    pub bubble: Bubble,
    /// Current mood
    pub mood: Mood,
    /// Whether shutdown was requested
    pub shutdown_requested: bool,
    /// Last update time
    pub last_update: Instant,
    /// Voice mode state
    pub voice_state: VoiceState,
    /// Expression trigger mapper
    pub mapper: ExpressionMapper,
}

/// Voice mode state for the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceState {
    /// Not in voice mode
    #[default]
    Idle,
    /// Listening for user speech
    Listening,
    /// Processing/thinking
    Thinking,
    /// Wakey is speaking
    Speaking,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            sprite: Sprite::new(),
            bubble: Bubble::new(),
            mood: Mood::Neutral,
            shutdown_requested: false,
            last_update: Instant::now(),
            voice_state: VoiceState::default(),
            mapper: ExpressionMapper::new(),
        }
    }
}

/// The main overlay application (implements eframe::App).
pub struct OverlayApp {
    /// Shared state (protected by mutex for cross-thread access)
    state: Arc<Mutex<OverlayState>>,
    /// Configuration
    config: OverlayConfig,
    /// Flag to signal when the app should close
    should_close: Arc<AtomicBool>,
}

impl OverlayApp {
    /// Create a new overlay app with shared state.
    pub fn new(
        state: Arc<Mutex<OverlayState>>,
        config: OverlayConfig,
        should_close: Arc<AtomicBool>,
    ) -> Self {
        Self {
            state,
            config,
            should_close,
        }
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Check if we should close
        if self.should_close.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
            return;
        }

        let now = Instant::now();

        // Lock state and update
        {
            let mut state = self.state.lock().unwrap();

            // Update sprite and bubble animations
            state.sprite.update(now);
            state.bubble.update(now);
            state.last_update = now;
        }

        // Request continuous repaint for smooth animation (60fps target)
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(1.0 / 60.0));

        // Draw on a transparent layer (no panels, just raw painter)
        let painter = ctx.layer_painter(eframe::egui::LayerId::new(
            eframe::egui::Order::Background,
            eframe::egui::Id::new("overlay"),
        ));

        // Calculate sprite position (center of window, slightly below center)
        let rect = ctx.screen_rect();
        let sprite_center = Pos2::new(
            rect.right() - self.config.edge_gap - self.config.size.0 / 2.0,
            rect.bottom() - self.config.edge_gap - self.config.size.1 / 2.0 + 20.0,
        );

        // Draw sprite and bubble
        let state = self.state.lock().unwrap();
        state
            .sprite
            .draw(&painter, sprite_center);
        state
            .bubble
            .draw(&painter, sprite_center, self.config.sprite_size);
    }

    /// Clear color (transparent background).
    fn clear_color(&self, _visuals: &eframe::egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // Fully transparent
    }
}

/// Run the overlay window on the main thread.
/// This function blocks until the window is closed.
pub fn run_overlay(
    state: Arc<Mutex<OverlayState>>,
    should_close: Arc<AtomicBool>,
) -> eframe::Result<()> {
    let config = OverlayConfig::default();

    // Calculate window position (bottom-right corner)
    // We'll use a fixed position for now; in future could query screen size
    let window_width = config.size.0;
    let window_height = config.size.1;
    let edge_gap = config.edge_gap;

    // Assume a typical screen size (1920x1080) for now
    // The window will be positioned relative to screen coordinates
    let screen_width = 1920.0;
    let screen_height = 1080.0;
    let pos_x = screen_width - window_width - edge_gap;
    let pos_y = screen_height - window_height - edge_gap;

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([window_width, window_height])
            .with_position(Pos2::new(pos_x, pos_y))
            .with_transparent(true)
            .with_decorations(false) // No title bar, borders
            .with_always_on_top()
            .with_taskbar(false) // No taskbar icon
            .with_resizable(false)
            .with_title("Wakey"), // Hidden title for identification
        ..Default::default()
    };

    let app = OverlayApp::new(state, config, should_close);

    eframe::run_native("Wakey", options, Box::new(|_cc| Ok(Box::new(app))))
}

/// Spawn a background task to handle spine events and update overlay state.
/// This runs in a tokio async context, separate from the GUI thread.
pub async fn run_spine_handler(
    spine: Spine,
    state: Arc<Mutex<OverlayState>>,
    should_close: Arc<AtomicBool>,
) {
    let mut receiver = spine.subscribe();

    tracing::info!("Overlay spine handler started");

    loop {
        tokio::select! {
            // Receive events from spine
            Ok(event) = receiver.recv() => {
                handle_spine_event(event, &state, &should_close, Instant::now());
            }

            // Periodic update (for animations that need time even without events)
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                // Animations are handled in the GUI thread's update loop
                // This is just for event-driven state sync
            }
        }

        // Check for shutdown
        if should_close.load(Ordering::Relaxed) {
            tracing::info!("Overlay spine handler shutting down");
            break;
        }
    }
}

/// Handle a single spine event, updating overlay state.
fn handle_spine_event(
    event: WakeyEvent,
    state: &Arc<Mutex<OverlayState>>,
    should_close: &Arc<AtomicBool>,
    _now: Instant,
) {
    let mut overlay_state = state.lock().unwrap();

    match event {
        // Voice-only mode: ShouldSpeak triggers TTS, no bubble
        // TTS listener emits VoiceWakeySpeaking for sprite animation
        WakeyEvent::ShouldSpeak {
            suggested_text: Some(text),
            reason,
            urgency,
        } => {
            tracing::info!(
                text = %text,
                reason = %reason,
                urgency = ?urgency,
                "ShouldSpeak event received (voice-only mode - no bubble)"
            );
            // No bubble - TTS listener will speak this
            // VoiceWakeySpeaking event will be emitted by TTS listener for animation
        }

        WakeyEvent::ShouldSpeak { reason, .. } => {
            tracing::warn!(reason = %reason, "ShouldSpeak event received but no text provided");
        }

        WakeyEvent::MoodChanged { to, .. } => {
            tracing::debug!(mood = ?to, "Mood changed");
            overlay_state.mood = to;
            overlay_state
                .sprite
                .set_expression(Expression::from_mood(to));
        }

        // ── Voice events ──
        WakeyEvent::VoiceListeningStarted => {
            tracing::debug!("Voice listening started");
            overlay_state.voice_state = VoiceState::Listening;
            if let Some(trigger) = crate::trigger::event_to_trigger(&event) {
                if let Some(expr) = overlay_state.mapper.map(&trigger) {
                    overlay_state.sprite.set_expression(expr);
                }
            }
        }

        WakeyEvent::VoiceListeningStopped => {
            tracing::debug!("Voice listening stopped");
            if overlay_state.voice_state == VoiceState::Listening {
                overlay_state.voice_state = VoiceState::Idle;
                let mood = overlay_state.mood;
                overlay_state
                    .sprite
                    .set_expression(Expression::from_mood(mood));
            }
        }

        WakeyEvent::VoiceUserSpeaking { text, is_final } => {
            tracing::debug!(text = %text, is_final, "User speaking");
            // No bubble in voice-only mode
        }

        WakeyEvent::VoiceWakeyThinking => {
            tracing::debug!("Wakey thinking");
            overlay_state.voice_state = VoiceState::Thinking;
            if let Some(trigger) = crate::trigger::event_to_trigger(&event) {
                if let Some(expr) = overlay_state.mapper.map(&trigger) {
                    overlay_state.sprite.set_expression(expr);
                }
            }
        }

        WakeyEvent::VoiceWakeySpeaking { ref text } => {
            tracing::debug!(text = %text, "Wakey speaking (voice-only mode)");
            overlay_state.voice_state = VoiceState::Speaking;
            if let Some(trigger) = crate::trigger::event_to_trigger(&event) {
                if let Some(expr) = overlay_state.mapper.map(&trigger) {
                    overlay_state.sprite.set_expression(expr);
                }
            }
        }

        WakeyEvent::VoiceSessionEnded => {
            tracing::debug!("Voice session ended");
            overlay_state.voice_state = VoiceState::Idle;
            let mood = overlay_state.mood;
            overlay_state
                .sprite
                .set_expression(Expression::from_mood(mood));
        }

        WakeyEvent::VoiceError { message } => {
            tracing::error!(message, "Voice error");
            overlay_state.voice_state = VoiceState::Idle;
            overlay_state
                .sprite
                .set_expression(Expression::from_mood(Mood::Concerned));
            // No bubble in voice-only mode, but could optionally show error briefly
        }

        WakeyEvent::Shutdown => {
            tracing::info!("Shutdown event received");
            should_close.store(true, Ordering::Relaxed);
        }

        WakeyEvent::Tick | WakeyEvent::Breath => {
            // Heartbeat events - could influence animation speed
            // For MVP, we just let the sprite's internal timer drive breathing
        }

        _ => {
            // Other events not directly relevant to overlay visuals
        }
    }
}
