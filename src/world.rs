//! A world snapshot: everything the collectors gathered in one tick, as plain
//! data. The graph is a pure function of this, which is the test seam.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::herdr::Agent;
use crate::modulemap::ModuleMap;

pub struct World {
    pub map: ModuleMap,
    /// Git common directory of the project; agents whose repo shares it are
    /// part of the project, wherever their worktree lives.
    pub project_git_dir: Option<PathBuf>,
    pub agents: Vec<AgentFacts>,
    pub issues: Vec<Issue>,
    pub prs: Vec<PullRequest>,
    /// Issue number → its classification. Issues not classified yet (or not
    /// cached, when offline) are absent.
    pub classifications: BTreeMap<u64, Classification>,
}

/// The classifier's answer for one issue: module id → probability (0..=1)
/// that the issue belongs to that module. Each probability is independent;
/// they need not sum to 1.
pub type Classification = BTreeMap<String, f32>;

pub struct AgentFacts {
    pub agent: Agent,
    /// None when the agent's cwd is not inside a git repository.
    pub repo: Option<RepoFacts>,
}

#[derive(Debug, Clone, Default)]
pub struct RepoFacts {
    pub git_dir: PathBuf,
    /// Root of the worktree the agent is in.
    pub toplevel: PathBuf,
    /// None on a detached HEAD.
    pub branch: Option<String>,
    /// True when `branch` is the repo's default branch (e.g. main).
    pub on_default_branch: bool,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    /// Markdown body; the classifier reads it with the title.
    pub body: String,
    /// GitHub's `updatedAt` (RFC 3339), which changes when the issue is edited.
    pub updated_at: String,
    pub parent: Option<IssueRef>,
}

#[derive(Debug, Clone)]
pub struct IssueRef {
    pub number: u64,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub head_branch: String,
    /// Issues the PR closes ("Closes #N").
    pub closes: Vec<u64>,
}
