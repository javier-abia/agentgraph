//! The module map: the stable parts of the project (modules), what each is
//! about (description and path globs, which the classifier reads), and the
//! dependency edges between them. Loaded from `.agentgraph.toml`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use globset::GlobBuilder;
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
    min_confidence: Option<f32>,
    /// Removed in v2; only read to reject it with a clear message.
    ignore: Option<toml::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModule {
    id: String,
    label: Option<String>,
    description: Option<String>,
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
    /// What the module is about, in a sentence; what the classifier reads.
    pub description: Option<String>,
    /// Path globs, validated at load. The classifier falls back to them when
    /// there is no description.
    pub paths: Vec<String>,
}

#[derive(Debug)]
pub struct ModuleMap {
    pub name: String,
    pub root: PathBuf,
    /// An issue is placed in its most likely module only when the
    /// classifier's probability for it reaches this.
    pub min_confidence: f32,
    pub modules: Vec<Module>,
    pub edges: Vec<Edge>,
}

impl ModuleMap {
    /// Parses a map. `map_dir` is the directory the map file lives in; a
    /// relative `project.root` is resolved against it, and it is the root
    /// when none is set.
    pub fn parse(text: &str, map_dir: &Path) -> Result<Self> {
        let raw: RawMap = toml::from_str(text).context("parsing module map TOML")?;
        if raw.project.ignore.is_some() {
            bail!(
                "project.ignore was removed: issues are no longer placed by the files they change. Delete it from the map."
            );
        }

        let root = match raw.project.root {
            Some(r) => map_dir.join(expand_tilde(&r)),
            None => map_dir.to_path_buf(),
        };
        let name = raw.project.name.unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project".into())
        });

        let min_confidence = raw.project.min_confidence.unwrap_or(0.5);
        if !(0.0..=1.0).contains(&min_confidence) {
            bail!("project.min_confidence must be between 0 and 1, got {min_confidence}");
        }

        let mut modules = Vec::with_capacity(raw.modules.len());
        for m in raw.modules {
            check_globs(&m.paths).with_context(|| format!("module `{}`", m.id))?;
            modules.push(Module {
                label: m.label.unwrap_or_else(|| m.id.clone()),
                id: m.id,
                description: m.description,
                paths: m.paths,
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
            min_confidence,
            modules,
            edges: raw.edges,
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading module map {}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new("."));
        Self::parse(&text, dir).with_context(|| format!("in {}", path.display()))
    }
}

/// Rejects malformed path globs early. `*` stays within one directory (like
/// .gitignore); `**` crosses them.
fn check_globs(patterns: &[String]) -> Result<()> {
    for p in patterns {
        GlobBuilder::new(p)
            .literal_separator(true)
            .build()
            .with_context(|| format!("bad glob `{p}`"))?;
    }
    Ok(())
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
    fn rejects_the_removed_project_ignore_with_a_clear_message() {
        let text = "[project]\nignore = [\"*.md\"]\n[[module]]\nid = \"docs\"\n";
        let err = ModuleMap::parse(text, Path::new("/repo")).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("project.ignore") && msg.contains("removed"),
            "{msg}"
        );
    }

    #[test]
    fn loads_description_paths_and_min_confidence() {
        let text = r#"
            [project]
            min_confidence = 0.7
            [[module]]
            id = "search"
            description = "Query parsing and ranking."
            paths = ["src/search/**"]
        "#;
        let map = ModuleMap::parse(text, Path::new("/repo")).unwrap();
        assert_eq!(map.min_confidence, 0.7);
        let m = &map.modules[0];
        assert_eq!(m.description.as_deref(), Some("Query parsing and ranking."));
        assert_eq!(m.paths, ["src/search/**"]);
    }

    #[test]
    fn defaults_min_confidence_to_half_and_description_to_none() {
        let map = ModuleMap::parse("[[module]]\nid = \"a\"\n", Path::new("/repo")).unwrap();
        assert_eq!(map.min_confidence, 0.5);
        assert_eq!(map.modules[0].description, None);
        assert!(map.modules[0].paths.is_empty());
    }

    #[test]
    fn rejects_min_confidence_outside_zero_to_one() {
        for bad in ["1.5", "-0.1", "nan"] {
            let text = format!("[project]\nmin_confidence = {bad}\n");
            let err = ModuleMap::parse(&text, Path::new("/repo")).unwrap_err();
            assert!(format!("{err:#}").contains("min_confidence"), "{bad}");
        }
    }

    #[test]
    fn rejects_bad_globs() {
        let text = "[[module]]\nid = \"a\"\npaths = [\"src/[\"]\n";
        assert!(ModuleMap::parse(text, Path::new("/repo")).is_err());
    }

    #[test]
    fn rejects_edges_to_unknown_modules() {
        let text = "[[module]]\nid = \"a\"\n[[edge]]\nfrom = \"a\"\nto = \"nope\"\n";
        assert!(ModuleMap::parse(text, Path::new("/repo")).is_err());
    }
}
