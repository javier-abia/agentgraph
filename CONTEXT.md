# agentgraph

A terminal view of which GitHub issue each herdr agent is working on, and which modules of the project those issues touch.

## Language

**Module**:
A stable part of the project (MCP server, Search, Ingest…), declared in the module map as an id, a label and path globs (tests included).
_Avoid_: feature (the v0 name; "feature" now means what an issue builds)

**Module map**:
The `.agentgraph.toml` file: `[project]` (name, root, `ignore` globs), `[[module]]` entries, and `[[edge]]` dependencies between modules.
_Avoid_: feature map

**Issue**:
An open GitHub issue: the feature or fix being built. Sub-issues are grouped under their parent.
_Avoid_: task, ticket

**Agent**:
A coding agent running in a herdr pane, with a status (working / idle / blocked), a cwd and a title.
_Avoid_: worker, session

**World**:
The facts collected in one tick: module map, agents, per-agent git facts, GitHub issues and PRs. All I/O happens in collectors that build it; the graph is a pure function of it.
_Avoid_: state, snapshot (a herdr snapshot is only one input)

**Graph**:
Modules → issues → agents, plus unplaced issues, unlinked agents and elsewhere agents.

**Link**:
An edge with a target, a source and a confidence (1.0 for every v1 source). Sources: `branch-number`, `title-match`, `pr`, `label`, `diff`.

**Agent → issue link**:
From the agent's branch (never the default branch), first match wins: issue number in the branch name, then branch name as the start of an open issue's title in branch form, then an open PR from that branch closing an issue.

**Issue → module link**:
First match wins: `module:<id>` labels, then the files the issue's branches changed against the default branch (working tree included, `ignore` globs skipped), weighted by lines changed. The heaviest module places the issue; others are "also".

**Unclaimed**:
An open `ready-for-agent` issue with no agent.

**Unplaced**:
An issue no source could tie to a module.

**Elsewhere**:
An agent whose cwd is outside the project's git common directory. Worktrees outside the repo folder that share it are in the project.
