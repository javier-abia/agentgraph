//! The core: turns a world snapshot into the module → issue → agent graph.
//! Pure — all I/O lives in the collectors.

use std::collections::BTreeMap;
use std::path::Path;

use crate::herdr::Agent;
use crate::world::{AgentFacts, FileChange, Issue, IssueRef, PullRequest, World};

/// Label that marks an issue as available work.
pub const READY_LABEL: &str = "ready-for-agent";
/// Label prefix that pins an issue to a module: `module:<id>`.
pub const MODULE_LABEL_PREFIX: &str = "module:";

/// Where a link came from. v2 adds a classifier source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSource {
    Label,
    BranchNumber,
    TitleMatch,
    PullRequest,
    Diff,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link<T> {
    pub target: T,
    pub source: LinkSource,
    /// How sure we are the link is right; 1.0 for deterministic sources.
    pub confidence: f32,
    /// Share of the work behind this link (e.g. fraction of changed lines).
    pub weight: f32,
}

impl<T> Link<T> {
    fn certain(target: T, source: LinkSource) -> Self {
        Self {
            target,
            source,
            confidence: 1.0,
            weight: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentNode {
    pub agent: Agent,
    pub branch: Option<String>,
    /// Worktree name when the agent is not in the project's main checkout.
    pub worktree: Option<String>,
    pub issue: Option<Link<u64>>,
}

#[derive(Debug, Clone)]
pub struct IssueNode {
    pub number: u64,
    pub title: String,
    pub ready: bool,
    pub parent: Option<IssueRef>,
    pub agents: Vec<AgentNode>,
    /// Modules the issue touches, heaviest first. The issue is drawn under
    /// the first one.
    pub modules: Vec<Link<usize>>,
}

#[derive(Debug, Default)]
pub struct ModuleNode {
    pub issues: Vec<IssueNode>,
    /// Agents with no issue, placed here by their own uncommitted changes.
    pub agents: Vec<AgentNode>,
}

#[derive(Debug, Default)]
pub struct Graph {
    /// Indexed like `ModuleMap::modules`.
    pub modules: Vec<ModuleNode>,
    /// Issues worth showing that no source could tie to a module.
    pub unplaced_issues: Vec<IssueNode>,
    /// Project agents with neither an issue nor changes in any module.
    pub unlinked_agents: Vec<AgentNode>,
    /// Agents working in other projects.
    pub elsewhere: usize,
}

pub fn build(world: &World) -> Graph {
    let map = &world.map;
    let mut graph = Graph {
        modules: (0..map.modules.len())
            .map(|_| ModuleNode::default())
            .collect(),
        ..Default::default()
    };

    // Agents grouped by the issue they are on; the rest kept with their changes.
    let mut by_issue: BTreeMap<u64, Vec<(AgentNode, &[FileChange])>> = BTreeMap::new();
    for facts in &world.agents {
        if !in_project(world, facts) {
            graph.elsewhere += 1;
            continue;
        }
        let repo = facts.repo.as_ref();
        let node = AgentNode {
            agent: facts.agent.clone(),
            branch: repo.and_then(|r| r.branch.clone()),
            worktree: repo
                .filter(|r| r.toplevel != map.root)
                .and_then(|r| r.toplevel.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            issue: repo
                .filter(|r| !r.on_default_branch)
                .and_then(|r| r.branch.as_deref())
                .and_then(|b| issue_for_branch(b, &world.issues, &world.prs)),
        };
        let changes = repo.map_or(&[][..], |r| &r.changes[..]);
        match &node.issue {
            Some(link) => by_issue
                .entry(link.target)
                .or_default()
                .push((node, changes)),
            None => match module_links(world, changes, None).first() {
                Some(primary) => graph.modules[primary.target].agents.push(node),
                None => graph.unlinked_agents.push(node),
            },
        }
    }

    // Issues to show: those with agents, plus unclaimed ready ones.
    for issue in &world.issues {
        let ready = issue.labels.iter().any(|l| l == READY_LABEL);
        let entries = by_issue.remove(&issue.number).unwrap_or_default();
        if entries.is_empty() && !ready {
            continue;
        }
        let changes: Vec<FileChange> = entries
            .iter()
            .flat_map(|(_, c)| c.iter().cloned())
            .collect();
        let node = IssueNode {
            number: issue.number,
            title: issue.title.clone(),
            ready,
            parent: issue.parent.clone(),
            modules: module_links(world, &changes, Some(issue)),
            agents: entries.into_iter().map(|(a, _)| a).collect(),
        };
        match node.modules.first() {
            Some(primary) => graph.modules[primary.target].issues.push(node),
            None => graph.unplaced_issues.push(node),
        }
    }

    graph
}

fn in_project(world: &World, facts: &AgentFacts) -> bool {
    match (&world.project_git_dir, &facts.repo) {
        (Some(project), Some(repo)) => &repo.git_dir == project,
        _ => facts.agent.cwd.starts_with(&world.map.root),
    }
}

/// agent → issue: branch number, then title match, then the branch's PR.
fn issue_for_branch(branch: &str, issues: &[Issue], prs: &[PullRequest]) -> Option<Link<u64>> {
    let known = |n: u64| issues.iter().any(|i| i.number == n);

    if let Some(n) = branch_number(branch).filter(|&n| known(n)) {
        return Some(Link::certain(n, LinkSource::BranchNumber));
    }

    let slug = slugify(branch.rsplit('/').next().unwrap_or(branch));
    // Short slugs like "fix" or "wip" would match too much.
    if slug.len() >= 12
        && let Some(issue) = issues.iter().find(|i| slugify(&i.title).starts_with(&slug))
    {
        return Some(Link::certain(issue.number, LinkSource::TitleMatch));
    }

    prs.iter()
        .filter(|pr| pr.head_branch == branch)
        .flat_map(|pr| &pr.closes)
        .find(|&&n| known(n))
        .map(|&n| Link::certain(n, LinkSource::PullRequest))
}

/// Issue number from a branch name: a trailing `-104`, or a leading `104-`
/// after any `user/` prefix.
fn branch_number(branch: &str) -> Option<u64> {
    let last = branch.rsplit('/').next()?;
    let trailing = last
        .rsplit(['-', '_'])
        .next()
        .filter(|s| s.len() < last.len());
    let leading = last
        .split(['-', '_'])
        .next()
        .filter(|s| s.len() < last.len());
    trailing
        .and_then(|s| s.parse().ok())
        .or_else(|| leading.and_then(|s| s.parse().ok()))
}

fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

/// issue → modules: `module:` labels win; otherwise the changed files,
/// weighted by lines changed. Heaviest first.
fn module_links(world: &World, changes: &[FileChange], issue: Option<&Issue>) -> Vec<Link<usize>> {
    let map = &world.map;

    let labelled: Vec<usize> = issue
        .into_iter()
        .flat_map(|i| &i.labels)
        .filter_map(|l| l.strip_prefix(MODULE_LABEL_PREFIX))
        .filter_map(|id| map.index_of(id))
        .collect();
    if !labelled.is_empty() {
        let weight = 1.0 / labelled.len() as f32;
        return labelled
            .into_iter()
            .map(|m| Link {
                weight,
                ..Link::certain(m, LinkSource::Label)
            })
            .collect();
    }

    let mut lines: BTreeMap<usize, u32> = BTreeMap::new();
    for change in changes {
        if let Some(m) = map.module_of(Path::new(&change.path)) {
            *lines.entry(m).or_default() += change.lines.max(1);
        }
    }
    let total: u32 = lines.values().sum();
    let mut links: Vec<Link<usize>> = lines
        .into_iter()
        .map(|(m, n)| Link {
            weight: n as f32 / total as f32,
            ..Link::certain(m, LinkSource::Diff)
        })
        .collect();
    links.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    links
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::modulemap::ModuleMap;
    use crate::world::RepoFacts;

    // Modelled on urbandocs.
    const MAP: &str = r#"
        [project]
        name = "urbandocs"
        ignore = ["*.md", "**/uv.lock"]

        [[module]]
        id = "server"
        paths = ["src/urbandocs/server.py", "tests/unit/test_server.py"]

        [[module]]
        id = "search"
        paths = ["src/urbandocs/search.py", "tests/unit/test_search.py"]

        [[module]]
        id = "evals"
        paths = ["evals/**"]
    "#;
    const SERVER: usize = 0;
    const SEARCH: usize = 1;
    const EVALS: usize = 2;

    const GIT_DIR: &str = "/home/u/urbandocs/.git";

    fn issue(number: u64, title: &str, labels: &[&str]) -> Issue {
        Issue {
            number,
            title: title.into(),
            labels: labels.iter().map(|l| l.to_string()).collect(),
            parent: None,
        }
    }

    fn agent(pane: &str, cwd: &str) -> Agent {
        Agent {
            agent: "claude".into(),
            agent_status: "working".into(),
            pane_id: pane.into(),
            cwd: cwd.into(),
            title: None,
        }
    }

    fn change(path: &str, lines: u32) -> FileChange {
        FileChange {
            path: path.into(),
            lines,
        }
    }

    /// An agent in a urbandocs worktree on `branch`.
    fn on_branch(pane: &str, toplevel: &str, branch: &str, changes: Vec<FileChange>) -> AgentFacts {
        AgentFacts {
            agent: agent(pane, toplevel),
            repo: Some(RepoFacts {
                git_dir: GIT_DIR.into(),
                toplevel: toplevel.into(),
                branch: Some(branch.into()),
                on_default_branch: branch == "main",
                changes,
            }),
        }
    }

    fn world(agents: Vec<AgentFacts>, issues: Vec<Issue>, prs: Vec<PullRequest>) -> World {
        World {
            map: ModuleMap::parse(MAP, Path::new("/home/u/urbandocs")).unwrap(),
            project_git_dir: Some(GIT_DIR.into()),
            agents,
            issues,
            prs,
        }
    }

    fn panes(agents: &[AgentNode]) -> Vec<&str> {
        agents.iter().map(|a| a.agent.pane_id.as_str()).collect()
    }

    fn numbers(issues: &[IssueNode]) -> Vec<u64> {
        issues.iter().map(|i| i.number).collect()
    }

    #[test]
    fn links_agent_by_trailing_branch_number_and_places_issue_by_diff() {
        let g = build(&world(
            vec![on_branch(
                "p1",
                "/home/u/urbandocs",
                "fix-matched-children-score-floor-104",
                vec![change("tests/unit/test_server.py", 12)],
            )],
            vec![issue(
                104,
                "Clear score floor in matched-children search",
                &[],
            )],
            vec![],
        ));
        let node = &g.modules[SERVER].issues[0];
        assert_eq!(node.number, 104);
        assert_eq!(panes(&node.agents), ["p1"]);
        assert_eq!(
            node.agents[0].issue.as_ref().unwrap().source,
            LinkSource::BranchNumber
        );
    }

    #[test]
    fn weights_modules_by_lines_changed_and_skips_ignored_files() {
        // PR #103 → #102: search.py, server.py and their tests.
        let g = build(&world(
            vec![on_branch(
                "p1",
                "/home/u/urbandocs",
                "search-score-floor-102",
                vec![
                    change("src/urbandocs/search.py", 40),
                    change("tests/unit/test_search.py", 30),
                    change("src/urbandocs/server.py", 10),
                    change("tests/unit/test_server.py", 20),
                    change("README.md", 500),
                ],
            )],
            vec![issue(102, "Floor search results at score >= 2", &[])],
            vec![],
        ));
        let node = &g.modules[SEARCH].issues[0];
        let modules: Vec<(usize, f32)> =
            node.modules.iter().map(|l| (l.target, l.weight)).collect();
        assert_eq!(modules, [(SEARCH, 0.7), (SERVER, 0.3)]);
        assert!(
            g.modules[SERVER].issues.is_empty(),
            "drawn only under its primary module"
        );
    }

    #[test]
    fn links_title_named_branch_in_worktree_outside_repo_folder() {
        let g = build(&world(
            vec![on_branch(
                "p1",
                "/home/u/orca/workspaces/urbandocs/prototype-delegating-search-get-to-a-codex-subag",
                "javier-abia/prototype-delegating-search-get-to-a-codex-subag",
                vec![],
            )],
            vec![issue(
                107,
                "Prototype delegating search→get to a Codex subagent, verbatim-output-first",
                &[],
            )],
            vec![],
        ));
        assert_eq!(g.elsewhere, 0);
        let node = &g.unplaced_issues[0];
        assert_eq!(node.number, 107);
        let a = &node.agents[0];
        assert_eq!(a.issue.as_ref().unwrap().source, LinkSource::TitleMatch);
        assert_eq!(
            a.worktree.as_deref(),
            Some("prototype-delegating-search-get-to-a-codex-subag")
        );
    }

    #[test]
    fn falls_back_to_the_branch_pull_request() {
        let g = build(&world(
            vec![on_branch(
                "p1",
                "/home/u/urbandocs",
                "feat/evals-langfuse",
                vec![],
            )],
            vec![issue(94, "Add a grep-over-PDFs baseline agent", &[])],
            vec![PullRequest {
                head_branch: "feat/evals-langfuse".into(),
                closes: vec![94],
            }],
        ));
        let a = &g.unplaced_issues[0].agents[0];
        assert_eq!(a.issue, Some(Link::certain(94, LinkSource::PullRequest)));
    }

    #[test]
    fn module_label_places_unclaimed_ready_issue() {
        let g = build(&world(
            vec![],
            vec![
                issue(
                    63,
                    "Render the fidelity disclosure",
                    &[READY_LABEL, "module:server"],
                ),
                issue(56, "Spec: build the search engine", &[READY_LABEL]),
                issue(110, "Codex prompt cache never hits", &["spike"]),
            ],
            vec![],
        ));
        assert_eq!(numbers(&g.modules[SERVER].issues), [63]);
        assert!(g.modules[SERVER].issues[0].ready);
        assert_eq!(
            numbers(&g.unplaced_issues),
            [56],
            "unready issues without agents are hidden"
        );
    }

    #[test]
    fn module_label_beats_the_diff() {
        let g = build(&world(
            vec![on_branch(
                "p1",
                "/r",
                "x-63",
                vec![change("evals/run.py", 99)],
            )],
            vec![issue(
                63,
                "Render the fidelity disclosure",
                &["module:server"],
            )],
            vec![],
        ));
        assert_eq!(
            g.modules[SERVER].issues[0].modules[0].source,
            LinkSource::Label
        );
    }

    #[test]
    fn agent_on_main_is_placed_by_its_own_changes_or_left_unlinked() {
        let g = build(&world(
            vec![
                on_branch(
                    "p1",
                    "/home/u/urbandocs",
                    "main",
                    vec![change("evals/run.py", 5)],
                ),
                on_branch("p2", "/home/u/urbandocs", "main", vec![]),
            ],
            // A branch number must not link an agent sitting on main.
            vec![issue(1, "Anything", &[])],
            vec![],
        ));
        assert_eq!(panes(&g.modules[EVALS].agents), ["p1"]);
        assert_eq!(panes(&g.unlinked_agents), ["p2"]);
    }

    #[test]
    fn counts_agents_in_other_repos_as_elsewhere() {
        let mut other = on_branch("p1", "/home/u/fintrack", "feat-3", vec![]);
        other.repo.as_mut().unwrap().git_dir = PathBuf::from("/home/u/fintrack/.git");
        let no_repo = AgentFacts {
            agent: agent("p2", "/home/u"),
            repo: None,
        };
        let g = build(&world(vec![other, no_repo], vec![], vec![]));
        assert_eq!(g.elsewhere, 2);
    }

    #[test]
    fn keeps_sub_issue_parent() {
        let mut child = issue(91, "Understand context replay", &[READY_LABEL]);
        child.parent = Some(IssueRef {
            number: 86,
            title: "Cut token use".into(),
        });
        let g = build(&world(vec![], vec![child], vec![]));
        assert_eq!(g.unplaced_issues[0].parent.as_ref().unwrap().number, 86);
    }

    #[test]
    fn parses_branch_numbers() {
        assert_eq!(branch_number("search-cap-benchmark-90"), Some(90));
        assert_eq!(branch_number("user/42-add-thing"), Some(42));
        assert_eq!(branch_number("feat/evals-langfuse"), None);
        assert_eq!(branch_number("123"), None);
    }
}
