# Classify issues into modules with Jev, without GitHub labels

v1 placed an issue in a module from `module:<id>` labels or from its branch diff, and listed `ready-for-agent` issues as unclaimed. Those conventions tie herdr-map to repos that adopt them, and diffs say nothing until code exists. From v2, each issue a live agent is on is placed only by Jev (`typesafe/jev-1.13` via OpenRouter), using the issue's title and body and the module map's descriptions. No labels and no diffs are used, and only live agents and their issues are shown.

## Considered options

- **Classifier fills gaps, deterministic sources win:** rejected because it still needs labels or code to exist.
- **Blend the classifier with the diff:** rejected as harder to explain, with no eval data to tune the blend.
- **A chat LLM with JSON output:** deferred. Jev returns a calibrated probability per module, has no output to parse, and costs about $0.04 per million input tokens. Its API is alpha, so it stays behind a single collector (`jev.rs`).

## Consequences

- An issue can sit in the wrong module even after code exists. A diff no longer corrects it.
- Without labelled data, `min_confidence` (default 0.5) is a guess until an `eval` command exists.
- Offline with an empty cache, every issue is unplaced. The view still renders, with an error line at the top.
