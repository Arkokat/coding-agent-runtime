# .opencode/

opencode configuration for this repo. See https://opencode.ai for the tool.

## Files

- `config.json` — provider, model, ignore paths, agent routing
- `agents/` — subagent definitions (explore, tdd, reviewer, planner)
- `commands/` — custom slash commands (`/test`, `/lint`, `/bench`)

## Default model

Local Ollama with `qwen2.5-coder:7b`. Override via env var or `/model` command in opencode.

## Subagents

- `explore` — codebase search, returns file:line table, no fixes
- `tdd` — TDD specialist, red-green-refactor
- `reviewer` — code reviewer, severity-tagged findings
- `planner` — implementation planner, ordered task list
