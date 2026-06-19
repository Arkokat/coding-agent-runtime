# agentd

Persistent Rust daemon that orchestrates multiple coding-agent sessions (opencode, Claude Code, Codex, Aider) inside tmux. Exposes a tmux-aware status line, a live ratatui dashboard, and a CLI. Single user, single machine.

## Status

v0.1.0 — design complete, implementation in progress.

## Features

- **Status line:** per-pane agent status, refreshed every second
- **Dashboard:** ratatui TUI in tmux popup, live event subscription
- **CLI:** `agentd list`, `new`, `jump`, `rename`, `kill`, `status`, `metrics`, `debug`, `plugin`, `init`, `uninstall`
- **Plugins:** out-of-process binaries, install via `agentd plugin install <name>`
- **Metrics:** Prometheus text + OTLP/JSON exporters (no network endpoint v1)
- **Multiarch:** linux/macos × amd64/aarch64, MIT licensed, 100% paywall-free tests

## Install

```bash
curl -fsSL https://agentd.dev/install.sh | sh
agentd init
agentd plugin install opencode
```

Other channels: `brew install agentd`, `cargo install agentd`.

## Quick start

```bash
# in tmux, press prefix+m
# or: agentd new ~/projects/myapp
# or: agentd tui    # open dashboard popup
```

## Documentation

- [Spec](docs/superpowers/specs/2026-06-18-agentd-design.md) — full design
- [Architecture](ARCHITECTURE.md) — high-level
- [Contributing](CONTRIBUTING.md) — how to work on this
- [Workflows](docs/workflows/) — TDD, commit, release

## License

MIT — see [LICENSE](LICENSE).

## Uninstall

```bash
agentd uninstall           # interactive, keeps sessions
agentd uninstall --yes     # non-interactive
```

Removes config, state, cache, runtime dirs, and tmux.conf fragment. Backup of state.db to `~/.local/share/agentd.last-backup/`.
