//! Plain-text renderer: modules, the issues under them, the agents on those
//! issues. A thin view over `Graph`; all decisions are made in `graph`.

use std::fmt::Write;

use crate::graph::{AgentNode, Graph, IssueNode, LinkSource};
use crate::modulemap::ModuleMap;

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
    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }
    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
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

/// The "busiest" status among some agents, for a module's dot.
fn busiest<'a>(agents: impl Iterator<Item = &'a AgentNode>) -> Option<&'a str> {
    let statuses: Vec<&str> = agents.map(|a| a.agent.agent_status.as_str()).collect();
    ["blocked", "error", "working"]
        .into_iter()
        .find(|s| statuses.contains(s))
        .or_else(|| statuses.first().copied())
}

fn dot(status: Option<&str>, style: &Style) -> String {
    match status {
        Some(s) => style.paint(status_code(s), status_dot(s)),
        None => style.dim("·"),
    }
}

fn agent_chip(a: &AgentNode, style: &Style) -> String {
    let s = &a.agent.agent_status;
    let mut chip = format!(
        "{} {} {}",
        style.paint(status_code(s), status_dot(s)),
        a.agent.agent,
        style.dim(&a.agent.pane_id),
    );
    // The branch is what links an agent to an issue; the worktree folder
    // name only stands in for it on a detached HEAD.
    if let Some(label) = a.branch.as_ref().or(a.worktree.as_ref()) {
        let _ = write!(chip, " {}", style.paint("36", &format!("⎇ {label}")));
    }
    match a.issue.as_ref().map(|l| l.source) {
        Some(LinkSource::TitleMatch) => chip.push_str(&style.dim(" (title match)")),
        Some(LinkSource::PullRequest) => chip.push_str(&style.dim(" (via PR)")),
        _ => {}
    }
    if let Some(title) = a.agent.title.as_deref().filter(|t| !t.is_empty()) {
        let _ = write!(chip, " \"{title}\"");
    }
    if s != "working" {
        chip.push_str(&style.dim(&format!(" ({s})")));
    }
    chip
}

const INDENT: &str = "     ";

fn render_issues(out: &mut String, issues: &[IssueNode], map: &ModuleMap, style: &Style) {
    // Top-level issues first, then sub-issues grouped under their parent.
    let mut order: Vec<&IssueNode> = issues.iter().collect();
    order.sort_by_key(|i| (i.parent.as_ref().map(|p| p.number), i.number));

    let mut current_parent = None;
    for issue in order {
        let mut indent = INDENT.to_string();
        if let Some(parent) = &issue.parent {
            if current_parent != Some(parent.number) {
                current_parent = Some(parent.number);
                let _ = writeln!(
                    out,
                    "{INDENT}{}",
                    style.dim(&format!("#{} {}", parent.number, parent.title))
                );
            }
            indent.push_str("  ");
        }

        let _ = write!(
            out,
            "{indent}{} {}",
            style.bold(&format!("#{}", issue.number)),
            issue.title
        );
        if issue.agents.is_empty() {
            out.push_str(&style.paint(
                "35",
                if issue.ready {
                    "  · unclaimed"
                } else {
                    "  · no agent"
                },
            ));
        }
        out.push('\n');

        for a in &issue.agents {
            let _ = writeln!(out, "{indent}  {}", agent_chip(a, style));
        }

        let mut notes = Vec::new();
        if issue
            .modules
            .first()
            .is_some_and(|l| l.source == LinkSource::Label)
        {
            notes.push("by label".to_string());
        }
        let also: Vec<String> = issue
            .modules
            .iter()
            .skip(1)
            .map(|l| format!("{} {:.0}%", map.modules[l.target].label, l.weight * 100.0))
            .collect();
        if !also.is_empty() {
            notes.push(format!("also: {}", also.join(", ")));
        }
        if !notes.is_empty() {
            let _ = writeln!(out, "{indent}  {}", style.dim(&notes.join(" · ")));
        }
    }
}

