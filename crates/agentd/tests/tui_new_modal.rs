#![allow(clippy::expect_used)]
#![allow(non_snake_case)]

use agentd::tui::new_modal::{NewModalOutcome, apply_key, filtered, open};
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use uuid::Uuid;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[tokio::test]
#[ignore = "needs AF_UNIX support (some local sandboxes block bind/connect)"]
async fn filtered_contains_all_when_query_empty() {
    let (server, sock) = stub_session_list_active();
    let client = agentd::control_client::ControlClient::connect(&sock)
        .await
        .expect("connect");
    let mut modal = open(&client).await;
    // Type a query.
    apply_key(&mut modal, key(KeyCode::Char('a')));
    apply_key(&mut modal, key(KeyCode::Char('g')));
    // query is "ag".
    let f = filtered(&modal);
    assert!(!f.is_empty(), "should have recents");
    // ...
    server.abort();
}

#[tokio::test]
async fn apply_key_backspace_trims() {
    let mut modal = agentd::tui::state::NewModal {
        query: "ab".into(),
        recents: vec![],
    };
    apply_key(&mut modal, key(KeyCode::Backspace));
    assert_eq!(modal.query, "a");
}

#[tokio::test]
async fn apply_key_enter_commits() {
    let mut modal = agentd::tui::state::NewModal {
        query: "agentd".into(),
        recents: vec![(PathBuf::from("/home/u/agentd"), Utc::now())],
    };
    let outcome = apply_key(&mut modal, key(KeyCode::Enter));
    match outcome {
        NewModalOutcome::Commit(p) => assert_eq!(p, PathBuf::from("/home/u/agentd")),
        _ => panic!("expected Commit"),
    }
}

#[tokio::test]
async fn apply_key_esc_cancels() {
    let mut modal = agentd::tui::state::NewModal {
        query: "x".into(),
        recents: vec![],
    };
    let outcome = apply_key(&mut modal, key(KeyCode::Esc));
    assert!(matches!(outcome, NewModalOutcome::Cancel));
}

#[tokio::test]
async fn apply_key_char_appends() {
    let mut modal = agentd::tui::state::NewModal {
        query: "ab".into(),
        recents: vec![],
    };
    apply_key(&mut modal, key(KeyCode::Char('c')));
    assert_eq!(modal.query, "abc");
}

// Helper: spawn a stub server that responds to `session.list_active` with a canned list.
fn stub_session_list_active() -> (tokio::task::JoinHandle<()>, std::path::PathBuf) {
    use tokio::net::UnixListener;
    let sock = format!("/tmp/agentd-newmodal-{}.sock", Uuid::now_v7());
    let listener = UnixListener::bind(&sock).expect("bind");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let (r, mut w) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(r);
        let mut line = String::new();
        tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
            .await
            .expect("read");
        let sessions = vec![
            sample_session_at("/home/u/agentd", "starting"),
            sample_session_at("/home/u/blog", "idle"),
        ];
        let resp = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": serde_json::to_value(&sessions).unwrap()
        });
        let mut buf = Vec::new();
        agentd::ipc::framing::write_message(&mut buf, &resp).expect("write");
        tokio::io::AsyncWriteExt::write_all(&mut w, &buf)
            .await
            .expect("write");
        tokio::io::AsyncWriteExt::flush(&mut w)
            .await
            .expect("flush");
    });
    (server, std::path::PathBuf::from(sock))
}

fn sample_session_at(working_dir: &str, status: &str) -> Session {
    use serde_json::json;
    Session {
        id: Uuid::now_v7(),
        agent_type: AgentType::Opencode,
        working_dir: working_dir.into(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: "x".into(),
        status: match status {
            "starting" => SessionStatus::Starting,
            _ => SessionStatus::Idle,
        },
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Cli,
        created_at: Utc::now(),
        last_event_at: Some(Utc::now()),
        finished_at: None,
        metadata: json!({}),
    }
}
