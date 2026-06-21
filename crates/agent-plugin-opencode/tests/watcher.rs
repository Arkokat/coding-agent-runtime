#![allow(clippy::expect_used)]

//! End-to-end tests for `agent_plugin_opencode::watcher`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_plugin_opencode::discovery::OpencodePane;
use agent_plugin_opencode::watcher::{Status, capture_pane, parse_status, run};
use agentd_plugin_sdk::AgentdClient;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

#[test]
fn parse_status_working_keywords() {
    assert_eq!(parse_status("Compiling foo v0.1.0"), Some(Status::Working));
    assert_eq!(parse_status("Building dependencies"), Some(Status::Working));
    assert_eq!(parse_status("Running test suite"), Some(Status::Working));
    assert_eq!(parse_status("Loading model..."), Some(Status::Working));
    assert_eq!(parse_status("Processing request"), Some(Status::Working));
}

#[test]
fn parse_status_errored_keywords() {
    assert_eq!(parse_status("Error: file not found"), Some(Status::Errored));
    assert_eq!(
        parse_status("error[E0425]: cannot find value"),
        Some(Status::Errored)
    );
    assert_eq!(
        parse_status("thread 'main' panicked"),
        Some(Status::Errored)
    );
    assert_eq!(parse_status("test failed: 0 passed"), Some(Status::Errored));
}

#[test]
fn parse_status_idle_prompt() {
    assert_eq!(parse_status("Build complete. ❯"), Some(Status::Idle));
    assert_eq!(parse_status("Ready  > "), Some(Status::Idle));
    assert_eq!(parse_status("❯"), Some(Status::Idle));
}

#[test]
fn parse_status_unknown() {
    assert_eq!(parse_status(""), None);
    assert_eq!(parse_status("thinking..."), None);
}

#[tokio::test]
async fn capture_pane_invokes_tmux_with_pane_and_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let argv_log = dir.path().join("argv.log");
    let argv_log_s = argv_log.to_string_lossy().to_string();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" >> {argv_log_s}\nprintf 'Compiling foo v0.1.0\\nBuild complete. \u{276f}\\n'\n"
    );
    let path = write_fake_tmux(dir.path(), &script);
    let captured = capture_pane("%3", 50, &path).await;
    assert!(
        captured.contains("Compiling foo v0.1.0"),
        "expected captured pane to include working keyword, got: {captured:?}"
    );
    // Verify argv shape: `capture-pane -p -t %3 -S -50`.
    let argv = std::fs::read_to_string(&argv_log).expect("read argv log");
    let parts: Vec<&str> = argv.lines().flat_map(|l| l.split_whitespace()).collect();
    let tail: Vec<&str> = parts
        .iter()
        .rev()
        .take(6)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    assert_eq!(
        tail,
        vec!["capture-pane", "-p", "-t", "%3", "-S", "-50"],
        "argv tail mismatch: {argv:?}"
    );
}

#[tokio::test]
async fn capture_pane_returns_empty_string_when_tmux_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("no-such-tmux");
    let captured = capture_pane("%0", 50, &missing).await;
    assert!(captured.is_empty());
}

#[tokio::test]
async fn capture_pane_returns_empty_string_when_tmux_exits_non_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_fake_tmux(dir.path(), "#!/bin/sh\nexit 7\n");
    let captured = capture_pane("%0", 50, &path).await;
    assert!(captured.is_empty());
}

fn write_fake_tmux(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("tmux");
    std::fs::write(&path, body).expect("write fake tmux");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).expect("stat").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
    }
    path
}

/// Recorded event emitted by the fake daemon server.
#[derive(Debug, Clone)]
struct RecordedEvent {
    method: String,
    session_id: Option<String>,
    kind: Option<String>,
    status: Option<String>,
}