pub fn render(map: &ModuleMap, graph: &Graph, warnings: &[String], style: &Style) -> String {
    let mut out = String::new();

    let agents: Vec<&AgentNode> = graph
        .modules
        .iter()
        .flat_map(|m| m.issues.iter().flat_map(|i| &i.agents).chain(&m.agents))
        .chain(graph.unplaced_issues.iter().flat_map(|i| &i.agents))
        .chain(&graph.unlinked_agents)
        .collect();
    let working = agents
        .iter()
        .filter(|a| a.agent.agent_status == "working")
        .count();
    let _ = write!(
        out,
        "{} · {} agent{} ({working} working)",
        style.bold(&map.name),
        agents.len(),
        if agents.len() == 1 { "" } else { "s" },
    );
    if graph.elsewhere > 0 {
        out.push_str(&style.dim(&format!(" · {} elsewhere", graph.elsewhere)));
    }
    out.push_str("\n\n");

    for (module, node) in map.modules.iter().zip(&graph.modules) {
        let status = busiest(
            node.issues
                .iter()
                .flat_map(|i| &i.agents)
                .chain(&node.agents),
        );
        let empty = node.issues.is_empty() && node.agents.is_empty();
        let label = if empty {
            style.dim(&module.label)
        } else {
            style.bold(&module.label)
        };
        let _ = write!(out, " {} {label}", dot(status, style));
        if empty {
            out.push_str(&style.dim("  —"));
        }
        out.push('\n');
        render_issues(&mut out, &node.issues, map, style);
        for a in &node.agents {
            let _ = writeln!(
                out,
                "{INDENT}{} {}",
                agent_chip(a, style),
                style.dim("(no issue)")
            );
        }
    }

    if !graph.unplaced_issues.is_empty() {
        let _ = writeln!(
            out,
            "\n {} {}",
            style.dim("?"),
            style.bold("Not placed in a module")
        );
        render_issues(&mut out, &graph.unplaced_issues, map, style);
    }
    if !graph.unlinked_agents.is_empty() {
        let _ = writeln!(
            out,
            "\n {} {}",
            style.dim("?"),
            style.bold("No issue, no changes")
        );
        for a in &graph.unlinked_agents {
            let _ = writeln!(out, "{INDENT}{}", agent_chip(a, style));
        }
    }

    if !map.edges.is_empty() {
        let _ = writeln!(out, "\n {}", style.dim("depends on"));
        // Edge endpoints are validated against module ids when the map loads.
        let label = |id: &str| -> &str {
            map.modules
                .iter()
                .find(|m| m.id == id)
                .map_or("?", |m| m.label.as_str())
        };
        for e in &map.edges {
            let _ = writeln!(out, "   {} ─▶ {}", label(&e.from), label(&e.to));
        }
    }

    for w in warnings {
        let _ = writeln!(out, "\n {}", style.paint("31", &format!("! {w}")));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::Agent;
    use std::path::PathBuf;

    fn node(branch: Option<&str>, worktree: Option<&str>) -> AgentNode {
        AgentNode {
            agent: Agent {
                agent: "claude".into(),
                agent_status: "working".into(),
                pane_id: "w1:p1".into(),
                cwd: PathBuf::from("/repo"),
                title: None,
            },
            branch: branch.map(Into::into),
            worktree: worktree.map(Into::into),
            issue: None,
        }
    }

    #[test]
    fn chip_shows_branch_over_worktree_folder() {
        let chip = agent_chip(
            &node(Some("tool-schema"), Some("srv")),
            &Style { color: false },
        );
        assert!(chip.contains("⎇ tool-schema"), "{chip}");
        assert!(!chip.contains("srv"), "{chip}");
    }

    #[test]
    fn chip_falls_back_to_worktree_on_detached_head() {
        let chip = agent_chip(&node(None, Some("srv")), &Style { color: false });
        assert!(chip.contains("⎇ srv"), "{chip}");
    }
}
