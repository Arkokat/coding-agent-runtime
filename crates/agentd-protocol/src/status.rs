use serde::{Deserialize, Serialize};
use std::fmt;

/// Normalized session status, written only by the owning plugin.
///
/// String-serialized as lowercase, with `waiting_for_user` using underscores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Row exists, agent process spawning.
    Starting,
    /// Agent alive, no current activity.
    Idle,
    /// Agent actively doing work.
    Working,
    /// Agent emitted a question or hit an approval prompt.
    WaitingForUser,
    /// Agent crashed or emitted an error. Sticky until dismissed.
    Errored,
    /// Agent exited cleanly. Terminal, immutable.
    Finished,
}

impl SessionStatus {
    /// Return all status variants in declaration order.
    pub const ALL: &'static [SessionStatus] = &[
        SessionStatus::Starting,
        SessionStatus::Idle,
        SessionStatus::Working,
        SessionStatus::WaitingForUser,
        SessionStatus::Errored,
        SessionStatus::Finished,
    ];

    /// True if this status is terminal (no further transitions expected).
    pub const fn is_terminal(self) -> bool {
        matches!(self, SessionStatus::Finished)
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            SessionStatus::Starting => "starting",
            SessionStatus::Idle => "idle",
            SessionStatus::Working => "working",
            SessionStatus::WaitingForUser => "waiting_for_user",
            SessionStatus::Errored => "errored",
            SessionStatus::Finished => "finished",
        };
        f.write_str(s)
    }
}
