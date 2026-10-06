//! The module map: the stable parts of the project (modules), the paths that
//! belong to each, and the dependency edges between them. Loaded from
//! `.agentgraph.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::Deserialize;

pub const FILE_NAME: &str = ".agentgraph.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMap {
    #[serde(default)]
    project: RawProject,
    #[serde(default, rename = "module")]
    modules: Vec<RawModule>,
    #[serde(default, rename = "edge")]
    edges: Vec<Edge>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProject {
    name: Option<String>,
    root: Option<String>,
    /// Paths that never pull an issue into a module (READMEs, lockfiles…).
    #[serde(default)]
    ignore: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModule {
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
pub struct Module {
    pub id: String,
    pub label: String,
    globs: GlobSet,
}

#[derive(Debug)]
pub struct ModuleMap {
    pub name: String,
    pub root: PathBuf,
    pub modules: Vec<Module>,
    pub edges: Vec<Edge>,
    ignore: GlobSet,
}

impl ModuleMap {
    /// Parses a map. `map_dir` is the directory the map file lives in; a
    /// relative `project.root` is resolved against it, and it is the root
    /// when none is set.
    pub fn parse(text: &str, map_dir: &Path) -> Result<Self> {
        let raw: RawMap = toml::from_str(text).context("parsing module map TOML")?;

        let root = match raw.project.root {
            Some(r) => map_dir.join(expand_tilde(&r)),
            None => map_dir.to_path_buf(),
        };
        let name = raw.project.name.unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into())
        });

        let mut modules = Vec::with_capacity(raw.modules.len());
        for m in raw.modules {
            let globs = glob_set(&m.paths).with_context(|| format!("module `{}`", m.id))?;
            modules.push(Module {
                label: m.label.unwrap_or_else(|| m.id.clone()),
                id: m.id,
                globs,
            });
        }

        for e in &raw.edges {
            for end in [&e.from, &e.to] {
                if !modules.iter().any(|m| &m.id == end) {
                    bail!("edge {} -> {}: unknown module `{end}`", e.from, e.to);
                }
            }
        }

        Ok(Self {
            name,
            root,
            modules,
            edges: raw.edges,
            ignore: glob_set(&raw.project.ignore).context("project.ignore")?,
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading module map {}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new("."));
        Self::parse(&text, dir).with_context(|| format!("in {}", path.display()))
    }

    /// The module a repo-relative file path belongs to: the first module
    /// whose globs match, unless the path is ignored.
    pub fn module_of(&self, rel_path: &Path) -> Option<usize> {
        if self.ignore.is_match(rel_path) {
            return None;
        }
        self.modules.iter().position(|m| m.globs.is_match(rel_path))
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.modules.iter().position(|m| m.id == id)
    }
}

/// `*` stays within one directory (like .gitignore); `**` crosses them.
fn glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        let glob = GlobBuilder::new(p)
            .literal_separator(true)
            .build()
            .with_context(|| format!("bad glob `{p}`"))?;
        builder.add(glob);
    }
    Ok(builder.build()?)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_does_not_cross_directories() {
        let text =
            "[project]\nignore = [\"*.md\"]\n[[module]]\nid = \"docs\"\npaths = [\"docs/**\"]\n";
        let map = ModuleMap::parse(text, Path::new("/repo")).unwrap();
        assert_eq!(map.module_of(Path::new("README.md")), None);
        assert_eq!(map.module_of(Path::new("docs/research/x.md")), Some(0));
    }

    #[test]
    fn rejects_edges_to_unknown_modules() {
        let text = "[[module]]\nid = \"a\"\n[[edge]]\nfrom = \"a\"\nto = \"nope\"\n";
        assert!(ModuleMap::parse(text, Path::new("/repo")).is_err());
    }
}
