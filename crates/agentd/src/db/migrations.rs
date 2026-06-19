use crate::db::wrapper::Db;
use rusqlite::Error as SqliteError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("read migration {0}: {1}")]
    Read(String, std::io::Error),
    #[error("apply migration {0}: {1}")]
    Apply(String, SqliteError),
}

/// Apply all bundled migrations in version order. Idempotent: each
/// migration is recorded in `schema_migrations` and skipped on re-run.
pub fn run(db: &Db) -> Result<(), MigrationError> {
    const MIGRATIONS: &[(&str, &str)] =
        &[("0001_init", include_str!("../../migrations/0001_init.sql"))];

    for (name, sql) in MIGRATIONS {
        let version: i64 = name
            .split('_')
            .next()
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);

        let already: bool = db
            .conn()
            .query_row(
                "SELECT count(*) > 0 FROM schema_migrations WHERE version = ?1",
                [version],
                |r| r.get(0),
            )
            .unwrap_or(false);
        if already {
            continue;
        }

        let tx = db
            .conn()
            .unchecked_transaction()
            .map_err(|e| MigrationError::Apply((*name).into(), e))?;
        tx.execute_batch(sql)
            .map_err(|e| MigrationError::Apply((*name).into(), e))?;
        tx.execute(
            "INSERT INTO schema_migrations (version) VALUES (?1)",
            [version],
        )
        .map_err(|e| MigrationError::Apply((*name).into(), e))?;
        tx.commit()
            .map_err(|e| MigrationError::Apply((*name).into(), e))?;
    }
    Ok(())
}
