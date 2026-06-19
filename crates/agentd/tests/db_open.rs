#![allow(clippy::expect_used)]

use tempfile::TempDir;

#[test]
fn opens_and_creates_file() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("state.db");
    let db = agentd::db::Db::open(&path).expect("open");
    assert!(path.exists());
    drop(db);
}

#[test]
fn wal_mode_is_set() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("state.db");
    let db = agentd::db::Db::open(&path).expect("open");
    let mode: String = db
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("pragma");
    assert_eq!(mode.to_lowercase(), "wal", "expected WAL, got {mode}");
}

#[test]
fn foreign_keys_are_on() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("state.db");
    let db = agentd::db::Db::open(&path).expect("open");
    let fk: i64 = db
        .conn()
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .expect("pragma");
    assert_eq!(fk, 1, "foreign_keys must be ON");
}

#[test]
fn open_is_idempotent() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("state.db");
    let _ = agentd::db::Db::open(&path).expect("first");
    let _ = agentd::db::Db::open(&path).expect("second");
}
