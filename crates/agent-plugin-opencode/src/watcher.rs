//! Poll opencode tmux panes for status, emit events to the daemon.
//!
//! The watcher runs alongside [`crate::discovery`]: after
//! [`crate::discovery::discover_opencode_panes`] returns the set of
//! panes running `opencode`, [`run`] registers each one with the
//! daemon and then continuously polls pane content for status
//! changes, emitting a `session.status_changed` event on each
//! transition.

use std::path::Path;
use std::time::Duration;

use agentd_plugin_sdk::AgentdClient;
use agentd_plugin_sdk::uuid::Uuid;
use serde_json::Value;
use tokio::process::Command;

use crate::discovery::OpencodePane;

/// Normalized status derived from the last non-empty line of a captured
/// tmux pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Session has been registered with the daemon but no terminal
    /// status has been observed yet.
    Starting,
    /// Opencode is actively working (compiling, building, running, etc.).
    Working,
    /// Opencode has returned to its prompt, awaiting input.
    Idle,
    /// The last line indicated an error, panic, or failure.
    Errored,
}

impl Status {
    /// Wire name for this status, matching the daemon's enum strings.
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Working => "working",
            Self::Idle => "idle",
            Self::Errored => "errored",
        }
    }
}

/// Parse the last non-empty line of captured pane content into a
/// status.
///
/// Returns `None` when the line does not match any of the known
/// opencode status keywords; callers should keep the previous status
/// in that case.
pub fn parse_status(line: &str) -> Option<Status> {
    let lower = line.to_lowercase();
    if lower.contains("compiling")
        || lower.contains("building")
        || lower.contains("running")
        || lower.contains("loading")
        || lower.contains("processing")
    {
        Some(Status::Working)
    } else if lower.contains("error") || lower.contains("panic") || lower.contains("failed") {
        Some(Status::Errored)
    } else {
        let trimmed = line.trim_end();
        if trimmed.ends_with('\u{276f}') || trimmed.ends_with('>') {
            Some(Status::Idle)
        } else {
            None
        }
    }
}

/// Capture the last `lines` lines of pane content via
/// `tmux capture-pane -p -t <pane> -S -<lines>`.
///
/// Returns the captured text on success. Returns an empty string when
/// `tmux` is missing, fails to spawn, or exits non-zero so the watch
/// loop can keep running on transient tmux hiccups.
pub async fn capture_pane(pane: &str, lines: u16, tmux: &Path) -> String {
    let start_arg = format!("-{lines}");
    let output = Command::new(tmux)
        .args(["capture-pane", "-p", "-t", pane, "-S", &start_arg])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => String::new(),
    }
}

/// Per-pane bookkeeping: the daemon-assigned session id, the last
/// status we observed (so we only emit on transitions), and the
/// `tmux_session:pane_id` key (kept here for nicer tracing later).
type PaneState = (Uuid, Status);

/// Watch loop. Calls `session.discover` for each new pane, then every
/// `interval` captures the pane content and emits
/// `session.status_changed` on change. Sends `plugin.heartbeat` every
/// 5 seconds.
///
/// Returns `Ok(())` only when the loop is cancelled by aborting the
/// task; it is intended to run for the lifetime of the plugin
/// process. The `tmux` path is taken explicitly so tests can inject a
/// fake `tmux` script.
pub async fn run(
    client: &mut AgentdClient,
    panes: Vec<OpencodePane>,
    interval: Duration,
    tmux: &Path,
) -> anyhow::Result<()> {
    let mut known: std::collections::HashMap<String, PaneState> = std::collections::HashMap::new();
    for pane in &panes {
        let key = pane_key(pane);
        let working_dir = pane.working_dir.to_string_lossy().to_string();
        match client
            .discover(&pane.tmux_session, &pane.pane_id, &working_dir)
            .await
        {
            Ok(value) => {
                if let Some(id_str) = value.get("session_id").and_then(Value::as_str) {
                    if let Ok(id) = id_str.parse::<Uuid>() {
                        known.insert(key, (id, Status::Starting));
                    } else {
                        tracing::warn!(pane = %key, "session.discover returned non-UUID session_id");
                    }
                } else {
                    tracing::warn!(pane = %key, "session.discover returned no session_id");
                }
            }
            Err(e) => tracing::warn!(error = %e, pane = %key, "session.discover failed"),
        }
    }

    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            () = tokio::time::sleep(interval) => {
                for pane in &panes {
                    let key = pane_key(pane);
                    let Some((session_id, prev_status)) = known.get(&key).copied() else {
                        continue;
                    };
                    let captured = capture_pane(&pane.pane_id, 50, tmux).await;
                    let last_line = captured
                        .lines()
                        .rfind(|l| !l.trim().is_empty())
                        .unwrap_or("");
                    let Some(new_status) = parse_status(last_line) else {
                        continue;
                    };
                    if new_status == prev_status {
                        continue;
                    }
                    let payload = serde_json::json!({ "status": new_status.as_str() });
                    if let Err(e) = client
                        .report_event(session_id, "session.status_changed", payload)
                        .await
                    {
                        tracing::warn!(error = %e, pane = %key, "report_event failed");
                    }
                    if let Some(slot) = known.get_mut(&key) {
                        slot.1 = new_status;
                    }
                }
            }
            _ = heartbeat_interval.tick() => {
                if let Err(e) = client.heartbeat().await {
                    tracing::warn!(error = %e, "heartbeat failed");
                }
            }
        }
    }
}

fn pane_key(pane: &OpencodePane) -> String {
    format!("{}:{}", pane.tmux_session, pane.pane_id)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn status_as_str_matches_protocol_strings() {
        assert_eq!(Status::Starting.as_str(), "starting");
        assert_eq!(Status::Working.as_str(), "working");
        assert_eq!(Status::Idle.as_str(), "idle");
        assert_eq!(Status::Errored.as_str(), "errored");
    }
}
