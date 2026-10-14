# agentgraph

A terminal view of what your [herdr](https://herdr.dev) agents are doing: which **GitHub issue** each one is on, and which **module** of the project each issue touches. Modules are drawn as a data-flow diagram, so you can see where agents' work meets.

```
 ◆ agentgraph urbandocs · 4 agents 2 working    updated 1s ago

                                 ╭── PDF CONVERSION ──╮
                                 │      no agents     │
                                 ╰──────────┬─────────╯
                                            │
                                            ▼
                             ╭── INGEST & CORPUS GATES ──╮
                             │         no agents         │
                             ╰─────────────┬─────────────╯
                                           │
                                           ▼
                               ╭── SUBSTRATE LOADING ──╮
                               │       no agents       │
                               ╰───────────┬───────────╯
                                           │
                                           ▼
                   ╔═══════════════ ● SEARCH & READ ════════════════╗
                   ║ #63 Render the fidelity disclosure on r…  0.94 ║
                   ║   ● codex w1:p2                                ║
                   ║ #86 Cut token use in the normativa-sear…  0.83 ║
                   ║   ● claude w1:p1                               ║
                   ╚════════════════════════╤═══════════════════════╝
                                            │
                                            ▼
                                 ╭──── MCP SERVER ────╮
                                 │      no agents     │
                                 ╰──────────┬─────────╯
                 ╭──────────────────────────┤
                 ▼                          ▼
  ╭── DEPLOY & AGENT CLIENTS ──╮    ╭─────────────────── ○ EVALS ────────────────────╮
  │          no agents         │    │ #94 Add a grep-over-PDFs baseline agent…  0.94 │
  ╰────────────────────────────╯    │   ○ claude w2:p1 idle                          │
                                    ╰────────────────────────────────────────────────╯

                ╭── RESEARCH & EVIDENCE ──╮    ╭── DEV TOOLING & CI ──╮
                │        no agents        │    │       no agents      │
                ╰─────────────────────────╯    ╰──────────────────────╯

                            ╭───────── ○ NO ISSUE ─────────╮
                            │   ○ claude w3:p1 ⎇ main idle │
                            ╰──────────────────────────────╯

 ● codex w1:p2  ⎇ 63-render-fidelity-disclosure  ·  #63 Render the fidelity disclosure on returned text

 ↑↓ select   ⏎ focus agent   PgUp/PgDn scroll   q quit
```

## Installation

Needs Rust (stable), [herdr](https://herdr.dev), `git`, an authenticated `gh` (the `repo` permission is enough), and `curl` 8.3 or later.

```sh
git clone git@github.com:javier-abia/agentgraph.git
cargo install --path agentgraph
```

To update, `git pull` in the clone and run `cargo install --path .` again.

Issues are placed in modules by a classifier on [OpenRouter](https://openrouter.ai), so set your key. agentgraph reads its own variable, not `OPENROUTER_API_KEY`, so you can give it a separate key:

```sh
export AGENTGRAPH_OPENROUTER_API_KEY=sk-or-...
```

## Usage

1. **Write a module map**, a `.agentgraph.toml` at the project root (see [Module map](#module-map)). Easiest: tell your coding agent, "Run `agentgraph --skill` and follow it to write this project's module map." The skill ships with the binary, so it matches your version. Adding "To write or change `.agentgraph.toml`, run `agentgraph --skill`." to the project's `AGENTS.md` helps agents find it later.
2. **Run it** from anywhere inside the project while your agents run in herdr:

   ```sh
   agentgraph           # print once
   agentgraph --watch   # interactive view
   ```

In `--watch`, each module is a box holding its issues and the agents on them, with arrows following the data (for an edge `from = "server", to = "search"` the arrow runs Search ─▶ MCP server). Modules with no agent are grayed out. Issues not placed in a module and agents with no issue sit in boxes underneath.

Two settings change how the diagram is laid out:

- **Layout.** `stacked` (the default) puts each data-flow layer on a row of its own, so arrows always go down. `chains` lays a run of modules, each the only input of the next, sideways (`├───▶`), wrapping at the screen edge like text; other modules in the same layers share its lines.
- **Long arrows**, which skip a row. `trunks` (the default) runs them down the left margin, one lane per source module branching off to each target. `stubs` leaves them out and names the sources on the target box's first line instead (`◂ ORM models`), lit when either end is selected.

| Key | |
|---|---|
| `↑` `↓` / `k` `j` | Select an agent; the view scrolls to it. |
| `Enter` | Focus the selected agent's pane in herdr. |
| `PgUp` `PgDn` | Scroll a map taller than the screen. |
| `v` | Switch the layout: stacked or chains. |
| `e` | Switch how long arrows are drawn: trunks or stubs. |
| `q` / `Esc` | Quit. |

| Flag | |
|---|---|
| `--map FILE` | Module map. Default: nearest `.agentgraph.toml` walking up from the cwd. |
| `--root DIR` | Overrides `project.root` from the map. |
| `--watch`, `--interval S` | Open the interactive view, refreshed every S seconds (default 2). Needs a terminal. |
| `--layout stacked\|chains` | Layout to start `--watch` in (default `stacked`). |
| `--arrows trunks\|stubs` | How `--watch` starts drawing long arrows (default `trunks`). |
| `--github-ttl S` | How long GitHub data is cached (default 60s). |
| `--no-github` | Skip GitHub; every agent is then listed under "No issue". |
| `--snapshot FILE` | Read agents from a saved `herdr api snapshot` instead of herdr. |
| `--demo FILE` | Show the made-up agents and issues in a TOML file instead of reading herdr, GitHub and Jev. |
| `--skill` | Print the agent skill for writing a module map. |

### What a refresh costs

Every refresh reads herdr for the live agents and `git` for each agent's branch. Both are local and free. GitHub issues and PRs are fetched at most once per `--github-ttl` (60s by default), not every refresh.

The classifier is **not** called every refresh. Each issue's classification is cached in `~/.cache/agentgraph/<project>.json` (under `$XDG_CACHE_HOME` when set), and Jev is asked only about an issue that has no cached answer: one an agent has just started on, one edited on GitHub since, or every issue after a module's id, `label` or `description` changes or a module is added or removed. So OpenRouter tokens are spent once per new or changed issue, not once per tick. Editing `paths` does not reclassify. Delete the cache file to reclassify everything.

Without a key, or when OpenRouter can't be reached, the view still renders with an error line on top (`✗ AGENTGRAPH_OPENROUTER_API_KEY not set`, `✗ OpenRouter unreachable: …`). Cached classifications are still used, and issues with nothing cached go under "Not placed in a module".

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

An edge reads "`from` depends on `to`": data flows from `to` into `from`. Globs are relative to the root and work like `.gitignore` (`*` stays within one directory, `**` crosses directories).

## How things get linked

**Project.** An agent belongs to the project when its cwd is in a repo with the same git common directory, so worktrees outside the repo folder count.

**Agent → issue**, from the agent's branch (never the default branch), first match wins:
1. An issue number in the branch name: `fix-score-floor-104`, `user/42-add-thing`.
2. A branch name that starts an open issue's title in branch form: `prototype-delegating-search-get-to-a-codex-subag` → #107.
3. An open PR from that branch that closes an issue.

With no match, the agent is listed under "No issue".

**Issue → module.** [Jev](https://openrouter.ai/docs/guides/community/jev) (`typesafe/jev-1.13`) reads the issue's title and body and answers one yes/no question per module, built from its `label` and `description` (or `paths` when there is no description). The issue goes under its most likely module when that probability reaches `min_confidence`, shown after the title. Otherwise it goes under "Not placed in a module" with its best guess. No labels or diffs are read.

Only live agents and the issues they are on are shown. Sub-issues are grouped under their parent.

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

Or with made-up agents and issues, which needs no herdr, GitHub or API key:

```sh
cargo run -- --map examples/urbandocs.agentgraph.toml --demo examples/urbandocs.demo.toml --watch
```

## Roadmap

- Several modules per issue, unclaimed issues, a classifier for agent → issue, and `agentgraph eval` replaying merged PRs.
- `agentgraph init`: an LLM agent proposes the first module map.
- Transcript reading for files an agent has read but not changed.
- The herdr socket and `events.subscribe`, packaging as a herdr plugin.
