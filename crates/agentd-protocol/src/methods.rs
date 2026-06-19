//! All JSON-RPC method names. Use the `Method::*` constants, never raw strings,
//! to prevent typos.

/// JSON-RPC 2.0 method name constants. Associated `const`s on a unit struct,
/// so callers write `Method::SESSION_CREATE` and the compiler enforces spelling.
pub struct Method;

// Control UDS methods (client -> daemon)
impl Method {
    /// Get all sessions and plugins snapshot.
    pub const STATE_SNAPSHOT: &'static str = "state.snapshot";
    /// Get a single session by id.
    pub const SESSION_GET: &'static str = "session.get";
    /// Get event log for a session.
    pub const SESSION_EVENTS: &'static str = "session.events";
    /// Get daemon status.
    pub const DAEMON_STATUS: &'static str = "daemon.status";
    /// List all configured plugins.
    pub const PLUGIN_LIST: &'static str = "plugin.list";
    /// Get current metrics.
    pub const METRICS: &'static str = "metrics";

    /// Create a new session.
    pub const SESSION_CREATE: &'static str = "session.create";
    /// Rename a session.
    pub const SESSION_RENAME: &'static str = "session.rename";
    /// Jump to a session.
    pub const SESSION_JUMP: &'static str = "session.jump";
    /// Kill a session.
    pub const SESSION_KILL: &'static str = "session.kill";
    /// Clear errored status.
    pub const SESSION_DISMISS_ERROR: &'static str = "session.dismiss_error";
    /// Start a configured plugin.
    pub const PLUGIN_START: &'static str = "plugin.start";
    /// Stop a running plugin.
    pub const PLUGIN_STOP: &'static str = "plugin.stop";
    /// Install a plugin.
    pub const PLUGIN_INSTALL: &'static str = "plugin.install";
    /// Update installed plugins.
    pub const PLUGIN_UPDATE: &'static str = "plugin.update";
    /// Remove an installed plugin.
    pub const PLUGIN_REMOVE: &'static str = "plugin.remove";
    /// Gracefully shut down the daemon.
    pub const DAEMON_SHUTDOWN: &'static str = "daemon.shutdown";

    /// Subscribe to event notifications.
    pub const SUBSCRIBE: &'static str = "subscribe";
    /// Stop receiving event notifications.
    pub const UNSUBSCRIBE: &'static str = "unsubscribe";
}

// Plugin UDS methods (plugin -> daemon)
impl Method {
    /// Plugin announces itself on connect.
    pub const PLUGIN_HELLO: &'static str = "plugin.hello";
    /// Plugin pushes an event for a session.
    pub const SESSION_REPORT_EVENT: &'static str = "session.report_event";
    /// Plugin reports a newly discovered session.
    pub const SESSION_DISCOVER: &'static str = "session.discover";
    /// Plugin liveness ping.
    pub const PLUGIN_HEARTBEAT: &'static str = "plugin.heartbeat";
    /// Plugin graceful disconnect.
    pub const PLUGIN_BYE: &'static str = "plugin.bye";

    /// Event notification (server -> subscriber, no response).
    pub const EVENT: &'static str = "event";
}