/// Spin up a fake agentd daemon on a UDS path. It accepts one
/// connection, dispatches JSON-RPC calls, and records every
/// `session.report_event` payload. Returns the socket path and the
/// shared event log.
fn spawn_fake_daemon() -> (PathBuf, Arc<Mutex<Vec<RecordedEvent>>>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("agentd.sock");
    let listener = UnixListener::bind(&sock).expect("bind uds");
    let log: Arc<Mutex<Vec<RecordedEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let log_for_task = Arc::clone(&log);
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let (read, mut write) = stream.into_split();
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
                let trimmed = line.trim();
                let req: Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let id = req.get("id").cloned().unwrap_or(Value::Null);
                let method = req
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let params = req.get("params").cloned().unwrap_or(Value::Null);
                let mut rec = RecordedEvent {
                    method: method.clone(),
                    session_id: None,
                    kind: None,
                    status: None,
                };
                let result = match method.as_str() {
                    "session.discover" => {
                        let id_str = uuid::Uuid::now_v7().to_string();
                        rec.session_id = Some(id_str.clone());
                        Value::String(id_str)
                    }
                    "session.report_event" => {
                        rec.session_id = params
                            .get("session_id")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        rec.kind = params
                            .get("type")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        rec.status = params
                            .get("payload")
                            .and_then(|p| p.get("status"))
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        Value::Bool(true)
                    }
                    "plugin.heartbeat" => Value::Bool(true),
                    _ => Value::Null,
                };
                log_for_task.lock().expect("lock").push(rec);
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "ok": true,
                        "session_id": result
                    }
                });
                let mut bytes = serde_json::to_vec(&resp).expect("encode");
                bytes.push(b'\n');
                let _ = write.write_all(&bytes).await;
                line.clear();
            }
        }
        // Keep the tempdir alive for the duration of the test so
        // the socket path stays valid.
        let _ = dir.keep();
    });
    (sock, log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs AF_UNIX bind (some local sandboxes block it)"]
async fn run_loop_emits_status_events_on_change() {
    let (sock, log) = spawn_fake_daemon();
    let tmux = write_cycling_tmux_script();
    let panes = vec![OpencodePane {
        tmux_session: "dev".into(),
        pane_id: "%0".into(),
        pane_pid: 1,
        working_dir: PathBuf::from("/tmp/proj"),
    }];

    let mut client = AgentdClient::connect(&sock).await.expect("connect");
    let task = tokio::spawn(async move {
        let _ = run(
            &mut client,
            panes,
            Duration::from_millis(150),
            Duration::from_millis(500),
            &tmux,
        )
        .await;
    });

    let got_three = wait_for_n_distinct_statuses(&log, 3, Duration::from_secs(5)).await;
    task.abort();
    let _ = task.await;

    let snapshot = log.lock().expect("lock").clone();
    assert_run_loop_results(&snapshot, got_three);
}

fn write_cycling_tmux_script() -> PathBuf {
    let dir = tempfile::tempdir().expect("tempdir");
    let counter = dir.path().join("counter");
    std::fs::write(&counter, "0\n").expect("write counter");
    let counter_s = counter.to_string_lossy().to_string();
    let body = format!(
        "#!/bin/sh\n\
c=\"{counter_s}\"\n\
if [ ! -f \"$c\" ]; then echo 0 > \"$c\"; fi\n\
n=$(cat \"$c\")\n\
n=$((n + 1))\n\
echo \"$n\" > \"$c\"\n\
case \"$n\" in\n\
    1) printf 'Compiling foo v0.1.0\\n' ;;\n\
    2) printf 'Building dependencies\\n' ;;\n\
    3) printf 'Error: oops\\n' ;;\n\
    *) printf 'Build complete. \u{276f}\\n' ;;\n\
esac\n"
    );
    let path = write_fake_tmux(dir.path(), &body);
    let _ = dir.keep();
    path
}

async fn wait_for_n_distinct_statuses(
    log: &Arc<Mutex<Vec<RecordedEvent>>>,
    n: usize,
    timeout: Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let snapshot = log.lock().expect("lock").clone();
        let mut unique: std::collections::HashSet<String> = snapshot
            .iter()
            .filter(|r| r.method == "session.report_event")
            .filter_map(|r| r.status.clone())
            .collect();
        unique.retain(|s| s != "starting");
        if unique.len() >= n {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[allow(clippy::too_many_lines)]
fn assert_run_loop_results(snapshot: &[RecordedEvent], got_three: bool) {
    let discover_count = snapshot
        .iter()
        .filter(|r| r.method == "session.discover")
        .count();
    assert_eq!(
        discover_count, 1,
        "expected exactly one discover, got: {snapshot:?}"
    );
    let report_events: Vec<_> = snapshot
        .iter()
        .filter(|r| r.method == "session.report_event")
        .collect();
    assert!(
        got_three,
        "expected at least 3 distinct status events, got {}: {snapshot:?}",
        report_events.len()
    );
    let kinds: Vec<&str> = report_events
        .iter()
        .filter_map(|r| r.status.as_deref())
        .collect();
    assert!(
        kinds.contains(&"working"),
        "expected a working event, got: {kinds:?}"
    );
    assert!(
        kinds.contains(&"errored"),
        "expected an errored event, got: {kinds:?}"
    );
    assert!(
        kinds.contains(&"idle"),
        "expected an idle event, got: {kinds:?}"
    );
    let all_session_ids: std::collections::HashSet<_> = report_events
        .iter()
        .filter_map(|r| r.session_id.clone())
        .collect();
    assert_eq!(
        all_session_ids.len(),
        1,
        "all report_events should target the same session_id, got: {all_session_ids:?}"
    );
    let event_kinds: Vec<&str> = report_events
        .iter()
        .filter_map(|r| r.kind.as_deref())
        .collect();
    assert!(
        event_kinds.iter().all(|k| *k == "session.status_changed"),
        "all report_events should be session.status_changed, got: {event_kinds:?}"
    );
}
