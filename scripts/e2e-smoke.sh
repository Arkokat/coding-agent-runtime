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

# --- 0. Args ----------------------------------------------------------------
# `--keep-tmp` preserves $TMPDIR on exit (so logs survive for inspection).
# `--log-file <path>` sets AGENTD_LOG_FILE; defaults to
# `$TMPDIR/state/agentd/daemon.log` once TMPDIR is known (see step 2).
KEEP_TMP=0
LOG_FILE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --keep-tmp) KEEP_TMP=1; shift ;;
    --log-file)  LOG_FILE="$2"; shift 2 ;;
    -h|--help)
      cat <<USAGE
Usage: $0 [--keep-tmp] [--log-file <path>]
  --keep-tmp      preserve the temp XDG tree on exit
  --log-file <p>  write daemon + plugin tracing to <p>
USAGE
      exit 0
      ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

# --- 1. Build ---------------------------------------------------------------
# Build both the daemon AND the opencode plugin (the daemon spawns the plugin
# by name, so the plugin's binary must be on $PATH at daemon-start time).
# Always rebuild — smoke tests live or die by running the latest source.
TARGET_BIN="$REPO_ROOT/target/debug"
echo ">> Building agentd + agentd-plugin-opencode (debug)..."
(cd "$REPO_ROOT" && cargo build -p agentd -p agentd-plugin-opencode)
# Make the plugin (and any other in-tree binaries) discoverable on $PATH.
export PATH="$TARGET_BIN:$PATH"
# Belt-and-suspenders: also tell the daemon EXACTLY where the plugin
# lives via $AGENTD_PLUGIN_BIN_DIR. cwd and PATH can be unreliable
# after the daemon's double-fork detach; this env var is read by
# `RealPluginSpawner::resolve` as the first lookup step.
export AGENTD_PLUGIN_BIN_DIR="$TARGET_BIN"

# --- 2. Clean temp XDG tree -------------------------------------------------
# Use `/tmp` directly (not `$TMPDIR`) so the resulting path stays well
# under macOS's SUN_LEN=104 limit (UDS sun_path is 104 bytes INCLUDING
# the null terminator; /var/folders/.../T/ alone is 48 bytes, plus
# `runtime/agentd/plugin-<name>.sock` is 32 bytes — too close for
# mktemp's variable random suffix).
TMPDIR="$(mktemp -d /tmp/agentd-e2e.XXXXXXXX)"
cleanup() {
  local rc=$?
  set +e
  echo ">> Cleaning up..."
  if [[ -n "${DAEMON_PID:-}" ]]; then
    "$AGENTD_BIN" daemon stop >/dev/null 2>&1 || true
    kill "$DAEMON_PID" 2>/dev/null || true
  fi
  tmux kill-session -t agentd-e2e 2>/dev/null || true
  if [[ "$KEEP_TMP" -eq 1 ]]; then
    echo ">> --keep-tmp: preserving $TMPDIR (logs at $LOG_FILE)" >&2
  else
    rm -rf "$TMPDIR"
  fi
  exit "$rc"
}
trap cleanup EXIT INT TERM

export XDG_RUNTIME_DIR="$TMPDIR/runtime"
export XDG_DATA_HOME="$TMPDIR/data"
export XDG_STATE_HOME="$TMPDIR/state"
export XDG_CONFIG_HOME="$TMPDIR/config"
mkdir -p "$XDG_RUNTIME_DIR" "$XDG_DATA_HOME" "$XDG_STATE_HOME" "$XDG_CONFIG_HOME"

# Default log file: under the temp XDG state dir so cleanup --keep-tmp
# can find it. User can override with --log-file.
if [[ -z "$LOG_FILE" ]]; then
  LOG_FILE="$XDG_STATE_HOME/agentd/daemon.log"
fi
mkdir -p "$(dirname "$LOG_FILE")"
export AGENTD_LOG_FILE="$LOG_FILE"
# Verbose tracing. `info` is the default; the targeted `debug` and
# `trace` levels surface the watcher tick, the plugin accept loop,
# and the discovery pass — the exact signals the smoke test needs
# to diagnose "no opencode session was discovered" failures.
export RUST_LOG="agentd=debug,agentd::plugin_supervisor=trace,agent_plugin_opencode=debug,agentd_plugin_sdk=debug,info"
echo ">> log file: $LOG_FILE"

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
  if [[ -f "$LOG_FILE" ]]; then
    echo ">> Last 60 lines of $LOG_FILE:" >&2
    tail -60 "$LOG_FILE" >&2 || true
  else
    echo ">> No log file at $LOG_FILE" >&2
  fi
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
  if [[ -f "$LOG_FILE" ]]; then
    echo ">> Last 60 lines of $LOG_FILE:" >&2
    tail -60 "$LOG_FILE" >&2 || true
  else
    echo ">> No log file at $LOG_FILE" >&2
  fi
  exit 1
fi
if [[ "$FOUND_FINISHED" -eq 0 ]]; then
  echo "FAIL: discovered $TOTAL opencode session(s) but none reached 'finished' within 30s" >&2
  if [[ -f "$LOG_FILE" ]]; then
    echo ">> Last 60 lines of $LOG_FILE:" >&2
    tail -60 "$LOG_FILE" >&2 || true
  else
    echo ">> No log file at $LOG_FILE" >&2
  fi
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
