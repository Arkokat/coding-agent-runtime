use crate::db::wrapper::Db;
use agentd_protocol::{Session, SessionStatus};
use chrono::{DateTime, Utc};
use rusqlite::{Error as SqliteError, params, params_from_iter};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] SqliteError),
    #[error("session not found: {0}")]
    NotFound(Uuid),
    #[error("invalid status string: {0}")]
    InvalidStatus(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EventRecord {
    pub id: i64,
    pub session_id: Uuid,
    pub kind: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PluginRecord {
    pub name: String,
    pub binary: String,
    pub socket_name: String,
    pub autostart: bool,
    pub last_connected_at: Option<String>,
    pub last_error: Option<String>,
}

pub struct SessionRepo<'a> {
    db: &'a Db,
}

impl<'a> SessionRepo<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Insert a new session row. Serializes `source` to a JSON-quoted string
    /// (e.g. `"cli"`) for the TEXT column.
    ///
    /// # Panics
    /// Panics if `serde_json::to_value(s.source)` fails (should never happen
    /// for the enum's derived `Serialize`).
    pub fn insert(&self, s: &Session) -> Result<(), RepoError> {
        let source_str = serde_json::to_string(&serde_json::to_value(s.source).unwrap())
            .unwrap_or_else(|_| "\"cli\"".into());
        self.db.conn().execute(
            "INSERT INTO sessions
             (id, agent_type, working_dir, tmux_session, tmux_pane_id, display_name,
              status, current_task, model, context_used_tokens, context_total_tokens,
              cost_usd, source, created_at, last_event_at, finished_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                s.id.to_string(),
                s.agent_type.as_str(),
                s.working_dir,
                s.tmux_session,
                s.tmux_pane_id,
                s.display_name,
                s.status.to_string(),
                s.current_task,
                s.model,
                s.context_used_tokens,
                s.context_total_tokens,
                s.cost_usd,
                source_str,
                s.created_at.to_rfc3339(),
                s.last_event_at.map(|t| t.to_rfc3339()),
                s.finished_at.map(|t| t.to_rfc3339()),
                s.metadata.to_string(),
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &Uuid) -> Result<Option<Session>, RepoError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, agent_type, working_dir, tmux_session, tmux_pane_id, display_name,
                    status, current_task, model, context_used_tokens, context_total_tokens,
                    cost_usd, source, created_at, last_event_at, finished_at, metadata
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_session(row)?)),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> Result<Vec<Session>, RepoError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, agent_type, working_dir, tmux_session, tmux_pane_id, display_name,
                    status, current_task, model, context_used_tokens, context_total_tokens,
                    cost_usd, source, created_at, last_event_at, finished_at, metadata
             FROM sessions ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map([], row_to_session)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_non_finished(&self) -> Result<Vec<Session>, RepoError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|s| !s.status.is_terminal())
            .collect())
    }

    pub fn update_status(&self, id: &Uuid, status: SessionStatus) -> Result<(), RepoError> {
        let n = self.db.conn().execute(
            "UPDATE sessions SET status = ?1, last_event_at = ?2 WHERE id = ?3",
            params![status.to_string(), Utc::now().to_rfc3339(), id.to_string()],
        )?;
        if n == 0 {
            return Err(RepoError::NotFound(*id));
        }
        Ok(())
    }

    pub fn update_task(&self, id: &Uuid, task: Option<&str>) -> Result<(), RepoError> {
        let n = self.db.conn().execute(
            "UPDATE sessions SET current_task = ?1, last_event_at = ?2 WHERE id = ?3",
            params![task, Utc::now().to_rfc3339(), id.to_string()],
        )?;
        if n == 0 {
            return Err(RepoError::NotFound(*id));
        }
        Ok(())
    }

    pub fn update_usage(
        &self,
        id: &Uuid,
        used: Option<u64>,
        total: Option<u64>,
        cost: Option<f64>,
    ) -> Result<(), RepoError> {
        let n = self.db.conn().execute(
            "UPDATE sessions
             SET context_used_tokens = ?1, context_total_tokens = ?2, cost_usd = ?3,
                 last_event_at = ?4
             WHERE id = ?5",
            params![used, total, cost, Utc::now().to_rfc3339(), id.to_string()],
        )?;
        if n == 0 {
            return Err(RepoError::NotFound(*id));
        }
        Ok(())
    }

    pub fn update_rename(&self, id: &Uuid, name: &str) -> Result<(), RepoError> {
        let n = self.db.conn().execute(
            "UPDATE sessions SET display_name = ?1 WHERE id = ?2",
            params![name, id.to_string()],
        )?;
        if n == 0 {
            return Err(RepoError::NotFound(*id));
        }
        Ok(())
    }

    pub fn mark_finished(&self, id: &Uuid) -> Result<(), RepoError> {
        let n = self.db.conn().execute(
            "UPDATE sessions SET status = 'finished', finished_at = ?1, last_event_at = ?1
             WHERE id = ?2",
            params![Utc::now().to_rfc3339(), id.to_string()],
        )?;
        if n == 0 {
            return Err(RepoError::NotFound(*id));
        }
        Ok(())
    }

    pub fn update_tmux(
        &self,
        id: &Uuid,
        session: Option<&str>,
        pane: Option<&str>,
    ) -> Result<(), RepoError> {
        let n = self.db.conn().execute(
            "UPDATE sessions SET tmux_session = ?1, tmux_pane_id = ?2 WHERE id = ?3",
            params_from_iter([
                &Some(session.map(str::to_string)) as &dyn rusqlite::ToSql,
                &Some(pane.map(str::to_string)) as &dyn rusqlite::ToSql,
                &Some(id.to_string()) as &dyn rusqlite::ToSql,
            ]),
        )?;
        if n == 0 {
            return Err(RepoError::NotFound(*id));
        }
        Ok(())
    }
}

