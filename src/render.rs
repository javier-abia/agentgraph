//! Plain-text renderer. v1 replaces this with a ratatui view; keeping the
//! v0 output a pure function of the state makes it easy to snapshot-test.

use std::fmt::Write;

use crate::attribution::{GraphState, Placed};
use crate::featuremap::FeatureMap;

pub struct Style {
    pub color: bool,
}

impl Style {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
}

fn status_code(status: &str) -> &'static str {
    match status {
        "working" => "32",           // green
        "blocked" | "error" => "31", // red
        "idle" | "done" => "33",     // yellow
        _ => "2",                    // dim
    }
}

fn status_dot(status: &str) -> &'static str {
    match status {
        "working" => "●",
        "blocked" | "error" => "!",
        _ => "○",
    }
}

/// The dot for a node: the "busiest" status among its agents.
fn node_status(agents: &[Placed]) -> Option<&str> {
    ["blocked", "error", "working"]
        .into_iter()
        .find(|s| agents.iter().any(|p| p.agent.agent_status == *s))
        .or_else(|| agents.first().map(|p| p.agent.agent_status.as_str()))
}

fn agent_chip(p: &Placed, style: &Style) -> String {
    let a = &p.agent;
    let mut chip = format!(
        "{} {} {}",
        style.paint(status_code(&a.agent_status), status_dot(&a.agent_status)),
        a.agent,
        style.paint("2", &a.pane_id),
    );
    if let Some(wt) = &p.worktree {
        let _ = write!(chip, " {}", style.paint("36", &format!("⎇ {wt}")));
    }
    if let Some(title) = a.title.as_deref().filter(|t| !t.is_empty()) {
        let _ = write!(chip, " \"{title}\"");
    }
    if a.agent_status != "working" {
        let _ = write!(
            chip,
            " {}",
            style.paint("2", &format!("({})", a.agent_status))
        );
    }
    chip
}

pub fn render(map: &FeatureMap, state: &GraphState, style: &Style) -> String {
    let mut out = String::new();

    let agents: Vec<&Placed> = state.project_agents().collect();
    let working = agents
        .iter()
        .filter(|p| p.agent.agent_status == "working")
        .count();
    let _ = write!(
        out,
        "{} · {} agent{} ({working} working)",
        style.paint("1", &map.name),
        agents.len(),
        if agents.len() == 1 { "" } else { "s" },
    );
    if state.elsewhere > 0 {
        let _ = write!(
            out,
            " · {}",
            style.paint("2", &format!("{} elsewhere", state.elsewhere))
        );
    }
    out.push_str("\n\n");

    let root_label = "(project root)";
    let width = map
        .nodes
        .iter()
        .map(|n| n.label.chars().count())
        .chain([root_label.len()])
        .max()
        .unwrap_or(0);

    let rows = map
        .nodes
        .iter()
        .map(|n| n.label.as_str())
        .zip(&state.by_node)
        .chain((!state.unattributed.is_empty()).then_some((root_label, &state.unattributed)));

    for (label, placed) in rows {
        let dot = match node_status(placed) {
            Some(s) => style.paint(status_code(s), status_dot(s)),
            None => style.paint("2", "·"),
        };
        let padded = format!("{label:<width$}");
        let label = if placed.is_empty() {
            style.paint("2", &padded)
        } else {
            style.paint("1", &padded)
        };
        let _ = write!(out, " {dot} {label}  ");

        if placed.is_empty() {
            out.push_str(&style.paint("2", "—"));
            out.push('\n');
        }
        for (i, p) in placed.iter().enumerate() {
            if i > 0 {
                let _ = write!(out, "{:width$}     ", "");
            }
            out.push_str(&agent_chip(p, style));
            out.push('\n');
        }
    }

    if !map.edges.is_empty() {
        let _ = writeln!(out, "\n {}", style.paint("2", "edges"));
        // Edge endpoints are validated against node ids when the map loads.
        let label_of = |id: &str| -> &str {
            map.nodes
                .iter()
                .find(|n| n.id == id)
                .map_or("?", |n| n.label.as_str())
        };
        for e in &map.edges {
            let _ = writeln!(out, "   {} ─▶ {}", label_of(&e.from), label_of(&e.to));
        }
    }

    out
}
