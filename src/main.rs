mod attribution;
mod featuremap;
mod herdr;
mod render;

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use featuremap::FeatureMap;

/// Shows which features of a project the herdr-managed agents are working on.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Feature map to use. Defaults to the nearest `.agentgraph.toml`
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
            featuremap::discover(&cwd).with_context(|| {
                format!(
                    "no {} found in {} or its parents (pass --map)",
                    featuremap::FILE_NAME,
                    cwd.display()
                )
            })?
        }
    };
    let mut map = FeatureMap::load(&map_path)?;
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

    loop {
        let agents = herdr::fetch_agents(args.snapshot.as_deref())?;
        let state = attribution::attribute(&map, agents);
        let frame = render::render(&map, &state, &style);

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
