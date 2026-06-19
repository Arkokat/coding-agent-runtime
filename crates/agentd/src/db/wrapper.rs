use rusqlite::Connection;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("open {0}: {1}")]
    Open(PathBuf, rusqlite::Error),
    #[error("pragma: {0}")]
    Pragma(rusqlite::Error),
    #[error("migrate: {0}")]
    Migrate(#[from] crate::db::migrations::MigrationError),
}

/// Owned `SQLite` connection. Set up for WAL, foreign keys, and a sane
/// busy timeout. The daemon holds one of these; tests create their own
/// per tempdir.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open or create `state.db` at `path` and apply pragmas.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path).map_err(|e| DbError::Open(path.to_path_buf(), e))?;
        let db = Self { conn };
        db.apply_pragmas()?;
        db.ensure_migrations_table()?;
        Ok(db)
    }

    /// Borrow the underlying connection. Use sparingly; prefer the
    /// repository functions in the `repo` module.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    fn apply_pragmas(&self) -> Result<(), DbError> {
        self.conn.pragma_update(None, "journal_mode", "WAL").map_err(DbError::Pragma)?;
        self.conn.pragma_update(None, "synchronous", "NORMAL").map_err(DbError::Pragma)?;
        self.conn.pragma_update(None, "foreign_keys", "ON").map_err(DbError::Pragma)?;
        self.conn.pragma_update(None, "busy_timeout", 5000).map_err(DbError::Pragma)?;
        Ok(())
    }

    fn ensure_migrations_table(&self) -> Result<(), DbError> {
        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
                )",
                [],
            )
            .map_err(DbError::Pragma)?;
        Ok(())
    }
}
