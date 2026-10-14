# AGENTS.md

## Agent skills

### Issue tracker

Issues live as GitHub issues on `javier-abia/herdr-map` (via the `gh` CLI). External PRs are not a triage surface. See `docs/agents/issue-tracker.md`.

### Triage labels

Default label vocabulary (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout (`CONTEXT.md` + `docs/adr/` at repo root). See `docs/agents/domain.md`.

### herdr API reference

Use the docs bundled with the installed herdr (`herdr --version`), which match it exactly: `herdr --skill` for CLI usage, `herdr api schema --json` (or `--output FILE`) for the socket API schema (agents, `events.subscribe`, plugins).

### Attribution

End PR descriptions, commit messages, issues, and comments with their own content; the `🤖 Generated with Claude Code` footer and the `Co-Authored-By: Claude` trailer go nowhere in this repo.
