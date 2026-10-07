mod cache;
mod git;
mod github;
mod graph;
mod herdr;
mod jev;
mod modulemap;
mod render;
mod world;

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use cache::Cache;
use github::GitHub;
use modulemap::ModuleMap;
use world::{AgentFacts, Classification, World};

/// Shows which modules and GitHub issues your herdr-managed agents are working on.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Module map to use. Defaults to the nearest `.agentgraph.toml`
    /// found walking up from the current directory.
    #[arg(long)]
    map: Option<PathBuf>,

    /// Project root; overrides `project.root` from the map.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Redraw continuously instead of printing once.
    #[arg(short, long)]
    watch: bool,

    /// Seconds between redraws in watch mode.
    #[arg(long, default_value_t = 2.0)]
    interval: f64,

    /// Read agents from a saved `herdr api snapshot` JSON file instead of herdr.
    #[arg(long)]
    snapshot: Option<PathBuf>,

    /// Seconds to cache GitHub issues and PRs between refreshes.
    #[arg(long, default_value_t = 60)]
    github_ttl: u64,

    /// Don't query GitHub; agents are then all listed under "No issue".
    #[arg(long)]
    no_github: bool,

    /// Disable ANSI colors (default when stdout is not a terminal).
    #[arg(long)]
    no_color: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let map_path = match args.map {
        Some(p) => p,
        None => {
            let cwd = std::env::current_dir()?;
            modulemap::discover(&cwd).with_context(|| {
                format!(
                    "no {} found in {} or its parents (pass --map)",
                    modulemap::FILE_NAME,
                    cwd.display()
                )
            })?
        }
    };
    let mut map = ModuleMap::load(&map_path)?;
    if let Some(root) = args.root {
        map.root = root;
    }
    // herdr reports absolute, resolved cwds; compare against the same form.
    map.root = map
        .root
        .canonicalize()
        .with_context(|| format!("project root {} does not exist", map.root.display()))?;

    let style = render::Style {
        color: !args.no_color && std::io::stdout().is_terminal(),
    };

    let mut github = GitHub::new(&map.root, Duration::from_secs(args.github_ttl));
    let mut world = World {
        project_git_dir: git::repo_facts(&map.root).map(|r| r.git_dir),
        map,
        agents: Vec::new(),
        issues: Vec::new(),
        prs: Vec::new(),
        classifications: Default::default(),
    };

    let cache_path = cache::path(&world.map.name);
    let mut cache = cache_path.as_deref().map(Cache::load).unwrap_or_default();

    loop {
        world.agents = herdr::fetch_agents(args.snapshot.as_deref())?
            .into_iter()
            .map(|agent| AgentFacts {
                repo: git::repo_facts(&agent.cwd),
                agent,
            })
            .collect();
        let mut errors = Vec::new();
        if !args.no_github {
            let data = github.data();
            (world.issues, world.prs) = (data.issues, data.prs);
            if let Some(e) = &github.error {
                errors.push(format!("GitHub unavailable, showing cached issues: {e}"));
            }
        }

        world.classifications = classify(&world, &mut cache, cache_path.as_deref(), &mut errors);

        let graph = graph::build(&world);
        let frame = render::render(&world.map, &graph, &errors, &style);

        let mut stdout = std::io::stdout().lock();
        if args.watch {
            // Clear screen and home the cursor.
            write!(stdout, "\x1b[2J\x1b[H")?;
        }
        write!(stdout, "{frame}")?;
        stdout.flush()?;

        if !args.watch {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs_f64(args.interval));
    }
}

/// Classifies the issues live agents are on: cached classifications first,
/// Jev only for the misses, and the fresh results saved for the next tick.
fn classify(
    world: &World,
    cache: &mut Cache,
    cache_path: Option<&Path>,
    errors: &mut Vec<String>,
) -> BTreeMap<u64, Classification> {
    let modules = &world.map.modules;
    let mut classifications = BTreeMap::new();
    let mut misses = Vec::new();
    for issue in graph::issues_to_classify(world) {
        let key = cache::key(issue, modules, jev::MODEL);
        match cache.get(issue.number, &key) {
            Some(c) => {
                classifications.insert(issue.number, c.clone());
            }
            None => misses.push((issue, key)),
        }
    }

    let issues: Vec<_> = misses.iter().map(|(issue, _)| *issue).collect();
    let (fresh, jev_error) = jev::classify(&issues, modules);
    errors.extend(jev_error);
    if fresh.is_empty() {
        return classifications;
    }
    for (issue, key) in misses {
        if let Some(c) = fresh.get(&issue.number) {
            cache.insert(issue.number, key, c.clone());
        }
    }
    if let Some(path) = cache_path
        && let Err(e) = cache.save(path)
    {
        errors.push(format!("classification cache not saved: {e:#}"));
    }
    classifications.extend(fresh);
    classifications
}
