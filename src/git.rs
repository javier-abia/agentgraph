//! Collects git facts for an agent's working directory by shelling out to git.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::world::{FileChange, RepoFacts};

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

    // Compare against the fork point from the default branch so the diff
    // is "what this branch changed", uncommitted edits included.
    let base = default
        .as_deref()
        .and_then(|d| git(cwd, &["merge-base", "HEAD", d]))
        .unwrap_or_else(|| "HEAD".into());
    let mut changes =
        numstat(&git(cwd, &["diff", "--numstat", "--no-renames", &base]).unwrap_or_default());
    if let Some(untracked) = git(cwd, &["ls-files", "--others", "--exclude-standard"]) {
        changes.extend(untracked.lines().map(|p| FileChange {
            path: p.into(),
            lines: 1,
        }));
    }

    Some(RepoFacts {
        git_dir,
        toplevel,
        branch,
        on_default_branch,
        changes,
    })
}

/// The ref to diff against: `origin/HEAD`'s target, else a local main/master.
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

/// Parses `git diff --numstat`: `added<TAB>removed<TAB>path`, `-` for binary.
fn numstat(out: &str) -> Vec<FileChange> {
    out.lines()
        .filter_map(|line| {
            let mut cols = line.splitn(3, '\t');
            let added = cols.next()?.parse::<u32>().unwrap_or(0);
            let removed = cols.next()?.parse::<u32>().unwrap_or(0);
            let path = cols.next()?;
            Some(FileChange {
                path: path.into(),
                lines: (added + removed).max(1),
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numstat_including_binary_files() {
        let changes = numstat("40\t2\tsrc/search.py\n-\t-\tdocs/fig.png\n");
        let got: Vec<(&str, u32)> = changes
            .iter()
            .map(|c| (c.path.to_str().unwrap(), c.lines))
            .collect();
        assert_eq!(got, [("src/search.py", 42), ("docs/fig.png", 1)]);
    }
}