pub struct EventRepo<'a> {
    db: &'a Db,
}

impl<'a> EventRepo<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn insert(&self, session_id: &Uuid, kind: &str, payload: &Value) -> Result<i64, RepoError> {
        self.db.conn().execute(
            "INSERT INTO session_events (session_id, type, payload) VALUES (?1, ?2, ?3)",
            params![session_id.to_string(), kind, payload.to_string()],
        )?;
        Ok(self.db.conn().last_insert_rowid())
    }

    pub fn list_for_session(&self, session_id: &Uuid) -> Result<Vec<EventRecord>, RepoError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT id, session_id, type, payload, created_at
             FROM session_events
             WHERE session_id = ?1
             ORDER BY created_at ASC, id ASC",
        )?;
        let rows = stmt.query_map([session_id.to_string()], |r| {
            let id: i64 = r.get(0)?;
            let sid: String = r.get(1)?;
            let kind: String = r.get(2)?;
            let payload: String = r.get(3)?;
            let created_at: String = r.get(4)?;
            Ok(EventRecord {
                id,
                session_id: Uuid::parse_str(&sid).map_err(|e| {
                    SqliteError::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })?,
                kind,
                payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
                created_at,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub struct PluginRepo<'a> {
    db: &'a Db,
}

impl<'a> PluginRepo<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn upsert(
        &self,
        name: &str,
        binary: &str,
        socket_name: &str,
        autostart: bool,
    ) -> Result<(), RepoError> {
        self.db.conn().execute(
            "INSERT INTO plugins (name, binary, socket_name, autostart)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET
                binary = excluded.binary,
                socket_name = excluded.socket_name,
                autostart = excluded.autostart",
            params![name, binary, socket_name, i64::from(autostart)],
        )?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<PluginRecord>, RepoError> {
        let mut stmt = self.db.conn().prepare(
            "SELECT name, binary, socket_name, autostart, last_connected_at, last_error
             FROM plugins ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            let autostart: i64 = r.get(3)?;
            Ok(PluginRecord {
                name: r.get(0)?,
                binary: r.get(1)?,
                socket_name: r.get(2)?,
                autostart: autostart != 0,
                last_connected_at: r.get(4)?,
                last_error: r.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_last_connected(&self, name: &str, ts: DateTime<Utc>) -> Result<(), RepoError> {
        self.db.conn().execute(
            "UPDATE plugins SET last_connected_at = ?1 WHERE name = ?2",
            params![ts.to_rfc3339(), name],
        )?;
        Ok(())
    }
}

fn row_to_session(r: &rusqlite::Row<'_>) -> Result<Session, SqliteError> {
    let id_str: String = r.get(0)?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        SqliteError::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let agent_type_str: String = r.get(1)?;
    let agent_type = agent_type_from_str(&agent_type_str)?;
    let status_str: String = r.get(6)?;
    let status = status_from_str(&status_str)?;
    let source_str: String = r.get(12)?;
    let source: agentd_protocol::SessionSource =
        serde_json::from_str(&format!("\"{}\"", source_str.trim_matches('"'))).map_err(|e| {
            SqliteError::FromSqlConversionFailure(12, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let created_str: String = r.get(13)?;
    let last_event_str: Option<String> = r.get(14)?;
    let finished_str: Option<String> = r.get(15)?;
    let metadata_str: String = r.get(16)?;

    Ok(Session {
        id,
        agent_type,
        working_dir: r.get(2)?,
        tmux_session: r.get(3)?,
        tmux_pane_id: r.get(4)?,
        display_name: r.get(5)?,
        status,
        current_task: r.get(7)?,
        model: r.get(8)?,
        context_used_tokens: r.get(9)?,
        context_total_tokens: r.get(10)?,
        cost_usd: r.get(11)?,
        source,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .map_err(|e| {
                SqliteError::FromSqlConversionFailure(13, rusqlite::types::Type::Text, Box::new(e))
            })?
            .with_timezone(&Utc),
        last_event_at: match last_event_str {
            Some(s) => Some(
                DateTime::parse_from_rfc3339(&s)
                    .map_err(|e| {
                        SqliteError::FromSqlConversionFailure(
                            14,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?
                    .with_timezone(&Utc),
            ),
            None => None,
        },
        finished_at: match finished_str {
            Some(s) => Some(
                DateTime::parse_from_rfc3339(&s)
                    .map_err(|e| {
                        SqliteError::FromSqlConversionFailure(
                            15,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?
                    .with_timezone(&Utc),
            ),
            None => None,
        },
        metadata: serde_json::from_str(&metadata_str).unwrap_or(Value::Null),
    })
}

fn agent_type_from_str(s: &str) -> Result<agentd_protocol::AgentType, SqliteError> {
    use agentd_protocol::AgentType;
    Ok(match s {
        "opencode" => AgentType::Opencode,
        "claude-code" => AgentType::ClaudeCode,
        "codex" => AgentType::Codex,
        "aider" => AgentType::Aider,
        other => {
            return Err(SqliteError::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                format!("unknown agent_type: {other}").into(),
            ));
        }
    })
}

fn status_from_str(s: &str) -> Result<SessionStatus, SqliteError> {
    use agentd_protocol::SessionStatus;
    Ok(match s {
        "starting" => SessionStatus::Starting,
        "idle" => SessionStatus::Idle,
        "working" => SessionStatus::Working,
        "waiting_for_user" => SessionStatus::WaitingForUser,
        "errored" => SessionStatus::Errored,
        "finished" => SessionStatus::Finished,
        other => {
            return Err(SqliteError::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Text,
                format!("invalid status: {other}").into(),
            ));
        }
    })
}
