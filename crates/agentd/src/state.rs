use crate::db::Db;
use crate::db::repo::PluginRecord;
use crate::db::repo::SessionRepo;
use agentd_protocol::Session;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("repo: {0}")]
    Repo(#[from] crate::db::repo::RepoError),
}

/// Immutable snapshot of session + plugin state. Returned by
/// `state.snapshot` and rebuilt by the event bus on every event.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub sessions: Vec<Session>,
    pub plugins: Vec<PluginRecord>,
}

impl State {
    /// Read all sessions and plugins from `SQLite`.
    pub fn capture(db: &Db) -> Result<Self, StateError> {
        let sessions = SessionRepo::new(db).list()?;
        let plugins = crate::db::repo::PluginRepo::new(db).list()?;
        Ok(Self { sessions, plugins })
    }
}
