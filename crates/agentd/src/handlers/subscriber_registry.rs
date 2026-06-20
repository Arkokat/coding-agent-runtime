//! Tracks TUI (and other) subscribers to live event notifications.
//! Each subscriber gets a tokio mpsc channel; the daemon's event bus
//! forwards into the registry, which fans out to all subscribers.

use crate::event_bus::Event;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;

/// Unique identifier for a registered subscriber. Returned by
/// [`SubscriberRegistry::register`] and consumed by
/// [`SubscriberRegistry::unregister`].
pub type SubscriptionId = usize;

/// Fan-out hub for live event notifications. The daemon creates one
/// of these at startup, the bus-forwarding task feeds it every event
/// the bus emits, and the control-UDS `subscribe` handler hands out
/// a per-subscriber `mpsc` receiver for the lifetime of the
/// connection.
pub struct SubscriberRegistry {
    senders: Mutex<Vec<(SubscriptionId, mpsc::UnboundedSender<Event>)>>,
    next_id: AtomicUsize,
}

impl SubscriberRegistry {
    /// Build a new, empty registry.
    pub fn new() -> Self {
        Self {
            senders: Mutex::new(Vec::new()),
            next_id: AtomicUsize::new(1),
        }
    }

    /// Register a new subscriber. Returns a unique id (use for
    /// `unregister`) and the receiver end of the channel.
    pub fn register(&self) -> (SubscriptionId, mpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.senders.lock().push((id, tx));
        (id, rx)
    }

    /// Remove a subscriber. Idempotent — calling with an unknown id
    /// is a no-op.
    pub fn unregister(&self, id: SubscriptionId) {
        self.senders.lock().retain(|(i, _)| *i != id);
    }

    /// Send `event` to every subscriber. Send-failures (closed
    /// channel) are treated as a dead subscriber and removed.
    pub fn broadcast(&self, event: &Event) {
        let mut senders = self.senders.lock();
        senders.retain(|(_, tx)| tx.send(event.clone()).is_ok());
    }

    /// Number of active subscribers.
    pub fn len(&self) -> usize {
        self.senders.lock().len()
    }

    /// Whether the registry has any active subscribers.
    pub fn is_empty(&self) -> bool {
        self.senders.lock().is_empty()
    }
}

impl Default for SubscriberRegistry {
    fn default() -> Self {
        Self::new()
    }
}
