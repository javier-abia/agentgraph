//! The on-disk classification cache, `~/.cache/agentgraph/<project>.json`.
//! An issue's classification is reused until its key changes: the issue
//! number, its `updatedAt`, a hash of the module ids, labels and descriptions,
//! and the model slug. Path globs are not part of the key.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::modulemap::Module;
use crate::world::{Classification, Issue};

/// Classifications by issue number, each stored with the key it was made
/// under. One entry per issue, so a stale one is overwritten, not piled up.
#[derive(Default, Serialize, Deserialize)]
pub struct Cache {
    entries: BTreeMap<u64, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    key: String,
    classification: Classification,
}

impl Cache {
    /// Reads the cache file. A missing, unreadable or corrupt file is an
    /// empty cache: the issues are classified again and the file rewritten.
    pub fn load(path: &Path) -> Cache {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the cache through a temporary file and a rename, so a reader
    /// never sees half a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let dir = path.parent().context("cache path has no directory")?;
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self)?)
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
    }

    /// The cached classification for `issue` under `key`, if there is one.
    pub fn get(&self, issue: u64, key: &str) -> Option<&Classification> {
        self.entries
            .get(&issue)
            .filter(|e| e.key == key)
            .map(|e| &e.classification)
    }

    pub fn insert(&mut self, issue: u64, key: String, classification: Classification) {
        self.entries.insert(
            issue,
            Entry {
                key,
                classification,
            },
        );
    }
}

/// `$XDG_CACHE_HOME/agentgraph/<project>.json`, else
/// `~/.cache/agentgraph/<project>.json`. None with neither variable set.
pub fn path(project: &str) -> Option<PathBuf> {
    let env = |var| std::env::var_os(var).filter(|v| !v.is_empty());
    let base = env("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    // The project name comes from the map; keep it to one path component.
    let file: String = project
        .chars()
        .map(|c| if c == '/' || c == '\\' { '_' } else { c })
        .collect();
    Some(base.join("agentgraph").join(format!("{file}.json")))
}

/// The cache key for one issue's classification. Pass the slug the request
/// sends, not the dated snapshot a response reports.
pub fn key(issue: &Issue, modules: &[Module], model: &str) -> String {
    // JSON keeps the fields apart, so ("ab", "c") and ("a", "bc") differ.
    let modules: Vec<_> = modules
        .iter()
        .map(|m| (&m.id, &m.label, &m.description))
        .collect();
    let modules = serde_json::to_string(&modules).expect("strings serialize");
    format!(
        "{}|{}|{:016x}|{model}",
        issue.number,
        issue.updated_at,
        fnv1a(modules.as_bytes())
    )
}

/// 64-bit FNV-1a. Unlike `DefaultHasher`, its output is fixed across Rust
/// releases, which a hash stored on disk needs.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: &str, label: &str, description: Option<&str>, paths: &[&str]) -> Module {
        Module {
            id: id.into(),
            label: label.into(),
            description: description.map(Into::into),
            paths: paths.iter().map(|p| p.to_string()).collect(),
        }
    }

    fn modules() -> Vec<Module> {
        vec![
            module(
                "search",
                "Search",
                Some("Query parsing."),
                &["src/search/**"],
            ),
            module("server", "Server", None, &["src/server/**"]),
        ]
    }

    fn issue(number: u64, updated_at: &str) -> Issue {
        Issue {
            number,
            title: "Floor scores".into(),
            body: "Drop hits below 2.".into(),
            updated_at: updated_at.into(),
            parent: None,
        }
    }

    const T0: &str = "2026-10-01T10:00:00Z";
    const MODEL: &str = "typesafe/jev-1.13";

    #[test]
    fn the_key_is_the_same_for_the_same_inputs() {
        assert_eq!(
            key(&issue(102, T0), &modules(), MODEL),
            key(&issue(102, T0), &modules(), MODEL)
        );
    }

    #[test]
    fn the_key_changes_with_the_issue_number_or_its_updated_at() {
        let k = key(&issue(102, T0), &modules(), MODEL);
        assert_ne!(k, key(&issue(103, T0), &modules(), MODEL));
        assert_ne!(
            k,
            key(&issue(102, "2026-10-02T10:00:00Z"), &modules(), MODEL)
        );
    }

    #[test]
    fn the_key_changes_with_a_module_id_label_or_description() {
        let k = key(&issue(102, T0), &modules(), MODEL);
        let edits: [fn(&mut Module); 4] = [
            |m| m.id = "query".into(),
            |m| m.label = "Query".into(),
            |m| m.description = Some("Ranking.".into()),
            |m| m.description = None,
        ];
        for edit in edits {
            let mut ms = modules();
            edit(&mut ms[0]);
            assert_ne!(k, key(&issue(102, T0), &ms, MODEL));
        }
        let mut added = modules();
        added.push(module("ingest", "Ingest", None, &[]));
        assert_ne!(k, key(&issue(102, T0), &added, MODEL), "module added");
    }

    #[test]
    fn the_key_changes_with_the_model_slug() {
        assert_ne!(
            key(&issue(102, T0), &modules(), MODEL),
            key(&issue(102, T0), &modules(), "typesafe/jev-1.14")
        );
    }

    #[test]
    fn the_key_ignores_path_globs() {
        let mut ms = modules();
        ms[0].paths = vec!["lib/search/**".into(), "tests/search/**".into()];
        assert_eq!(
            key(&issue(102, T0), &modules(), MODEL),
            key(&issue(102, T0), &ms, MODEL)
        );
    }

    #[test]
    fn a_stale_entry_misses_and_is_replaced_by_the_new_one() {
        let mut cache = Cache::default();
        let old = key(&issue(102, T0), &modules(), MODEL);
        cache.insert(102, old.clone(), [("search".into(), 0.8)].into());
        assert!(cache.get(102, &old).is_some());

        let new = key(&issue(102, "2026-10-02T10:00:00Z"), &modules(), MODEL);
        assert!(cache.get(102, &new).is_none(), "issue edited");
        cache.insert(102, new.clone(), [("search".into(), 0.3)].into());
        assert_eq!(cache.get(102, &new).unwrap()["search"], 0.3);
        assert!(cache.get(102, &old).is_none(), "one entry per issue");
    }

    #[test]
    fn module_fields_cannot_run_into_each_other() {
        // Same concatenated text, different modules.
        let a = [module("ab", "c", None, &[])];
        let b = [module("a", "bc", None, &[])];
        assert_ne!(
            key(&issue(102, T0), &a, MODEL),
            key(&issue(102, T0), &b, MODEL)
        );
    }
}
