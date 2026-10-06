mod git;
mod github;
mod graph;
mod herdr;
mod modulemap;
mod render;
mod world;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use github::GitHub;
use modulemap::ModuleMap;
use world::{AgentFacts, World};

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

    /// Don't query GitHub; agents are then placed by their changes only.
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
    };

    loop {
        world.agents = herdr::fetch_agents(args.snapshot.as_deref())?
            .into_iter()
            .map(|agent| AgentFacts {
                repo: git::repo_facts(&agent.cwd),
                agent,
            })
            .collect();
        let mut warnings = Vec::new();
        if !args.no_github {
            let data = github.data();
            (world.issues, world.prs) = (data.issues, data.prs);
            if let Some(e) = &github.error {
                warnings.push(format!("GitHub unavailable, showing cached issues: {e}"));
            }
        }

        let graph = graph::build(&world);
        let frame = render::render(&world.map, &graph, &warnings, &style);

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
