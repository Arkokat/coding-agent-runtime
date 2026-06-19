# docs/architecture/

Deep-dive architecture docs. Per-component or per-concern. The full design is in `docs/superpowers/specs/2026-06-18-agentd-design.md`; these docs go into more detail on specific topics.

## Planned docs (added in later plans)

- `data-flow.md` — sequence diagrams for hot paths (new session, status update, jump)
- `plugin-sdk.md` — how to write a plugin, trait surface, mock backend
- `status-line.md` — how the per-pane status is generated, performance budget
- `metrics.md` — Prometheus + OTLP exporters, what each metric means
- `tui.md` — event subscription, render loop, snapshot testing
