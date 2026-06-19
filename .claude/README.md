# .claude/

Claude Code configuration for this repo.

## Files

- `settings.example.json` — example settings file. Copy to `~/.claude/settings.json` and customize.

## Hooks

No hooks configured in v1. Future hooks (planned):

- PreToolUse: enforce TDD reminder when writing `.rs` files
- PostToolUse: auto-run `cargo fmt` on save
- Stop: summarize the diff for the user

## Notes

Claude Code reads the root `AGENTS.md` for instructions. See also `CLAUDE.md` for Claude-specific notes.
