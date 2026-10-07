# agentgraph

A terminal view of what your [herdr](https://herdr.dev) agents are doing: which **GitHub issue** each one is on, and which **modules** of the project those issues touch.

```
urbandocs · 2 agents (1 working) · 1 elsewhere

 · MCP server  —
 · Scripts  —
 · Evals  —

 ? Not placed in a module
     #107 Prototype delegating search→get to a Codex subagent, verbatim-output-first
       ● codex w2:p1 ⎇ prototype-delegating-search-get-to-a-codex-subag (title match)

 ? No issue
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
| `--no-github` | Skip GitHub; every agent is then listed under "No issue". |
| `--snapshot FILE` | Read agents from a saved `herdr api snapshot` instead of herdr. |

Needs `herdr`, `git`, and an authenticated `gh` (the `repo` permission is enough).

## Module map

```toml
[project]
name = "urbandocs"
root = "~/Projects/Ayuntamiento/urbandocs"  # optional; defaults to the map's directory

[[module]]
id = "search"
label = "Search & read"
paths = ["src/urbandocs/search.py", "tests/unit/test_search.py"]  # include the module's tests

[[edge]]
from = "server"
to = "search"   # server depends on search
```

Globs are relative to the root and work like `.gitignore`: `*` stays within one directory, `**` crosses directories.

`project.ignore` is gone: a map that still has it fails to load with a message asking you to delete it.

## How things get linked

**Project membership.** An agent belongs to the project when its cwd is in a repo with the same git common directory. Worktrees outside the repo folder count.

**Agent → issue**, from the agent's branch (never the default branch), first match wins:
1. An issue number in the branch name: `fix-score-floor-104`, `user/42-add-thing`.
2. A branch name that is the start of an open issue's title in branch form: `prototype-delegating-search-get-to-a-codex-subag` → #107.
3. An open PR from that branch that closes an issue.

No match means the agent is listed under "No issue"; nothing guesses.

**Issue → module.** No GitHub labels and no diffs are read. The v2 classifier will place each issue from its title and body ([#4](https://github.com/javier-abia/agentgraph/issues/4)); until it lands, every shown issue is under "Not placed in a module".

**What's shown.** Only live agents and the issues they are on. Issues with no agent are not shown. Sub-issues are grouped under their parent.

Every link records its source and a confidence value (1.0 for the agent → issue sources).

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
