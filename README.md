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

Needs `herdr`, `git`, an authenticated `gh` (the `repo` permission is enough), `curl` 8.3 or later, and `OPENROUTER_API_KEY` set to an [OpenRouter](https://openrouter.ai) key for the issue classifier.

```sh
export OPENROUTER_API_KEY=sk-or-...
```

Without the key, or when OpenRouter can't be reached, the view still renders: an error line such as `✗ OPENROUTER_API_KEY not set` or `✗ OpenRouter unreachable: …` goes on top, cached classifications are still used, and issues with nothing cached are listed under "Not placed in a module". Nothing about it is fatal, so `--watch` keeps running.

## Module map

```toml
[project]
name = "urbandocs"
root = "~/Projects/Ayuntamiento/urbandocs"  # optional; defaults to the map's directory
min_confidence = 0.5  # optional; the probability an issue's best module needs to place it

[[module]]
id = "search"
label = "Search & read"
description = "Query parsing, ranking, and resolving and reading cited documents."  # optional but recommended; what the classifier reads
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

**Issue → module.** No GitHub labels and no diffs are read. [Jev](https://openrouter.ai/docs/guides/community/jev) (`typesafe/jev-1.13` via OpenRouter) gets the issue's title and body (truncated to about 4k tokens) and one yes/no question per module, built from the module's `label` and `description`, or its `label` and `paths` when there is no description. Its probability of "yes" for each module is independent of the others. The issue goes under its most likely module when that probability reaches `min_confidence`, shown dimmed after the title. Otherwise it goes under "Not placed in a module" with `(best guess: <module> <p>)`. An issue with no classification (no key, or the request failed, with nothing cached) is unplaced with no guess.

**Classification cache.** Classifications are saved in `~/.cache/agentgraph/<project>.json` (`$XDG_CACHE_HOME/agentgraph/` when that is set) and reused until the issue is edited (its `updatedAt` changes), a module's id, `label` or `description` changes, a module is added or removed, or the model changes. Editing `paths` does not reclassify, even for a module without a description. So Jev is called only for new or changed issues, and probabilities stay put between ticks. A corrupt or unreadable cache file counts as empty and is replaced on the next successful classification; delete it to reclassify everything.

**What's shown.** Only live agents and the issues they are on. Issues with no agent are not shown. Sub-issues are grouped under their parent.

Every link records its source and a confidence value: 1.0 for the agent → issue sources, the classifier's probability for issue → module.

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
