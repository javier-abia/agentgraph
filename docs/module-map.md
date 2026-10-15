# Module map

`.herdr-map.toml` at the project root. The easiest way to write one is to ask your coding agent: "Run `herdr-map --skill` and follow it to write this project's module map." The skill ships with the binary, so it matches your version.

```toml
[project]
name = "agentic-rag"
root = "~/Projects/agentic-rag"  # optional; defaults to the map's directory
min_confidence = 0.5  # optional; the probability an issue's best module needs to place it

[[module]]
id = "retriever"
label = "Retriever"
description = "Turning a query into the top-k chunks: embedding the query, searching the index and reranking the results."  # optional but recommended; what the classifier reads
paths = ["src/rag/retriever.py", "tests/test_retriever.py"]  # include the module's tests

[[edge]]
from = "agent"
to = "retriever"   # agent depends on retriever
```

An edge reads "`from` depends on `to`": data flows from `to` into `from`. Globs are relative to the root and work like `.gitignore` (`*` stays within one directory, `**` crosses directories).

A full example: [`examples/agentic-rag.herdr-map.toml`](../examples/agentic-rag.herdr-map.toml).
