//! `SQLite` layer. Owns the `state.db` connection. Single writer (the daemon);
//! CLI/TUI may open read-only via `Db::open_read_only` (added in Task 11).

pub mod migrations;
pub mod repo;
mod wrapper;

pub use wrapper::{Db, DbError};
