# agentgraph

A terminal view of which features of a project your [herdr](https://herdr.dev) agents are working on.

```
urbandocs · 2 agents (1 working)

 · MCP server       —
 ● Evals            ● claude w3:p2 "Add judge rubric"
 ○ (project root)   ○ claude w3:p6 "Set env vars" (idle)

 edges
   Evals ─▶ MCP server
```

## Usage

```sh
cargo run -- --map examples/urbandocs.agentgraph.toml          # print once
cargo run -- --map examples/urbandocs.agentgraph.toml --watch  # redraw every 2s
```

If you leave out `--map`, agentgraph looks for the nearest `.agentgraph.toml` walking up from the current directory. `--snapshot FILE` reads a saved `herdr api snapshot` instead of the live server.

## Feature map

```toml
[project]
name = "urbandocs"
root = "~/Projects/Ayuntamiento/urbandocs"  # optional; defaults to the map's directory

[[node]]
id = "evals"
label = "Evals"
paths = ["evals/**"]   # globs, relative to root

[[edge]]
from = "evals"
to = "server"
```

## How agents are placed (v0)

- Agents come from `herdr api snapshot`, polled.
- An agent is placed on the first node whose globs cover its **cwd**. Agents in `.claude/worktrees/<name>/` are matched by their path inside the worktree and get a `⎇ name` tag.
- Agents at the project root, or in a directory no node covers, are listed under `(project root)`.

Most agents run from the repo root, so a cwd tells you little. The next step is attributing agents by the files they touch.

## Roadmap

- **v1:** tail each agent's transcript (herdr's `agent_session` id → `~/.claude/projects/<slug>/<id>.jsonl`), map edited/read files to nodes with recency decay; switch to the herdr socket and `events.subscribe`; ratatui view with Enter → `agent.focus`.
- **v2:** ship as a herdr plugin (`herdr-plugin.toml` pane + keybinding); `agentgraph claim <node>` for explicit attribution via a Claude Code hook.
- **v3:** generated feature maps, activity history, non-Claude transcript formats.
