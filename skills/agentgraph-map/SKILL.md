---
name: agentgraph-map
description: Write or revise an agentgraph module map (`.agentgraph.toml`). Use when creating a project's first map, or when adding, splitting, merging or renaming modules.
---

# agentgraph module map

agentgraph shows which GitHub issue each coding agent is working on, under the module of the project that issue belongs to. The module map tells it what the modules are. The issue → module decision is made by a classifier (Jev) that reads **only** the issue's title and body plus each module's `label` and `description`. It asks one yes/no question per module: "Does the work described in the issue belong mainly to `module`?" So the descriptions are the map. Everything else is structure around them.

## Steps

### 1. Survey the project

Read `git ls-files`, the README, any architecture or domain docs (`CONTEXT.md`, `docs/adr/`, `ARCHITECTURE.md`), and the open and recently closed issues (`gh issue list --state all --limit 60`).

Done when you can name the project's stable parts and say which of the listed issues falls in each.

### 2. Choose the modules

A module is a **stable part** of the project: a component that will still exist in a year, which many issues touch over time. Examples: the MCP server, search, ingest, evals. A feature being built right now is an issue, not a module.

- Aim for 4–12 modules. Each one should catch issues regularly. Two parts that always change together are one module.
- Give a part whose work is mostly writing (research notes, docs) its own module when issues target it.
- `id`: short, lowercase, stable. Changing it reclassifies every issue, so pick it once.
- `label`: what a person calls it, as shown in the view.

Done when every issue from step 1 has exactly one module it clearly belongs to.

### 3. Write the descriptions

One or two sentences per module saying **what work belongs there**, in the words issues use. This is what the classifier reads.

- Name the concrete responsibilities and nouns an issue about this module would mention: "ranking", "cites", "docling conversion", "Langfuse runs".
- Draw the line against neighbours wherever an issue could be torn between two modules. Example: "The MCP transport adapter … with no retrieval logic of its own" keeps search issues out of the server.
- Describe responsibilities rather than file paths. Paths go in `paths`.

Editing a description later reclassifies every issue (it costs API calls, and placements can move), so get them right now.

Done when each description, read alone, lets you decide yes/no for every issue from step 1 without hesitating.

### 4. Write the paths

`paths` are globs relative to the project root, working like `.gitignore`: `*` stays within one directory, `**` crosses directories. Include each module's **tests** with the module they exercise. Every source and test file should belong to one module. Docs and config can stay unmapped.

The classifier reads paths only for a module with no description, so paths mostly document the module. Editing them never reclassifies.

Done when every glob matches at least one tracked file. Check each one with `git ls-files ':(glob)<pattern>'`, which uses the same `*` / `**` rules.

### 5. Write the edges

An edge reads "`from` depends on `to`": `to` produces something `from` reads, calls or loads. Data flows from `to` into `from`, and the interactive view draws the arrow that way. For each pair, ask which one consumes the other's output. That one is `from`.

From the urbandocs map:

```toml
[[edge]]
from = "server"     # the server calls search
to = "search"

[[edge]]
from = "substrate"  # substrate loads the corpus ingest writes
to = "ingest"
```

Add an edge only for a real runtime or data dependency. Leave unrelated modules, such as frozen scripts or research notes, with no edges.

Done when every edge names a concrete thing that flows (a call, a file, an index), and following the arrows traces the project's real data path.

### 6. Validate and hand over

Load the map. With no agents and no GitHub, this parses it, checks globs and edge ids, and prints every module:

```sh
agentgraph --map .agentgraph.toml --no-github \
  --snapshot <(echo '{"result":{"snapshot":{"agents":[]}}}')
```

If `agentgraph` is not on PATH, ask the user how they run it.

Then show the user the map with one line per module: the issues from step 1 that it would catch.

## File format

The file lives at the project root as `.agentgraph.toml`.

```toml
[project]
name = "urbandocs"       # optional; defaults to the root folder's name
# root = "~/path/to/repo"  # only when the file lives outside the repo
# min_confidence = 0.5     # probability an issue's best module needs; keep the default

[[module]]
id = "search"
label = "Search & read"
description = "The retrieval operations: sweeping and ranking sections for search, resolving printed cites to addresses, and reading provision text, ancestors, children and pages."
paths = ["src/urbandocs/search.py", "src/urbandocs/resolve.py", "tests/unit/test_search.py"]

[[edge]]
from = "server"
to = "search"
```

Unknown keys fail to load, and so does `project.ignore`, which was removed.
