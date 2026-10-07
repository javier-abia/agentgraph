//! Collects branch facts for an agent's working directory by shelling out to
//! git. No diffs: an issue's module comes from the classifier, not its changes.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::world::RepoFacts;

/// None when `cwd` is not inside a git repository.
pub fn repo_facts(cwd: &Path) -> Option<RepoFacts> {
    let dirs = git(
        cwd,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
            "--show-toplevel",
        ],
    )?;
    let mut dirs = dirs.lines().map(|l| canonical(Path::new(l)));
    let git_dir = dirs.next()?;
    let toplevel = dirs.next()?;

    let branch = git(cwd, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let default = default_branch(cwd);
    let on_default_branch = match (&branch, &default) {
        (Some(b), Some(d)) => d.rsplit('/').next() == Some(b.as_str()),
        _ => false,
    };

    Some(RepoFacts {
        git_dir,
        toplevel,
        branch,
        on_default_branch,
    })
}

/// The default branch: `origin/HEAD`'s target, else a local main/master.
fn default_branch(cwd: &Path) -> Option<String> {
    git(
        cwd,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .or_else(|| {
        ["main", "master"]
            .into_iter()
            .find(|b| git(cwd, &["rev-parse", "--verify", "--quiet", b]).is_some())
            .map(String::from)
    })
}

fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}
