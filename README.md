# agentgraph

A terminal view of what your [herdr](https://herdr.dev) agents are doing: which **GitHub issue** each one is on, and which **modules** of the project those issues touch.

```
urbandocs · 2 agents (1 working) · 1 elsewhere

 · MCP server  —
 ● Scripts
     #107 Prototype delegating search→get to a Codex subagent, verbatim-output-first
       ● codex w2:p1 ⎇ prototype-delegating-search-get-to-a-codex-subag (title match)
 · Evals  —

 ? Not placed in a module
     #63 Render the fidelity disclosure on returned text  · unclaimed

 ? No issue, no changes
     ○ claude w3:p6 ⎇ main "Fresh start" (idle)

 depends on
   Evals ─▶ MCP server
```

## Usage

```sh
cargo run -- --map examples/urbandocs.agentgraph.toml          # print once
cargo run -- --map examples/urbandocs.agentgraph.toml --watch  # redraw every 2s
```

| Flag | |
|---|---|
| `--map FILE` | Module map. Default: nearest `.agentgraph.toml` walking up from the cwd. |
| `--root DIR` | Overrides `project.root` from the map. |
| `--watch`, `--interval S` | Redraw continuously. |
| `--github-ttl S` | How long GitHub data is cached (default 60s). |
| `--no-github` | Skip GitHub; agents are placed by their changes only. |
| `--snapshot FILE` | Read agents from a saved `herdr api snapshot` instead of herdr. |

Needs `herdr`, `git`, and an authenticated `gh` (the `repo` permission is enough).

## Module map

```toml
[project]
name = "urbandocs"
root = "~/Projects/Ayuntamiento/urbandocs"  # optional; defaults to the map's directory
ignore = ["*.md", "**/uv.lock"]            # changes here never place an issue

[[module]]
id = "search"
label = "Search & read"
paths = ["src/urbandocs/search.py", "tests/unit/test_search.py"]  # include the module's tests

[[edge]]
from = "server"
to = "search"   # server depends on search
```

Globs are relative to the root and work like `.gitignore`: `*` stays within one directory, `**` crosses directories.

## How things get linked

**Project membership.** An agent belongs to the project when its cwd is in a repo with the same git common directory. Worktrees outside the repo folder count.

**Agent → issue**, from the agent's branch (never the default branch), first match wins:
1. An issue number in the branch name: `fix-score-floor-104`, `user/42-add-thing`.
2. A branch name that is the start of an open issue's title in branch form: `prototype-delegating-search-get-to-a-codex-subag` → #107.
3. An open PR from that branch that closes an issue.

**Issue → modules**, first match wins:
1. `module:<id>` labels on the issue.
2. The files its agents' branches changed against the default branch, uncommitted and untracked files included, weighted by lines changed. The issue is drawn under its heaviest module; the others are listed as "also".

**What's shown.** Issues with agents, plus open `ready-for-agent` issues without one (marked unclaimed). Agents without an issue are placed by their own changes, or listed separately. Sub-issues are grouped under their parent.

Every link records its source and a confidence value (1.0 for all v1 sources), ready for a v2 classifier.

## Development

After cloning, enable the versioned pre-commit hook once:

```sh
git config core.hooksPath .githooks
```

It runs the same checks as CI: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `scripts/check-pure.sh`.

## Roadmap

- **v2** ([#4](https://github.com/javier-abia/agentgraph/issues/4)): Jev places each live agent's issue in a module from its title, body and module descriptions, so no GitHub labels are needed. Labels, unclaimed issues and placement by diff are removed. The probability is shown in the graph.
- Later: several modules per issue, unclaimed issues without labels, a classifier for agent → issue, and `agentgraph eval` replaying merged PRs.
- `agentgraph init`: an LLM agent proposes the first module map.
- Transcript reading for files an agent has read but not changed.
- An interactive ratatui view (Enter → `agent.focus`), the herdr socket and `events.subscribe`, packaging as a herdr plugin.
