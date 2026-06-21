//! Poll opencode tmux panes for status, emit events to the daemon.
//!
//! The watcher runs alongside [`crate::discovery`]: after
//! [`crate::discovery::discover_opencode_panes`] returns the set of
//! panes running `opencode`, [`run`] registers each one with the
//! daemon and then continuously polls pane content for status
//! changes, emitting a `session.status_changed` event on each
//! transition.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use agentd_plugin_sdk::AgentdClient;
use agentd_plugin_sdk::uuid::Uuid;
use serde_json::Value;
use tokio::process::Command;

use crate::discovery::{OpencodePane, enumerate_panes, is_opencode_comm, pane_key_from};

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

/// Per-pane bookkeeping: the daemon-assigned session id and the last
/// status we observed (so we only emit on transitions).
type PaneState = (Uuid, Status);

/// Return the keys present in `prev` but not in `curr` — i.e., the
/// panes that have disappeared between two discovery sweeps.
///
/// Extracted as a free function so the diff logic can be unit-tested
/// without spinning up the full watch loop.
#[allow(clippy::implicit_hasher)] // HashSet<String> is fine for this internal helper
pub fn diff_panes(prev: &HashSet<String>, curr: &HashSet<String>) -> Vec<String> {
    prev.difference(curr).cloned().collect()
}

/// Diff `known` (`pane_key` -> `V`) against `current` (`pane_key` ->
/// `pane_current_command`), returning `(vanished, new)` `pane_key`s.
///
/// Generic over `V` because `known` carries per-pane bookkeeping
/// (`PaneState` in production, but tests can use any `Uuid` or
/// `()`). The function only consults `known.keys()`; values are
/// ignored.
///
/// A pane is **vanished** if it was in `known` and either:
/// - is not in `current` at all (the pane was killed), OR
/// - is in `current` but `pane_current_command` is not `opencode`
///   (opencode exited cleanly and the shell prompt returned; the
///   pane persists).
///
/// A pane is **new** if it appears in `current` with command
/// `opencode` and is not in `known`.
///
/// Extracted as a free function so the diff logic can be unit-tested
/// without spinning up the full watch loop.
#[allow(clippy::implicit_hasher)] // HashMap<String, V> is fine for this internal helper
pub fn diff_pane_states<V>(
    known: &HashMap<String, V>,
    current: &HashMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let current_opencode: HashSet<String> = current
        .iter()
        .filter(|(_, cmd)| is_opencode_comm(cmd))
        .map(|(k, _)| k.clone())
        .collect();

    let vanished: Vec<String> = known
        .keys()
        .filter(|k| !current_opencode.contains(*k))
        .cloned()
        .collect();

    let new: Vec<String> = current_opencode
        .iter()
        .filter(|k| !known.contains_key(*k))
        .cloned()
        .collect();

    (vanished, new)
}

fn pane_key(pane: &OpencodePane) -> String {
    pane_key_from(&pane.tmux_session, &pane.pane_id)
}

/// Call `session.discover` for `pane` and parse the returned session
/// id. Returns `None` if the call failed or the response did not
/// include a UUID session id; warnings are logged in that case.
async fn discover_pane(client: &mut AgentdClient, pane: &OpencodePane) -> Option<PaneState> {
    let key = pane_key(pane);
    let working_dir = pane.working_dir.to_string_lossy().to_string();
    match client
        .discover(&pane.tmux_session, &pane.pane_id, &working_dir)
        .await
    {
        Ok(value) => {
            if let Some(id_str) = value.get("session_id").and_then(Value::as_str) {
                if let Ok(id) = id_str.parse::<Uuid>() {
                    return Some((id, Status::Starting));
                }
                tracing::warn!(pane = %key, "session.discover returned non-UUID session_id");
            } else {
                tracing::warn!(pane = %key, "session.discover returned no session_id");
            }
        }
        Err(e) => tracing::warn!(error = %e, pane = %key, "session.discover failed"),
    }
    None
}

/// Emit `session.finished` for each vanished key, then remove it
/// from `known`.
async fn emit_finished_for_vanished(
    client: &mut AgentdClient,
    known: &mut std::collections::HashMap<String, PaneState>,
    vanished: Vec<String>,
) {
    for key in vanished {
        if let Some((session_id, _)) = known.remove(&key) {
            let payload = serde_json::json!({});
            match client
                .report_event(session_id, "session.finished", payload)
                .await
            {
                Ok(_) => {
                    tracing::info!(pane = %key, %session_id, "emitted session.finished");
                }
                Err(e) => {
                    tracing::warn!(error = %e, pane = %key, "session.finished report failed");
                }
            }
        }
    }
}

