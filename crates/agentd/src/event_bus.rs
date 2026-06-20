use chrono::{DateTime, Utc};
#[cfg(test)]
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// One event on the bus. Server pushes these to every subscriber.
#[derive(Debug, Clone)]
pub struct Event {
    /// Method name without the `event.` prefix (e.g. `session.status_changed`).
    pub kind: String,
    /// The session this event is about, if any.
    pub session_id: Option<Uuid>,
    /// Event-specific payload.
    pub payload: Value,
    /// When the daemon received/processed the event.
    pub ts: DateTime<Utc>,
}

/// Process-wide event bus. Built on `tokio::sync::broadcast`.
///
/// Capacity is the number of events buffered per subscriber before
/// `RecvError::Lagged` is returned. Set this high enough that a slow
/// subscriber can't fall behind, but low enough that memory doesn't
/// grow unbounded. 1024 is the v1 default.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
    _capacity: usize,
}

impl EventBus {
    /// Create a new bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            sender,
            _capacity: capacity,
        }
    }

    /// Subscribe to all future events. Each subscriber gets its own receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Publish an event. Returns the number of subscribers that received it.
    pub fn emit(&self, event: Event) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    /// Number of active subscribers. Used by metrics.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Shared event bus. Clone the Arc and pass to handlers.
pub type SharedEventBus = Arc<EventBus>;

/// Helper for tests that want to read the latest emitted event without
/// dealing with the broadcast Receiver API directly.
#[cfg(test)]
pub struct TestSink {
    inner: Mutex<Vec<Event>>,
}

#[cfg(test)]
impl TestSink {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }
    pub fn push(&self, e: Event) {
        self.inner.lock().push(e);
    }
    pub fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *self.inner.lock())
    }
}

#[cfg(test)]
impl Default for TestSink {
    fn default() -> Self {
        Self::new()
    }
}
