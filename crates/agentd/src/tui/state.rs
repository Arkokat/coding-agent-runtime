//! TUI state: snapshot-derived, mutated by events + input, read by render.

use crate::event_bus::Event;
use agentd_protocol::{Plugin, Session, SessionStatus};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// How long a row remains "flashed" (highlighted) after a change.
pub const FLASH_DURATION: Duration = Duration::from_millis(500);

/// How long a transient status message stays on screen.
pub const STATUS_MESSAGE_DURATION: Duration = Duration::from_secs(3);

/// Per-status session counts, recomputed whenever the session set changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusCounters {
    /// Sessions whose status is `Working`.
    pub working: u32,
    /// Sessions whose status is `WaitingForUser`.
    pub waiting: u32,
    /// Sessions whose status is `Errored`.
    pub errored: u32,
    /// Sessions whose status is `Idle`.
    pub idle: u32,
    /// Sessions whose status is `Starting`.
    pub starting: u32,
    /// Sessions whose status is `Finished`.
    pub finished: u32,
}

impl StatusCounters {
    /// Recompute counters from a session list. Single pass.
    pub fn from_sessions(sessions: &[Session]) -> Self {
        let mut c = Self::default();
        for s in sessions {
            match s.status {
                SessionStatus::Working => c.working += 1,
                SessionStatus::WaitingForUser => c.waiting += 1,
                SessionStatus::Errored => c.errored += 1,
                SessionStatus::Idle => c.idle += 1,
                SessionStatus::Starting => c.starting += 1,
                SessionStatus::Finished => c.finished += 1,
            }
        }
        c
    }
}

/// Rename modal state — session id and the in-progress input string.
#[derive(Debug, Clone)]
pub struct RenameModal {
    /// Session whose display name is being edited.
    pub session_id: Uuid,
    /// Current input buffer.
    pub input: String,
}

/// New-session modal state — search query and recent working dirs to filter.
#[derive(Debug, Clone)]
pub struct NewModal {
    /// User-typed search string.
    pub query: String,
    /// Candidate recent working dirs and their last-used timestamps.
    pub recents: Vec<(PathBuf, DateTime<Utc>)>,
}

/// All state the TUI render layer reads.
///
/// Mutations come from three places: initial snapshot (`from_snapshot`),
/// bus events (`apply_event`), and keypress handlers (direct field sets,
/// task 5+). `dirty` is set on any change; the render loop checks it to
/// avoid unnecessary draws.
#[derive(Debug, Clone)]
pub struct TuiState {
    /// All currently known sessions, in insertion order.
    pub sessions: Vec<Session>,
    /// All currently known plugins, in insertion order.
    pub plugins: Vec<Plugin>,
    /// Index into `sessions` of the highlighted row.
    pub selected: usize,
    /// Recomputed on any session set change.
    pub counters: StatusCounters,
    /// Session id -> expiry instant for row highlight.
    pub flash_until: HashMap<Uuid, Instant>,
    /// Render layer should redraw.
    pub dirty: bool,
    /// `?` help overlay visible.
    pub show_help: bool,
    /// Active rename modal, if any.
    pub rename_modal: Option<RenameModal>,
    /// Active new-session modal, if any.
    pub new_modal: Option<NewModal>,
    /// Transient status line message and its expiry.
    pub status_message: Option<(String, Instant)>,
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new()
    }
}