/// Re-scan tmux for opencode panes, emit `session.finished` for
/// vanished ones, and discover any newly appeared panes. Failures
/// from `enumerate_panes` are logged and swallowed so the watch loop
/// can keep running.
async fn run_pane_check(
    client: &mut AgentdClient,
    known: &mut std::collections::HashMap<String, PaneState>,
    tmux: &Path,
) {
    let Ok(all_panes) = enumerate_panes(tmux).await else {
        return;
    };
    let current: HashMap<String, String> = all_panes
        .iter()
        .map(|p| {
            (
                pane_key_from(&p.session, &p.pane_id),
                p.pane_current_command.clone(),
            )
        })
        .collect();
    let (vanished, new_keys) = diff_pane_states(known, &current);
    let continuing: Vec<&String> = known.keys().filter(|k| current.contains_key(*k)).collect();
    tracing::debug!(
        vanished = vanished.len(),
        new = new_keys.len(),
        continuing = continuing.len(),
        "pane_check tick",
    );
    emit_finished_for_vanished(client, known, vanished).await;
    discover_new_raw_panes(client, known, &all_panes, &new_keys).await;
}

/// Discover raw panes in `all_panes` whose `pane_key` appears in
/// `new_keys` and register each with the daemon. `new_keys` is the
/// output of [`diff_pane_states`] — pane keys whose `pane_current_command`
/// was `opencode` but are not yet in `known`.
async fn discover_new_raw_panes(
    client: &mut AgentdClient,
    known: &mut std::collections::HashMap<String, PaneState>,
    all_panes: &[crate::discovery::RawPane],
    new_keys: &[String],
) {
    #[allow(clippy::map_entry)] // the value is async; entry().or_insert_with() can't be used
    for pane in all_panes {
        let key = pane_key_from(&pane.session, &pane.pane_id);
        if !new_keys.contains(&key) {
            continue;
        }
        let working_dir = pane.working_dir.to_string_lossy().to_string();
        match client
            .discover(&pane.session, &pane.pane_id, &working_dir)
            .await
        {
            Ok(value) => {
                if let Some(id_str) = value.get("session_id").and_then(Value::as_str) {
                    if let Ok(id) = id_str.parse::<Uuid>() {
                        known.insert(key, (id, Status::Starting));
                        continue;
                    }
                    tracing::warn!(pane = %key, "session.discover returned non-UUID session_id");
                } else {
                    tracing::warn!(pane = %key, "session.discover returned no session_id");
                }
            }
            Err(e) => tracing::warn!(error = %e, pane = %key, "session.discover failed"),
        }
    }
}

/// Watch loop. Calls `session.discover` for each new pane, then every
/// `interval` captures the pane content and emits
/// `session.status_changed` on change. Every `pane_check_interval`
/// re-scans tmux for opencode panes; vanished panes get a
/// `session.finished` event, newly appeared panes get a fresh
/// `session.discover`. Sends `plugin.heartbeat` every 5 seconds.
///
/// Returns `Ok(())` only when the loop is cancelled by aborting the
/// task; it is intended to run for the lifetime of the plugin
/// process. The `tmux` path is taken explicitly so tests can inject a
/// fake `tmux` script.
pub async fn run(
    client: &mut AgentdClient,
    panes: Vec<OpencodePane>,
    interval: Duration,
    pane_check_interval: Duration,
    tmux: &Path,
) -> anyhow::Result<()> {
    let mut known: std::collections::HashMap<String, PaneState> = std::collections::HashMap::new();
    for pane in &panes {
        if let Some(state) = discover_pane(client, pane).await {
            known.insert(pane_key(pane), state);
        }
    }

    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
    heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pane_check_ticker = tokio::time::interval(pane_check_interval);
    pane_check_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
            _ = pane_check_ticker.tick() => {
                run_pane_check(client, &mut known, tmux).await;
            }
            _ = heartbeat_interval.tick() => {
                if let Err(e) = client.heartbeat().await {
                    tracing::warn!(error = %e, "heartbeat failed");
                }
            }
        }
    }
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
