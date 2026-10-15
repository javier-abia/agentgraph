# Usage

```sh
herdr-map         # interactive view
herdr-map --once  # print once (also what you get when piped)
```

Each module is a box holding its issues and the agents on them, with arrows following the data (for an edge `from = "server", to = "search"` the arrow runs Search ─▶ MCP server). Modules with no agent are grayed out. Issues not placed in a module and agents with no issue sit in boxes underneath.

## Keys

| Key | |
|---|---|
| `↑` `↓` / `k` `j` | Select an agent; the view scrolls to it. |
| `Enter` | Focus the selected agent's pane in herdr. |
| `PgUp` `PgDn` | Scroll a map taller than the screen. |
| `v` | Switch the layout: stacked or chains. |
| `e` | Switch how long arrows are drawn: trunks or stubs. |
| `q` / `Esc` | Quit. |

## Layout

- **Layout.** `stacked` (the default) puts each data-flow layer on a row of its own, so arrows always go down. `chains` lays a run of modules, each the only input of the next, sideways (`├───▶`), wrapping at the screen edge like text; other modules in the same layers share its lines.
- **Long arrows**, which skip a row. `trunks` (the default) runs them down the left margin, one lane per source module branching off to each target. `stubs` leaves them out and names the sources on the target box's first line instead (`◂ ORM models`), lit when either end is selected.

## Flags

| Flag | |
|---|---|
| `--map FILE` | Module map. Default: nearest `.herdr-map.toml` walking up from the cwd. |
| `--root DIR` | Overrides `project.root` from the map. |
| `--once` | Print the map once and exit instead of opening the interactive view. Implied when stdout is not a terminal. |
| `--interval S` | Seconds between refreshes of the interactive view (default 2). |
| `--layout stacked\|chains` | Layout to start the interactive view in (default `stacked`). |
| `--arrows trunks\|stubs` | How the interactive view starts drawing long arrows (default `trunks`). |
| `--github-ttl S` | How long GitHub data is cached (default 60s). |
| `--no-github` | Skip GitHub; every agent is then listed under "No issue". |
| `--snapshot FILE` | Read agents from a saved `herdr api snapshot` instead of herdr. |
| `--demo FILE` | Show the made-up agents and issues in a TOML file instead of reading herdr, GitHub and Jev. |
| `--skill` | Print the agent skill for writing a module map. |

## What a refresh costs

Every refresh reads herdr for the live agents and `git` for each agent's branch. Both are local and free. GitHub issues and PRs are fetched at most once per `--github-ttl` (60s by default), not every refresh.

The classifier is **not** called every refresh. Each issue's classification is cached in `~/.cache/herdr-map/<project>.json` (under `$XDG_CACHE_HOME` when set), and Jev is asked only about an issue that has no cached answer: one an agent has just started on, one edited on GitHub since, or every issue after a module's id, `label` or `description` changes or a module is added or removed. So OpenRouter tokens are spent once per new or changed issue, not once per tick. Editing `paths` does not reclassify. Delete the cache file to reclassify everything.

## Errors

herdr-map reads `HERDR_MAP_OPENROUTER_API_KEY`, not `OPENROUTER_API_KEY`, so you can give it a separate key.

Without a key, or when OpenRouter can't be reached, the view still renders with an error line on top (`✗ HERDR_MAP_OPENROUTER_API_KEY not set`, `✗ OpenRouter unreachable: …`). Cached classifications are still used, and issues with nothing cached go under "Not placed in a module".