impl TuiState {
    /// Construct an empty state. `dirty` starts as `true` so the first
    /// frame renders even with no data.
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            plugins: Vec::new(),
            selected: 0,
            counters: StatusCounters::default(),
            flash_until: HashMap::new(),
            dirty: true,
            show_help: false,
            rename_modal: None,
            new_modal: None,
            status_message: None,
        }
    }

    /// Initialize from a `state.snapshot` response value.
    pub fn from_snapshot(snap: &Value) -> Self {
        let mut s = Self::new();
        if let Some(arr) = snap.get("sessions").and_then(Value::as_array) {
            for v in arr {
                if let Ok(session) = serde_json::from_value::<Session>(v.clone()) {
                    s.sessions.push(session);
                }
            }
        }
        if let Some(arr) = snap.get("plugins").and_then(Value::as_array) {
            for v in arr {
                if let Ok(plugin) = serde_json::from_value::<Plugin>(v.clone()) {
                    s.plugins.push(plugin);
                }
            }
        }
        s.counters = StatusCounters::from_sessions(&s.sessions);
        s.dirty = true;
        s
    }

    /// Currently selected session, if any.
    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.selected)
    }

    /// Apply an event from the daemon. Mutates state and sets `dirty`.
    pub fn apply_event(&mut self, event: &Event) {
        let now = Instant::now();
        match event.kind.as_str() {
            "session.created" | "session.discovered" => {
                if let Ok(s) = serde_json::from_value::<Session>(event.payload.clone()) {
                    if let Some(existing) = self.sessions.iter_mut().find(|x| x.id == s.id) {
                        *existing = s;
                    } else {
                        self.flash_until.insert(s.id, now + FLASH_DURATION);
                        self.sessions.push(s);
                    }
                    self.recompute_counters();
                }
            }
            "session.renamed" => {
                if let Some(id) = event.session_id {
                    if let Some(s) = self.sessions.iter_mut().find(|x| x.id == id) {
                        if let Some(name) =
                            event.payload.get("display_name").and_then(Value::as_str)
                        {
                            s.display_name = name.to_string();
                            self.flash_until.insert(id, now + FLASH_DURATION);
                        }
                    }
                }
            }
            "session.killed" | "session.finished" => {
                if let Some(id) = event.session_id {
                    self.sessions.retain(|s| s.id != id);
                    self.flash_until.remove(&id);
                    if self.selected >= self.sessions.len() && !self.sessions.is_empty() {
                        self.selected = self.sessions.len() - 1;
                    }
                    self.recompute_counters();
                }
            }
            "session.status_changed" => {
                if let Some(id) = event.session_id {
                    if let Some(s) = self.sessions.iter_mut().find(|x| x.id == id) {
                        if let Some(status) = event.payload.get("status").and_then(Value::as_str) {
                            if let Ok(new_status) = serde_json::from_value::<SessionStatus>(
                                serde_json::Value::String(status.to_string()),
                            ) {
                                s.status = new_status;
                                self.flash_until.insert(id, now + FLASH_DURATION);
                                self.recompute_counters();
                            }
                        }
                    }
                }
            }
            "session.task_changed" => {
                if let Some(id) = event.session_id {
                    if let Some(s) = self.sessions.iter_mut().find(|x| x.id == id) {
                        if let Some(task) = event.payload.get("task").and_then(Value::as_str) {
                            s.current_task = Some(task.to_string());
                            self.flash_until.insert(id, now + FLASH_DURATION);
                        }
                    }
                }
            }
            "session.usage_updated" => {
                if let Some(id) = event.session_id {
                    if let Some(s) = self.sessions.iter_mut().find(|x| x.id == id) {
                        if let Some(used) = event.payload.get("used").and_then(Value::as_u64) {
                            s.context_used_tokens = Some(used);
                        }
                        if let Some(total) = event.payload.get("total").and_then(Value::as_u64) {
                            s.context_total_tokens = Some(total);
                        }
                        if let Some(cost) = event.payload.get("cost_usd").and_then(Value::as_f64) {
                            s.cost_usd = Some(cost);
                        }
                    }
                }
            }
            "plugin.connected" => {
                if let Ok(p) = serde_json::from_value::<Plugin>(event.payload.clone()) {
                    if let Some(existing) = self.plugins.iter_mut().find(|x| x.name == p.name) {
                        *existing = p;
                    } else {
                        self.plugins.push(p);
                    }
                }
            }
            "plugin.disconnected" => {
                if let Some(name) = event.payload.get("name").and_then(Value::as_str) {
                    self.plugins.retain(|p| p.name != name);
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    /// Clear expired flash entries. Sets `dirty` if anything changed.
    pub fn tick_flash(&mut self, now: Instant) {
        let before = self.flash_until.len();
        self.flash_until.retain(|_, t| *t > now);
        if self.flash_until.len() != before {
            self.dirty = true;
        }
    }

    fn recompute_counters(&mut self) {
        self.counters = StatusCounters::from_sessions(&self.sessions);
    }
}
