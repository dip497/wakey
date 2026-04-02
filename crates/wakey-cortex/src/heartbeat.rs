//! Heartbeat runner — multi-rhythm consciousness engine.
//!
//! Based on OpenClaw heartbeat pattern (configurable intervals, lightContext)
//! and ZeroClaw cron triggers (window-based check, at-most-once per tick).
//!
//! Rhythms:
//! - Tick (2s): Local only, no LLM. Active window + vitals.
//! - Breath (30s): May call VLM. Screen understanding.
//! - Reflect (15min): LLM call. Summarize, compact memory.
//! - Dream (daily): Heavy. Pattern learning, memory compression.

use chrono::Utc;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};
use wakey_spine::Spine;
use wakey_types::WakeyEvent;
use wakey_types::config::HeartbeatConfig;

/// The heartbeat runner — emits rhythm events on schedule.
pub struct HeartbeatRunner {
    spine: Spine,
    tick_interval: Duration,
    breath_interval: Duration,
    reflect_interval: Duration,
    #[allow(dead_code)]
    dream_hour: u8,
}

impl HeartbeatRunner {
    /// Create a new heartbeat runner from config.
    pub fn new(spine: Spine, config: &HeartbeatConfig) -> Self {
        Self {
            spine,
            tick_interval: Duration::from_millis(config.tick_interval_ms),
            breath_interval: Duration::from_millis(config.breath_interval_ms),
            reflect_interval: Duration::from_millis(config.reflect_interval_ms),
            dream_hour: config.dream_hour,
        }
    }

    /// Run the heartbeat loop until shutdown.
    ///
    /// Emits Tick events at configured interval, gathering window and system info.
    /// No LLM calls in tick loop — stays under 10ms per tick.
    pub async fn run(self, mut shutdown: broadcast::Receiver<WakeyEvent>) {
        info!(
            tick_ms = self.tick_interval.as_millis(),
            "Heartbeat starting"
        );

        let mut tick_timer = tokio::time::interval(self.tick_interval);
        tick_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut breath_timer = tokio::time::interval(self.breath_interval);
        breath_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        let mut reflect_timer = tokio::time::interval(self.reflect_interval);
        reflect_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                // Tick rhythm (2s) — local window + vitals
                _ = tick_timer.tick() => {
                    self.emit_tick();
                }

                // Breath rhythm (30s) — potential VLM call
                _ = breath_timer.tick() => {
                    self.emit_breath();
                }

                // Reflect rhythm (15min) — memory compaction
                _ = reflect_timer.tick() => {
                    self.emit_reflect();
                }

                // Shutdown signal
                Ok(WakeyEvent::Shutdown) = shutdown.recv() => {
                    info!("Heartbeat shutting down");
                    break;
                }
            }
        }
    }

    /// Emit a Tick event with current window focus and system vitals.
    ///
    /// Must complete in <10ms. No LLM calls.
    fn emit_tick(&self) {
        let start = std::time::Instant::now();

        // Gather window info (from wakey-senses)
        let window_info = wakey_senses::window::get_active_window();
        if let Some((app, title)) = window_info {
            self.spine.emit(WakeyEvent::WindowFocusChanged {
                app,
                title,
                timestamp: Utc::now(),
            });
        }

        // Gather system vitals (from wakey-senses)
        let vitals = wakey_senses::system::get_system_vitals();
        self.spine.emit(WakeyEvent::SystemVitals {
            battery_percent: vitals.battery_percent,
            cpu_usage: vitals.cpu_usage,
            ram_usage_mb: vitals.ram_usage_mb,
            timestamp: Utc::now(),
        });

        // Emit Tick marker
        self.spine.emit(WakeyEvent::Tick);

        let elapsed = start.elapsed();
        if elapsed.as_millis() > 10 {
            warn!(
                elapsed_ms = elapsed.as_millis(),
                "Tick exceeded 10ms budget"
            );
        } else {
            debug!(elapsed_ms = elapsed.as_millis(), "Tick completed");
        }
    }

    /// Emit a Breath event.
    ///
    /// Cortex may call VLM on this rhythm for screen understanding.
    fn emit_breath(&self) {
        debug!("Breath rhythm triggered");
        self.spine.emit(WakeyEvent::Breath);
    }

    /// Emit a Reflect event.
    ///
    /// Cortex compacts memory and summarizes on this rhythm.
    fn emit_reflect(&self) {
        debug!("Reflect rhythm triggered");
        self.spine.emit(WakeyEvent::Reflect);
    }
}
