//! Wakey Overlay — The face of Wakey.
//!
//! Always-on-top transparent window with animated breathing sprite
//! and chat bubble. Uses eframe (egui framework) for rendering.
//!
//! # Architecture
//!
//! The overlay runs in two threads:
//! - Main thread: eframe GUI event loop (must be on main for X11/GL)
//! - Background task: Spine event handler (tokio async)
//!
//! State is shared via `Arc<Mutex<OverlayState>>`.
//!
//! # Usage
//!
//! ```rust,no_run
//! use wakey_overlay::run_overlay_with_spine;
//! use wakey_spine::Spine;
//!
//! let spine = Spine::new();
//!
//! // This blocks on the GUI thread; run in a dedicated thread if needed
//! run_overlay_with_spine(spine);
//! ```

pub mod bubble;
pub mod expressions;
pub mod sprite;
pub mod window;

pub use bubble::Bubble;
pub use expressions::Expression;
pub use sprite::Sprite;
pub use window::{
    OverlayApp, OverlayConfig, OverlayState, VoiceState, run_overlay, run_spine_handler,
};

use std::sync::{Arc, Mutex, atomic::AtomicBool};
use wakey_spine::Spine;

/// Run the overlay window on the main thread with spine integration.
///
/// This function MUST be called from the main thread on Linux/X11.
/// It spawns the spine event handler in a background tokio runtime
/// and runs the eframe GUI on the calling (main) thread.
///
/// This function blocks until the window is closed.
pub fn run_overlay_with_spine(spine: Spine) {
    let state = Arc::new(Mutex::new(OverlayState::default()));
    let should_close = Arc::new(AtomicBool::new(false));

    // Spawn spine event handler in a separate tokio runtime on a background thread
    let state_clone = state.clone();
    let should_close_clone = should_close.clone();
    let spine_clone = spine.clone();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for overlay");

        rt.block_on(async move {
            run_spine_handler(spine_clone, state_clone, should_close_clone).await;
        });
    });

    // Run GUI on main thread (blocks until window closes)
    if let Err(e) = run_overlay(state, should_close) {
        tracing::error!(error = %e, "Overlay window failed");
    }
}
