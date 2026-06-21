//! Shared test helpers for the `agent-plugin-opencode` integration suite.
//!
//! [`MockTmux`] creates a temp directory containing a fake `tmux`
//! binary and the data files it reads. The script's `list-panes`
//! output is the verbatim string passed to [`MockTmux::with_output`];
//! the script's `capture-pane` output is read from
//! `<dir>/capture/<pane_id>` for each entry in the `capture_pane` map.
//! Splitting the data into per-pane files (rather than embedding
//! everything in the script body) keeps the script free of
//! interpolation pitfalls — no JSON parsing in POSIX sh, no heredoc
//! sentinel collisions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A fake `tmux` binary backed by files in a tempdir.
///
/// The tempdir is deleted when `MockTmux` is dropped. Hold the
/// `MockTmux` for the duration of the test that needs the script to
/// exist on disk.
pub struct MockTmux {
    _dir: tempfile::TempDir,
    bin: PathBuf,
}

impl MockTmux {
    /// Create a `MockTmux` with empty `list-panes` and no
    /// `capture-pane` entries.
    pub fn new() -> Self {
        Self::with_output("", &HashMap::new())
    }

    /// Create a `MockTmux` whose fake `tmux list-panes` outputs
    /// `list_panes` verbatim and whose fake `tmux capture-pane
    /// -t <pane>` outputs the value mapped from `<pane>` in
    /// `capture_pane` (empty string if the pane is not in the map).
    pub fn with_output(list_panes: &str, capture_pane: &HashMap<String, String>) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let capture_subdir = dir.path().join("capture");
        std::fs::create_dir(&capture_subdir).expect("mkdir capture");
        std::fs::write(dir.path().join("list_panes.txt"), list_panes).expect("write list_panes");

        for (pane, content) in capture_pane {
            std::fs::write(capture_subdir.join(pane), content).expect("write capture file");
        }

        let bin = dir.path().join("tmux");
        let script = r#"#!/bin/sh
# Fake tmux binary used by the agent-plugin-opencode integration tests.
DIR="$(cd "$(dirname "$0")" && pwd)"
case "$1" in
  list-panes)
    cat "$DIR/list_panes.txt"
    ;;
  capture-pane)
    PANE=
    while [ $# -gt 0 ]; do
      case "$1" in
        -t) PANE="$2"; shift 2;;
        *) shift;;
      esac
    done
    if [ -f "$DIR/capture/$PANE" ]; then
      cat "$DIR/capture/$PANE"
    fi
    ;;
esac
"#;
        std::fs::write(&bin, script).expect("write tmux script");
        #[cfg(unix)]
        {
            std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .expect("chmod tmux");
        }

        Self { _dir: dir, bin }
    }

    /// Path to the fake `tmux` binary. Pass this to
    /// [`agent_plugin_opencode::discovery::discover_with_tmux`] or
    /// [`agent_plugin_opencode::watcher::capture_pane`].
    pub fn path(&self) -> &Path {
        &self.bin
    }
}

impl Default for MockTmux {
    fn default() -> Self {
        Self::new()
    }
}
