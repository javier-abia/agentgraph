#!/usr/bin/env bash
# Enforce the purity rule: src/graph.rs and src/world.rs take a World snapshot
# and return a graph. They must not do I/O or call collector functions.
# Type imports from collectors (e.g. crate::herdr::Agent) are allowed;
# lowercase items (functions/modules, e.g. crate::git::repo_facts) are not.
set -euo pipefail

cd "$(dirname "$0")/.."

files=(src/graph.rs src/world.rs)
collectors='git|github|herdr|modulemap'

patterns=(
  # std I/O modules, as a path or inside a std::{...} group
  'std::(process|fs|net)\b'
  'std::\{[^}]*\b(process|fs|net)\b'
  # process spawning
  '\bCommand\b'
  # collector functions/modules: lowercase item after crate::<collector>::
  "crate::($collectors)::[a-z_]"
  # same, inside a brace group: crate::herdr::{Agent, fetch_agents}
  "crate::($collectors)::\{[^}]*\b[a-z_][a-z0-9_]*\b"
  # importing the collector module itself (then calling herdr::fetch_agents)
  "crate::($collectors)[[:space:]]*(;|as\b)"
)

fail=0
for pat in "${patterns[@]}"; do
  # -n file:line, skip // comment lines
  while IFS= read -r hit; do
    [[ -z "$hit" ]] && continue
    code=${hit#*:*:}
    [[ "$code" =~ ^[[:space:]]*// ]] && continue
    echo "check-pure: ${hit%%:*}:$(cut -d: -f2 <<<"$hit"): forbidden I/O or collector call (/$pat/):" >&2
    echo "    ${code#"${code%%[![:space:]]*}"}" >&2
    fail=1
  done < <(grep -nHE "$pat" "${files[@]}" || true)
done

if (( fail )); then
  echo "check-pure: graph.rs/world.rs must stay pure (World in, Graph out). Move I/O into a collector." >&2
  exit 1
fi
echo "check-pure: ok"
