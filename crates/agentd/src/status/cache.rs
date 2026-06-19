use crate::db::repo::SessionRepo;
use crate::db::Db;
use agentd_protocol::SessionStatus;
use parking_lot::RwLock;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("repo: {0}")]
    Repo(#[from] crate::db::repo::RepoError),
}

#[derive(Debug, Clone)]
pub struct FormattedStatus {
    pub agent: String,
    pub status: SessionStatus,
    pub task: Option<String>,
    pub cost_usd: Option<f64>,
}

/// In-memory cache for the tmux status line. Rebuilt on event bus
/// activity; the formatters never touch `SQLite`.
///
/// Performance target (spec section 5): cold <5ms, p99 <50ms, hard
/// limit 1s. Unknown pane → empty string in <1ms.
pub struct StatusCache {
    by_pane: RwLock<HashMap<String, FormattedStatus>>,
    global: RwLock<GlobalSummary>,
}

#[derive(Debug, Clone, Default)]
struct GlobalSummary {
    total: usize,
    by_status: HashMap<SessionStatus, usize>,
    cost_usd: f64,
}

impl StatusCache {
    pub fn new() -> Self {
        Self {
            by_pane: RwLock::new(HashMap::new()),
            global: RwLock::new(GlobalSummary::default()),
        }
    }

    /// Wipe and refill the cache from `SQLite`. This is the only
    /// method that touches the DB.
    pub fn rebuild(&self, db: &Db) -> Result<usize, CacheError> {
        let sessions = SessionRepo::new(db).list_non_finished()?;
        let mut by_pane = self.by_pane.write();
        let mut global = self.global.write();
        by_pane.clear();
        let mut summary = GlobalSummary::default();
        for s in &sessions {
            summary.total += 1;
            *summary.by_status.entry(s.status).or_insert(0) += 1;
            if let Some(c) = s.cost_usd {
                summary.cost_usd += c;
            }
            if let Some(pane) = &s.tmux_pane_id {
                by_pane.insert(
                    pane.clone(),
                    FormattedStatus {
                        agent: s.agent_type.as_str().to_string(),
                        status: s.status,
                        task: s.current_task.clone(),
                        cost_usd: s.cost_usd,
                    },
                );
            }
        }
        *global = summary;
        Ok(by_pane.len())
    }

    /// Per-pane line: `"claude · editing src/foo.rs"` (spec section 5).
    /// Empty string for unknown pane — must not call into DB.
    pub fn format_pane(&self, pane: &str) -> String {
        let g = self.by_pane.read();
        match g.get(pane) {
            Some(f) => match &f.task {
                Some(t) => format!("{} · {}", f.agent, t),
                None => f.agent.clone(),
            },
            None => String::new(),
        }
    }

    /// Aggregate line: `"5 agents · 2 waiting · 1 working · $0.42"`.
    pub fn format_global(&self) -> String {
        let g = self.global.read();
        let working = g.by_status.get(&SessionStatus::Working).copied().unwrap_or(0);
        let waiting = g
            .by_status
            .get(&SessionStatus::WaitingForUser)
            .copied()
            .unwrap_or(0);
        let idle = g.by_status.get(&SessionStatus::Idle).copied().unwrap_or(0);
        let errored = g.by_status.get(&SessionStatus::Errored).copied().unwrap_or(0);
        let cost = format!("${:.2}", g.cost_usd);
        format!(
            "{total} agents · {waiting} waiting · {working} working · {idle} idle · {errored} errored · {cost}",
            total = g.total,
        )
    }
}

impl Default for StatusCache {
    fn default() -> Self {
        Self::new()
    }
}
