//! Decides which feature node each agent is working on.
//!
//! v0 only uses the agent's cwd: an agent sitting inside a node's directory
//! is placed on that node. v1 adds the stronger signal (files the agent
//! touched, read from its transcript) behind the same output type.

use std::path::{Component, Path, PathBuf};

use crate::featuremap::FeatureMap;
use crate::herdr::Agent;

#[derive(Debug, Clone)]
pub struct Placed {
    pub agent: Agent,
    /// Git worktree name when the agent runs in `.claude/worktrees/<name>`.
    pub worktree: Option<String>,
}

#[derive(Debug, Default)]
pub struct GraphState {
    /// Agents per node, indexed like `FeatureMap::nodes`.
    pub by_node: Vec<Vec<Placed>>,
    /// In the project but not inside any node (e.g. at the repo root).
    pub unattributed: Vec<Placed>,
    /// Agents running in other projects.
    pub elsewhere: usize,
}

impl GraphState {
    pub fn project_agents(&self) -> impl Iterator<Item = &Placed> {
        self.by_node.iter().flatten().chain(&self.unattributed)
    }
}

pub fn attribute(map: &FeatureMap, agents: Vec<Agent>) -> GraphState {
    let mut state = GraphState {
        by_node: vec![Vec::new(); map.nodes.len()],
        ..Default::default()
    };

    for agent in agents {
        let Ok(rel) = agent.cwd.strip_prefix(&map.root) else {
            state.elsewhere += 1;
            continue;
        };
        let (worktree, rel_cwd) = split_worktree(rel);
        let node = node_for_dir(map, &rel_cwd);
        let placed = Placed { agent, worktree };
        match node {
            Some(i) => state.by_node[i].push(placed),
            None => state.unattributed.push(placed),
        }
    }
    state
}

/// First node whose globs cover the directory `rel_dir`.
fn node_for_dir(map: &FeatureMap, rel_dir: &Path) -> Option<usize> {
    if rel_dir.as_os_str().is_empty() {
        return None;
    }
    // Globs like `evals/**` match files *inside* a directory, not the
    // directory itself, so probe with a child path too.
    let probe = rel_dir.join("_");
    map.nodes
        .iter()
        .position(|n| n.matches(rel_dir) || n.matches(&probe))
}

/// `.claude/worktrees/<name>/rest` -> (Some(name), rest)
fn split_worktree(rel: &Path) -> (Option<String>, PathBuf) {
    let parts: Vec<Component> = rel.components().collect();
    match parts.as_slice() {
        [a, b, name, rest @ ..] if a.as_os_str() == ".claude" && b.as_os_str() == "worktrees" => {
            let name = name.as_os_str().to_string_lossy().into_owned();
            (Some(name), rest.iter().collect())
        }
        _ => (None, rel.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::parse_snapshot;

    const MAP: &str = r#"
        [project]
        name = "demo"

        [[node]]
        id = "evals"
        paths = ["evals/**"]

        [[node]]
        id = "search"
        paths = ["src/search/**", "src/search.py"]
    "#;

    fn agents() -> Vec<Agent> {
        parse_snapshot(include_str!("../tests/fixtures/snapshot.json")).unwrap()
    }

    fn state() -> GraphState {
        let map = FeatureMap::parse(MAP, Path::new("/repo")).unwrap();
        attribute(&map, agents())
    }

    fn panes(placed: &[Placed]) -> Vec<&str> {
        placed.iter().map(|p| p.agent.pane_id.as_str()).collect()
    }

    #[test]
    fn places_agent_inside_node_directory() {
        let s = state();
        assert_eq!(panes(&s.by_node[0]), ["w1:p2"]);
    }

    #[test]
    fn strips_claude_worktree_prefix() {
        let s = state();
        assert_eq!(panes(&s.by_node[1]), ["w1:p3"]);
        assert_eq!(s.by_node[1][0].worktree.as_deref(), Some("fix-ranking"));
    }

    #[test]
    fn root_cwd_is_unattributed_and_other_projects_are_counted() {
        let s = state();
        assert_eq!(panes(&s.unattributed), ["w1:p1"]);
        assert_eq!(s.elsewhere, 1);
    }

    #[test]
    fn rejects_edges_to_unknown_nodes() {
        let bad = format!("{MAP}\n[[edge]]\nfrom = \"evals\"\nto = \"nope\"\n");
        assert!(FeatureMap::parse(&bad, Path::new("/repo")).is_err());
    }
}
