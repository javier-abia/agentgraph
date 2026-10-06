//! The feature map: the user-defined nodes and edges of the project graph,
//! loaded from `.agentgraph.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

pub const FILE_NAME: &str = ".agentgraph.toml";

#[derive(Debug, Deserialize)]
struct RawMap {
    #[serde(default)]
    project: RawProject,
    #[serde(default, rename = "node")]
    nodes: Vec<RawNode>,
    #[serde(default, rename = "edge")]
    edges: Vec<Edge>,
}

#[derive(Debug, Default, Deserialize)]
struct RawProject {
    name: Option<String>,
    root: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawNode {
    id: String,
    label: Option<String>,
    #[serde(default)]
    paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

#[derive(Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    globs: GlobSet,
}

impl Node {
    /// True if `rel_path` (relative to the project root) belongs to this node.
    pub fn matches(&self, rel_path: &Path) -> bool {
        self.globs.is_match(rel_path)
    }
}

#[derive(Debug)]
pub struct FeatureMap {
    pub name: String,
    pub root: PathBuf,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl FeatureMap {
    /// Parses a map. `map_dir` is the directory the map file lives in; a
    /// relative `project.root` is resolved against it, and it is the root
    /// when none is set.
    pub fn parse(text: &str, map_dir: &Path) -> Result<Self> {
        let raw: RawMap = toml::from_str(text).context("parsing feature map TOML")?;

        let root = match raw.project.root {
            Some(r) => map_dir.join(expand_tilde(&r)),
            None => map_dir.to_path_buf(),
        };
        let name = raw.project.name.unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into())
        });

        let mut nodes = Vec::with_capacity(raw.nodes.len());
        for n in raw.nodes {
            let mut builder = GlobSetBuilder::new();
            for p in &n.paths {
                let glob =
                    Glob::new(p).with_context(|| format!("node `{}`: bad glob `{p}`", n.id))?;
                builder.add(glob);
            }
            nodes.push(Node {
                label: n.label.unwrap_or_else(|| n.id.clone()),
                id: n.id,
                globs: builder.build()?,
            });
        }

        for e in &raw.edges {
            for end in [&e.from, &e.to] {
                if !nodes.iter().any(|n| &n.id == end) {
                    bail!("edge {} -> {}: unknown node `{end}`", e.from, e.to);
                }
            }
        }

        Ok(Self {
            name,
            root,
            nodes,
            edges: raw.edges,
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading feature map {}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new("."));
        Self::parse(&text, dir).with_context(|| format!("in {}", path.display()))
    }
}

/// Walks up from `start` looking for `.agentgraph.toml`, like git does for `.git`.
pub fn discover(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(FILE_NAME))
        .find(|candidate| candidate.is_file())
}

fn expand_tilde(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(p),
    }
}
