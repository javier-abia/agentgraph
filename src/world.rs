//! A world snapshot: everything the collectors gathered in one tick, as plain
//! data. The graph is a pure function of this, which is the test seam.

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
}

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
    /// Files changed against the default branch, uncommitted edits included.
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    /// Relative to the worktree root.
    pub path: PathBuf,
    /// Lines added plus removed; at least 1 so binary/new files still count.
    pub lines: u32,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub labels: Vec<String>,
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
