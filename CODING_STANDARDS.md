# Coding standards

Judgement calls for reviewing a diff. Mechanical rules live in `scripts/`; this
file covers what a script can't check.

## Layering

- Data flows one way: collectors -> `World` -> `graph` -> renderer.
- `src/graph.rs` and `src/world.rs` are pure: a `World` snapshot in, a graph out.
  `scripts/check-pure.sh` enforces this (no `std::fs`/`process`/`net`, no
  collector functions; collector *types* like `herdr::Agent` are fine).
- Collectors (`herdr.rs`, `git.rs`, `github.rs`, modulemap loading) are thin
  adapters: run the command or read the file, parse it, return plain data.
  Interpretation belongs in `graph.rs`.
- Collectors degrade gracefully: a missing tool, repo, or token yields `None` or
  an empty list for that source, and the tick still renders everything else.
  Reserve hard errors for problems the user must fix (e.g. bad CLI args).
- The renderer only draws. Every decision (which edges exist, attribution,
  grouping, status, ordering) is made in `graph.rs` and arrives as data.

## Tests

- Each new attribution or linking heuristic ships with a unit test in
  `graph.rs` that builds a `World` by hand and asserts on the resulting graph.
  This is the test seam: no processes, files, or network needed.
- Name the test after the behaviour it pins down, and include the case the
  heuristic must *not* match when one exists.
