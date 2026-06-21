#!/usr/bin/env bash
# scripts/e2e-smoke.sh — exercise the full agentd stack end-to-end.
#
# Steps:
#   1. Build `agentd` (debug profile) if not already built.
#   2. Set up a clean temp XDG tree (so the test does not pollute the
#      user's real daemon at $XDG_DATA_HOME/agentd, $XDG_RUNTIME_DIR/agentd, etc).
#   3. Start the daemon via `target/debug/agentd daemon start --detach`.
#   4. Poll the control UDS (`$XDG_RUNTIME_DIR/agentd/control.sock`) for up to 5s.
#   5. Spawn `opencode run "..."` in a fresh tmux pane (session `agentd-e2e`).
#   6. Poll `$XDG_DATA_HOME/agentd/state.db` for a session row with
#      `agent_type='opencode'` AND `status='finished'`. Timeout 30s.
#   7. Assert: at least one opencode session was discovered and at least
#      one reached the `finished` status.
#   8. Stop the daemon and clean up the tmux session.
#
# NOT for CI. Local hand-run only. Many sandboxes block AF_UNIX bind,
# sqlite WAL writes, and tmux; see scripts/README.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
AGENTD_BIN="$REPO_ROOT/target/debug/agentd"

# --- 1. Build ---------------------------------------------------------------
if [[ ! -x "$AGENTD_BIN" ]]; then
  echo ">> Building agentd (debug)..."
  (cd "$REPO_ROOT" && cargo build -p agentd)
fi

# --- 2. Clean temp XDG tree -------------------------------------------------
# Use a portable mktemp invocation. On macOS, `mktemp -d -t prefix.XXXXXX`
# does not expand the XXXXXX template (it appends a random suffix to the
# whole name), so we let mktemp pick the path and use `mktemp -d -t
# agentd-e2e` for a friendly prefix.
TMPDIR="$(mktemp -d -t agentd-e2e)"
cleanup() {
  local rc=$?
  set +e
  echo ">> Cleaning up..."
  if [[ -n "${DAEMON_PID:-}" ]]; then
    "$AGENTD_BIN" daemon stop >/dev/null 2>&1 || true
    kill "$DAEMON_PID" 2>/dev/null || true
  fi
  tmux kill-session -t agentd-e2e 2>/dev/null || true
  rm -rf "$TMPDIR"
  exit "$rc"
}
trap cleanup EXIT INT TERM

export XDG_RUNTIME_DIR="$TMPDIR/runtime"
export XDG_DATA_HOME="$TMPDIR/data"
export XDG_STATE_HOME="$TMPDIR/state"
export XDG_CONFIG_HOME="$TMPDIR/config"
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CONFIG_HOME"

# --- 2b. Minimal config so the opencode plugin is autostarted --------------
mkdir -p "$XDG_CONFIG_HOME/agentd"
cat > "$XDG_CONFIG_HOME/agentd/plugins.toml" <<'EOF'
[[plugins]]
name = "opencode"
binary = "agentd-plugin-opencode"
autostart = true
EOF

# --- 3. Start daemon --------------------------------------------------------
echo ">> Starting daemon..."
"$AGENTD_BIN" daemon start --detach

# --- 4. Wait for control UDS ------------------------------------------------
CONTROL_UDS="$XDG_RUNTIME_DIR/agentd/control.sock"
for _ in {1..50}; do
  if [[ -S "$CONTROL_UDS" ]]; then
    break
  fi
  sleep 0.1
done
if [[ ! -S "$CONTROL_UDS" ]]; then
  echo "FAIL: daemon did not bind $CONTROL_UDS within 5s" >&2
  echo "      (often a sandbox issue: see scripts/README.md)" >&2
  exit 1
fi
echo ">> Daemon ready: $CONTROL_UDS"

# --- 5. Spawn opencode in a fresh tmux pane --------------------------------
echo ">> Spawning opencode in tmux session 'agentd-e2e'..."
tmux new-session -d -s agentd-e2e -x 200 -y 50 \
  "opencode run 'say hello and exit' || true"

# --- 6. Poll the DB ---------------------------------------------------------
DB="$XDG_DATA_HOME/agentd/state.db"
echo ">> Polling $DB for an opencode session (status=finished)..."
FOUND_FINISHED=0
for _ in {1..60}; do
  if [[ -f "$DB" ]]; then
    FINISHED=$(sqlite3 "$DB" \
      "SELECT COUNT(*) FROM sessions
       WHERE agent_type='opencode' AND status='finished';" \
      2>/dev/null || echo 0)
    if [[ "$FINISHED" -gt 0 ]]; then
      FOUND_FINISHED=1
      break
    fi
  fi
  sleep 0.5
done

# --- 7. Assert --------------------------------------------------------------
TOTAL=$(sqlite3 "$DB" \
  "SELECT COUNT(*) FROM sessions WHERE agent_type='opencode';" \
  2>/dev/null || echo 0)
if [[ "$TOTAL" -eq 0 ]]; then
  echo "FAIL: no opencode session was discovered in $DB" >&2
  exit 1
fi
if [[ "$FOUND_FINISHED" -eq 0 ]]; then
  echo "FAIL: discovered $TOTAL opencode session(s) but none reached 'finished' within 30s" >&2
  sqlite3 "$DB" \
    "SELECT id, agent_type, status, source, created_at
     FROM sessions WHERE agent_type='opencode'
     ORDER BY created_at DESC LIMIT 5;" >&2 || true
  exit 1
fi

echo ">> Sessions discovered:"
sqlite3 "$DB" \
  "SELECT id, agent_type, status, source
   FROM sessions WHERE agent_type='opencode'
   ORDER BY created_at DESC LIMIT 5;" || true
echo ">> PASS: smoke test succeeded ($TOTAL opencode session(s), at least 1 finished)"
