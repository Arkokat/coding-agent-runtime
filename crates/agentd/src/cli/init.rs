use std::path::Path;

/// The text inserted into (and later removed from) `~/.tmux.conf` by
/// `agentd init` / `agentd uninstall`. Marked with sentinel comments
/// so uninstall can find it. Matches spec section 5.
pub fn tmux_conf_fragment() -> String {
    let mut s = String::new();
    s.push_str("# >>> agentd >>>\n");
    s.push_str("set -g status-interval 1\n");
    s.push_str("set -g status-right \"#(agentd status --global)\"\n");
    s.push_str("set -g pane-border-status bottom\n");
    s.push_str("set -g pane-border-format \"#{?pane_active,#[bold],}#(agentd status --pane '#{pane_id}' 2>/dev/null)\"\n");
    s.push_str("# <<< agentd <<<\n");
    s
}

/// Run `tmux -V` and return true if >= 2.6. Re-exported from
/// `crate::tmux` so there is one canonical implementation.
pub use crate::tmux::tmux_version_ok;

/// Write the default `config.toml` and empty `plugins.toml` under
/// `paths.config_dir` if they don't already exist.
pub fn write_default_configs(paths: &crate::paths::Paths) -> std::io::Result<()> {
    paths.ensure()?;
    let cfg = paths.config_dir.join("config.toml");
    if !cfg.exists() {
        std::fs::write(cfg, crate::config::Config::default().default_serialized())?;
    }
    let plg = paths.config_dir.join("plugins.toml");
    if !plg.exists() {
        std::fs::write(plg, "# Add [[plugin]] entries; see docs.\n")?;
    }
    Ok(())
}

/// Check whether `~/.tmux.conf` already contains the agentd fragment.
pub fn tmux_conf_has_fragment(home: &Path) -> bool {
    let path = home.join(".tmux.conf");
    match std::fs::read_to_string(&path) {
        Ok(s) => s.contains("# >>> agentd >>>"),
        Err(_) => false,
    }
}
