//! Collects open issues and PRs from GitHub through `gh`, cached so the
//! fast herdr refresh doesn't hit GitHub's rate limits.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::world::{Issue, IssueRef, PullRequest};

const QUERY: &str = r#"
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    issues(states: OPEN, first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes { number title labels(first: 20) { nodes { name } } parent { number title } }
    }
    pullRequests(states: OPEN, first: 50) {
      nodes { headRefName closingIssuesReferences(first: 10) { nodes { number } } }
    }
  }
}"#;

#[derive(Clone, Default)]
pub struct GitHubData {
    pub issues: Vec<Issue>,
    pub prs: Vec<PullRequest>,
}

pub struct GitHub {
    root: PathBuf,
    ttl: Duration,
    cache: Option<(Instant, GitHubData)>,
    /// Set when the last refresh failed; stale data is served meanwhile.
    pub error: Option<String>,
}

impl GitHub {
    pub fn new(root: &Path, ttl: Duration) -> Self {
        Self {
            root: root.to_path_buf(),
            ttl,
            cache: None,
            error: None,
        }
    }

    pub fn data(&mut self) -> GitHubData {
        let fresh = self
            .cache
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < self.ttl);
        if !fresh {
            match fetch(&self.root) {
                Ok(data) => {
                    self.cache = Some((Instant::now(), data));
                    self.error = None;
                }
                Err(e) => {
                    self.error = Some(format!("{e:#}"));
                    // Back off a full TTL before retrying; keep stale data.
                    let stale = self.cache.take().map(|(_, d)| d).unwrap_or_default();
                    self.cache = Some((Instant::now(), stale));
                }
            }
        }
        self.cache
            .as_ref()
            .map(|(_, d)| d.clone())
            .unwrap_or_default()
    }
}

fn fetch(root: &Path) -> Result<GitHubData> {
    #[derive(Deserialize)]
    struct Repo {
        owner: Login,
        name: String,
    }
    #[derive(Deserialize)]
    struct Login {
        login: String,
    }
    let repo: Repo = serde_json::from_str(&gh(root, &["repo", "view", "--json", "owner,name"])?)
        .context("parsing `gh repo view`")?;

    let out = gh(
        root,
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={QUERY}"),
            "-F",
            &format!("owner={}", repo.owner.login),
            "-F",
            &format!("name={}", repo.name),
        ],
    )?;
    parse(&out)
}

fn parse(json: &str) -> Result<GitHubData> {
    #[derive(Deserialize)]
    struct Resp {
        data: Data,
    }
    #[derive(Deserialize)]
    struct Data {
        repository: Repository,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Repository {
        issues: Nodes<RawIssue>,
        pull_requests: Nodes<RawPr>,
    }
    #[derive(Deserialize)]
    struct Nodes<T> {
        nodes: Vec<T>,
    }
    #[derive(Deserialize)]
    struct RawIssue {
        number: u64,
        title: String,
        labels: Nodes<Name>,
        parent: Option<RawRef>,
    }
    #[derive(Deserialize)]
    struct Name {
        name: String,
    }
    #[derive(Deserialize)]
    struct RawRef {
        number: u64,
        title: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawPr {
        head_ref_name: String,
        closing_issues_references: Nodes<Number>,
    }
    #[derive(Deserialize)]
    struct Number {
        number: u64,
    }

    let repo = serde_json::from_str::<Resp>(json)
        .context("parsing GitHub GraphQL response")?
        .data
        .repository;
    Ok(GitHubData {
        issues: repo
            .issues
            .nodes
            .into_iter()
            .map(|i| Issue {
                number: i.number,
                title: i.title,
                labels: i.labels.nodes.into_iter().map(|l| l.name).collect(),
                parent: i.parent.map(|p| IssueRef {
                    number: p.number,
                    title: p.title,
                }),
            })
            .collect(),
        prs: repo
            .pull_requests
            .nodes
            .into_iter()
            .map(|p| PullRequest {
                head_branch: p.head_ref_name,
                closes: p
                    .closing_issues_references
                    .nodes
                    .into_iter()
                    .map(|n| n.number)
                    .collect(),
            })
            .collect(),
    })
}

fn gh(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("gh")
        .current_dir(cwd)
        .args(args)
        .output()
        .context("running gh (is the GitHub CLI installed?)")?;
    if !out.status.success() {
        bail!(
            "gh {}: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
