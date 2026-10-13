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

## Installation

Needs Rust (stable), [herdr](https://herdr.dev), `git`, an authenticated `gh` (the `repo` permission is enough), and `curl` 8.3 or later.

```sh
git clone git@github.com:javier-abia/agentgraph.git
cargo install --path agentgraph
```

To update, `git pull` in the clone and run `cargo install --path .` again.

The issue classifier calls [OpenRouter](https://openrouter.ai), so set your key, for example in your shell profile:

```sh
export OPENROUTER_API_KEY=sk-or-...
```

## Usage

1. **Write a module map.** Each project needs a `.agentgraph.toml` at its root (see [Module map](#module-map)). The easiest way to get one is to have your coding agent write it: tell the agent, "Run `agentgraph --skill` and follow it to write this project's module map." The skill is bundled with the binary, so it matches your installed version's map format. To help agents find it later, for example when asked to split a module, add this line to the project's `AGENTS.md`: "To write or change `.agentgraph.toml`, run `agentgraph --skill`."
2. **Run it** from anywhere inside the project, while your agents run in herdr:

   ```sh
   agentgraph           # print once
   agentgraph --watch   # interactive view, updated every 2s
   ```

**The interactive view** (`--watch`) draws each module as a box holding its issues and the agents on them, joined by arrows that follow the data: for an edge `from = "server", to = "search"` the arrow runs Search ─▶ MCP server. Modules no agent is on are grayed out, not hidden. Issues not placed in a module and agents with no issue sit in boxes underneath.

| Key | |
|---|---|
| `↑` `↓` / `k` `j` | Select an agent; the view scrolls to it. |
| `Enter` | Focus the selected agent's pane in herdr. |
| `PgUp` `PgDn` | Scroll a map taller than the screen. |
| `q` / `Esc` | Quit. |

| Flag | |
|---|---|
| `--map FILE` | Module map. Default: nearest `.agentgraph.toml` walking up from the cwd. |
| `--root DIR` | Overrides `project.root` from the map. |
| `--watch`, `--interval S` | Open the interactive view, updated every S seconds (default 2). Needs a terminal. |
| `--github-ttl S` | How long GitHub data is cached (default 60s). |
| `--no-github` | Skip GitHub; every agent is then listed under "No issue". |
| `--snapshot FILE` | Read agents from a saved `herdr api snapshot` instead of herdr. |
| `--skill` | Print the agent skill for writing a module map. |

Without the key, or when OpenRouter can't be reached, the view still renders: when some agent's issue needs classifying, an error line such as `✗ OPENROUTER_API_KEY not set` or `✗ OpenRouter unreachable: …` goes on top, cached classifications are still used, and issues with nothing cached are listed under "Not placed in a module". Nothing about it is fatal, so `--watch` keeps running.

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

An edge reads "`from` depends on `to`": `to` produces something `from` reads, calls or loads, so data flows from `to` into `from`.

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

Run from the checkout against the example map:

```sh
cargo run -- --map examples/urbandocs.agentgraph.toml
```

## Roadmap

- **v2** ([#4](https://github.com/javier-abia/agentgraph/issues/4)): Jev places each live agent's issue in a module from its title, body and module descriptions, so no GitHub labels are needed. Labels, unclaimed issues and placement by diff are removed. The probability is shown in the graph.
- Later: several modules per issue, unclaimed issues without labels, a classifier for agent → issue, and `agentgraph eval` replaying merged PRs.
- `agentgraph init`: an LLM agent proposes the first module map.
- Transcript reading for files an agent has read but not changed.
- The herdr socket and `events.subscribe`, packaging as a herdr plugin.
