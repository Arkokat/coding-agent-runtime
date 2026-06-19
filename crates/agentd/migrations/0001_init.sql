-- 0001_init.sql — initial agentd schema. Matches spec section 8 exactly.

CREATE TABLE sessions (
  id                   TEXT PRIMARY KEY,            -- UUID v7
  agent_type           TEXT NOT NULL,
  working_dir          TEXT NOT NULL,
  tmux_session         TEXT,
  tmux_pane_id         TEXT,
  display_name         TEXT NOT NULL,
  status               TEXT NOT NULL,               -- starting|idle|working|waiting_for_user|errored|finished
  current_task         TEXT,
  model                TEXT,
  context_used_tokens  INTEGER,
  context_total_tokens INTEGER,
  cost_usd             REAL,
  source               TEXT NOT NULL,               -- cli|discovered|resumed
  created_at           TEXT NOT NULL,
  last_event_at        TEXT,
  finished_at          TEXT,
  metadata             TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_sessions_status_event ON sessions(status, last_event_at DESC);
CREATE UNIQUE INDEX idx_sessions_tmux
  ON sessions(tmux_session, tmux_pane_id)
  WHERE tmux_session IS NOT NULL AND tmux_pane_id IS NOT NULL;
CREATE INDEX idx_sessions_agent ON sessions(agent_type);

CREATE TABLE session_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  type       TEXT NOT NULL,
  payload    TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_events_session_time ON session_events(session_id, created_at DESC);

CREATE TABLE plugins (
  name              TEXT PRIMARY KEY,
  binary            TEXT NOT NULL,
  socket_name       TEXT NOT NULL,
  autostart         INTEGER NOT NULL DEFAULT 1,
  last_connected_at TEXT,
  last_error        TEXT
);

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- Default rows (spec section 8)
INSERT INTO settings (key, value) VALUES ('scan_interval_secs',  '5');
INSERT INTO settings (key, value) VALUES ('status_interval_secs', '1');
INSERT INTO settings (key, value) VALUES ('default_agent',       'opencode');
INSERT INTO settings (key, value) VALUES ('auto_detect',         '1');
INSERT INTO settings (key, value) VALUES ('last_agent',          'opencode');
INSERT INTO settings (key, value) VALUES ('slow_status_warn_ms', '500');
