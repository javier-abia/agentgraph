---
name: herdr-map
description: Write or revise a herdr-map module map (`.herdr-map.toml`). Use when creating a project's first map, or when adding, splitting, merging or renaming modules.
---

# herdr-map module map

herdr-map shows which GitHub issue each coding agent is working on, under the module of the project that issue belongs to. The module map tells it what the modules are. The issue → module decision is made by a classifier (Jev) that reads **only** the issue's title and body plus each module's `label` and `description`. It asks one yes/no question per module: "Does the work described in the issue belong mainly to `module`?" So the descriptions are the map. Everything else is structure around them.

## Steps

### 1. Survey the project

Read `git ls-files`, the README, and any architecture or domain docs (`CONTEXT.md`, `docs/adr/`, `ARCHITECTURE.md`). The structure comes from the code and docs.

Done when you can name the project's stable parts and say what each one does.

### 2. Choose the modules

A module is a **stable part** of the project: a component that will still exist in a year, which many issues will touch over time. Examples: the MCP server, search, ingest, evals. A feature being built right now is an issue, not a module.

- Aim for 4–12 modules. Two parts that always change together are one module.
- Give a part whose work is mostly writing (research notes, docs) its own module when it is a real target of work.
- `id`: short, lowercase, stable. Changing it reclassifies every issue, so pick it once.
- `label`: what a person calls it, as shown in the view.

Done when every source file from step 1 has exactly one module it clearly belongs to.

### 3. Write the descriptions

One or two sentences per module saying **what work belongs there**, in the words issues use. This is what the classifier reads.

- Name the concrete responsibilities and nouns an issue about this module would mention: "ranking", "cites", "docling conversion", "Langfuse runs".
- Draw the line against neighbours wherever an issue could be torn between two modules. Example: "The MCP transport adapter … with no retrieval logic of its own" keeps search issues out of the server.
- Describe responsibilities rather than file paths. Paths go in `paths`.

Editing a description later reclassifies every issue (it costs API calls, and placements can move), so get them right now.

Then test the descriptions against open issues, if the repo has any (`gh issue list --state open --limit 30 --json number,title,body`). For each issue, ask the classifier's yes/no question of every module, reading only the title, body and descriptions. Reword a description whenever an issue gets a yes from two modules or from none. If several issues fit no module, a stable part may be missing: go back to step 2.

Done when each description, read alone, gives exactly one yes for every open issue without hesitating, or the repo has no open issues.

### 4. Write the paths

`paths` are globs relative to the project root, working like `.gitignore`: `*` stays within one directory, `**` crosses directories. Include each module's **tests** with the module they exercise. Every source and test file should belong to one module. Docs and config can stay unmapped.

The classifier reads paths only for a module with no description, so paths mostly document the module. Editing them never reclassifies.

Done when every glob matches at least one tracked file. Check each one with `git ls-files ':(glob)<pattern>'`, which uses the same `*` / `**` rules.

### 5. Write the edges

An edge reads "`from` depends on `to`": `to` produces something `from` reads, calls or loads. Data flows from `to` into `from`, and the interactive view draws the arrow that way. For each pair, ask which one consumes the other's output. That one is `from`.

From an agentic RAG map:

```toml
[[edge]]
from = "agent"      # the agent calls the retriever
to = "retriever"

[[edge]]
from = "index"      # the index stores the embeddings ingest writes
to = "ingest"
```

Add an edge only for a real runtime or data dependency. Leave unrelated modules, such as frozen scripts or research notes, with no edges.

Done when every edge names a concrete thing that flows (a call, a file, an index), and following the arrows traces the project's real data path.

### 6. Validate and hand over

Load the map. With no agents and no GitHub, this parses it, checks globs and edge ids, and prints every module:

```sh
herdr-map --map .herdr-map.toml --no-github \
  --snapshot <(echo '{"result":{"snapshot":{"agents":[]}}}')
```

If `herdr-map` is not on PATH, ask the user how they run it.

Then show the user the map with one line per module: its label and, when step 3 had open issues, the issues it would catch.

## File format

The file lives at the project root as `.herdr-map.toml`.

```toml
[project]
name = "agentic-rag"     # optional; defaults to the root folder's name
# root = "~/path/to/repo"  # only when the file lives outside the repo
# min_confidence = 0.5     # probability an issue's best module needs; keep the default

[[module]]
id = "retriever"
label = "Retriever"
description = "Turning a query into the top-k chunks: embedding the query, searching the vector index, and reranking the results by relevance."
paths = ["src/rag/retriever.py", "src/rag/rerank.py", "tests/test_retriever.py"]

[[edge]]
from = "agent"
to = "retriever"
```

Unknown keys fail to load, and so does `project.ignore`, which was removed.
