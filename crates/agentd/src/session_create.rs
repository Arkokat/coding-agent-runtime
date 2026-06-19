use crate::db::Db;
use crate::db::repo::SessionRepo;
use crate::tmux::{Tmux, validate_session_name};
use agentd_protocol::{AgentType, Session, SessionSource, SessionStatus};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CreateError {
    #[error("invalid name: {0}")]
    InvalidName(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("tmux: {0}")]
    Tmux(#[from] crate::tmux::TmuxError),
    #[error("db: {0}")]
    Db(#[from] crate::db::repo::RepoError),
    #[error("internal: {0}")]
    Internal(String),
}

/// Create a new session end-to-end:
///   1. Validate the display name
///   2. INSERT a `starting` row
///   3. Create a tmux session for the agent
///   4. Bind the pane id back to the row
///   5. Return the persisted `Session`
///
/// On any failure after step 2, the row is deleted and the tmux
/// session is killed (rollback). The plugin will discover the tmux
/// pane and call `session.discover` to re-attach.
pub async fn create_session(
    tmux: &dyn Tmux,
    db: &Db,
    agent_type: AgentType,
    working_dir: &str,
    display_name: Option<&str>,
) -> Result<Session, CreateError> {
    let name = display_name.unwrap_or("session");
    validate_session_name(name).map_err(|_| CreateError::InvalidName(name.to_string()))?;

    let id = Uuid::now_v7();
    let session = Session {
        id,
        agent_type,
        working_dir: working_dir.to_string(),
        tmux_session: None,
        tmux_pane_id: None,
        display_name: name.to_string(),
        status: SessionStatus::Starting,
        current_task: None,
        model: None,
        context_used_tokens: None,
        context_total_tokens: None,
        cost_usd: None,
        source: SessionSource::Cli,
        created_at: chrono::Utc::now(),
        last_event_at: None,
        finished_at: None,
        metadata: serde_json::json!({}),
    };

    SessionRepo::new(db).insert(&session)?;

    // tmux session name: use display_name (already validated).
    let pane_id = match tmux.new_session(name, working_dir).await {
        Ok(p) => p,
        Err(e) => {
            // Rollback: delete the row.
            let _ = db.conn().execute(
                "DELETE FROM sessions WHERE id = ?1",
                rusqlite::params![id.to_string()],
            );
            return Err(CreateError::Tmux(e));
        }
    };

    if let Err(e) = SessionRepo::new(db).update_tmux(&id, Some(name), Some(&pane_id)) {
        // Rollback: kill tmux, delete row.
        let _ = tmux.kill_session(name).await;
        let _ = db.conn().execute(
            "DELETE FROM sessions WHERE id = ?1",
            rusqlite::params![id.to_string()],
        );
        return Err(CreateError::Db(e));
    }

    // Reload the row with the tmux info filled in.
    let mut updated = SessionRepo::new(db).get(&id)?.ok_or_else(|| {
        CreateError::Internal("session disappeared after update_tmux".to_string())
    })?;
    updated.tmux_session = Some(name.to_string());
    updated.tmux_pane_id = Some(pane_id);
    Ok(updated)
}
