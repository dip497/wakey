use tokio::sync::broadcast;
use wakey_types::WakeyEvent;

const CHANNEL_CAPACITY: usize = 1024;

/// The event spine — central nervous system of Wakey.
/// All crate-to-crate communication flows through here.
#[derive(Debug, Clone)]
pub struct Spine {
    sender: broadcast::Sender<WakeyEvent>,
}

impl Spine {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Emit an event into the spine.
    pub fn emit(&self, event: WakeyEvent) {
        // Ignore send errors (no receivers is fine during startup/shutdown)
        let _ = self.sender.send(event);
    }

    /// Subscribe to all events flowing through the spine.
    pub fn subscribe(&self) -> broadcast::Receiver<WakeyEvent> {
        self.sender.subscribe()
    }

    /// Get the current number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for Spine {
    fn default() -> Self {
        Self::new()
    }
}
