#![allow(clippy::expect_used)]

use agentd::db::Db;
use tempfile::TempDir;

fn open_test_db() -> (TempDir, Db) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("state.db");
    let db = Db::open(&path).expect("open");
    (dir, db)
}

#[test]
fn init_creates_all_five_tables() {
    let (_dir, db) = open_test_db();
    agentd::db::migrations::run(&db).expect("migrate");

    let names: Vec<String> = db
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("prep")
        .query_map([], |r| r.get(0))
        .expect("map")
        .filter_map(Result::ok)
        .collect();
    for required in [
        "schema_migrations",
        "sessions",
        "session_events",
        "plugins",
        "settings",
    ] {
        assert!(
            names.contains(&required.to_string()),
            "missing table: {required}"
        );
    }
}

#[test]
fn init_creates_sessions_indexes() {
    let (_dir, db) = open_test_db();
    agentd::db::migrations::run(&db).expect("migrate");
    let count: i64 = db
        .conn()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND tbl_name='sessions'",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert!(count >= 3, "expected >= 3 indexes on sessions, got {count}");
}

#[test]
fn migrate_is_idempotent() {
    let (_dir, db) = open_test_db();
    agentd::db::migrations::run(&db).expect("first");
    agentd::db::migrations::run(&db).expect("second");
    let count: i64 = db
        .conn()
        .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
        .expect("q");
    assert_eq!(count, 1, "0001 should be recorded exactly once");
}

#[test]
fn session_event_insert_with_cascade_delete_works() {
    let (_dir, db) = open_test_db();
    agentd::db::migrations::run(&db).expect("migrate");

    db.conn()
        .execute(
            "INSERT INTO sessions (id, agent_type, working_dir, display_name, status, source, created_at)
             VALUES ('id1', 'opencode', '/tmp', 'tmp', 'starting', 'cli', '2026-06-19T00:00:00Z')",
            [],
        )
        .expect("insert session");

    db.conn()
        .execute(
            "INSERT INTO session_events (session_id, type, payload) VALUES ('id1', 'session.started', '{}')",
            [],
        )
        .expect("insert event");

    db.conn()
        .execute("DELETE FROM sessions WHERE id = 'id1'", [])
        .expect("delete");
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT count(*) FROM session_events WHERE session_id = 'id1'",
            [],
            |r| r.get(0),
        )
        .expect("q");
    assert_eq!(n, 0, "ON DELETE CASCADE should remove the event");
}
