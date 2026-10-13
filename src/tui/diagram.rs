//! Draws the graph as a data-flow diagram: one box per module, in the rows
//! `flow::layers` gives, joined by arrows that point the way data flows.
//! Issues with no module and agents with no issue go in a tray of boxes
//! underneath. Pure: the whole diagram is drawn into a buffer as tall as it
//! needs, and the caller shows the part that fits.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Widget};

use super::flow::{MapInfo, layers};
use crate::graph::{AgentNode, Graph, IssueNode};

pub const C_ACCENT: Color = Color::Rgb(125, 211, 252);
pub const C_NUM: Color = Color::Rgb(103, 232, 249);
const C_ACTIVE: Color = Color::Rgb(134, 239, 172);
const C_IDLE: Color = Color::Rgb(75, 85, 99);
const C_EDGE: Color = Color::Rgb(100, 116, 139);
const C_WARN: Color = Color::Rgb(250, 204, 21);
const C_BLOCKED: Color = Color::Rgb(248, 113, 113);
const C_SELECTED_BG: Color = Color::Rgb(30, 58, 74);

pub fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

pub fn status_color(status: &str) -> Color {
    match status {
        "working" => Color::Green,
        "blocked" | "error" => Color::Red,
        "idle" | "done" => Color::Yellow,
        _ => Color::DarkGray,
    }
}

pub fn dot(status: &str) -> Span<'static> {
    let d = match status {
        "working" => "●",
        "blocked" | "error" => "!",
        _ => "○",
    };
    Span::styled(d, Style::new().fg(status_color(status)).bold())
}

/// The "busiest" status among some agents, for a box's title dot.
fn busiest<'a>(statuses: &[&'a str]) -> Option<&'a str> {
    ["blocked", "error", "working"]
        .into_iter()
        .find(|s| statuses.contains(s))
        .or_else(|| statuses.first().copied())
}

/// The branch is what links an agent to an issue; the worktree folder name
/// only stands in for it on a detached HEAD.
pub fn branch(a: &AgentNode) -> &str {
    a.branch.as_deref().or(a.worktree.as_deref()).unwrap_or("")
}

/// One agent in selection order, with where it sits in the diagram.
pub struct Entry<'a> {
    pub agent: &'a AgentNode,
    pub issue: Option<&'a IssueNode>,
    /// The module the issue is placed under (None: unplaced or no issue).
    pub module: Option<usize>,
}

/// Top-level issues first, then sub-issues grouped under their parent.
fn sorted(issues: &[IssueNode]) -> Vec<&IssueNode> {
    let mut v: Vec<&IssueNode> = issues.iter().collect();
    v.sort_by_key(|i| (i.parent.as_ref().map(|p| p.number), i.number));
    v
}

/// Every agent in the order the diagram reads: modules row by row, then
/// unplaced issues, then agents with no issue.
pub fn entries<'a>(map: &MapInfo, g: &'a Graph) -> Vec<Entry<'a>> {
    let mut out = Vec::new();
    for m in layers(map).concat() {
        for issue in sorted(&g.modules[m].issues) {
            out.extend(issue.agents.iter().map(|agent| Entry {
                agent,
                issue: Some(issue),
                module: Some(m),
            }));
        }
    }
    for issue in sorted(&g.unplaced_issues) {
        out.extend(issue.agents.iter().map(|agent| Entry {
            agent,
            issue: Some(issue),
            module: None,
        }));
    }
    out.extend(g.no_issue_agents.iter().map(|agent| Entry {
        agent,
        issue: None,
        module: None,
    }));
    out
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// What goes in a box.
#[derive(Clone, Copy)]
enum Content<'a> {
    Module(usize),
    Unplaced(&'a [IssueNode]),
    NoIssue(&'a [AgentNode]),
}

/// A box in the diagram, and where it goes once placed.
#[derive(Clone)]
struct GBox<'a> {
    content: Content<'a>,
    title: String,
    rect: Rect,
}

impl<'a> GBox<'a> {
    fn new(content: Content<'a>, title: String) -> Self {
        Self {
            content,
            title,
            rect: Rect::default(),
        }
    }

    fn module(&self) -> Option<usize> {
        match self.content {
            Content::Module(m) => Some(m),
            _ => None,
        }
    }
}

/// What is selected, as the diagram needs it.
struct Selection<'a> {
    pane: &'a str,
    module: Option<usize>,
    has_issue: bool,
}

fn chip(a: &AgentNode, selected: bool, with_branch: bool) -> Line<'static> {
    let s = &a.agent.agent_status;
    let mut spans = vec![
        Span::from("  "),
        dot(s),
        Span::from(format!(" {} ", a.agent.agent)),
        Span::styled(a.agent.pane_id.clone(), dim()),
    ];
    if with_branch {
        spans.push(Span::styled(
            format!(" ⎇ {}", branch(a)),
            Style::new().fg(C_NUM),
        ));
    }
    if s != "working" {
        spans.push(Span::styled(
            format!(" {s}"),
            Style::new().fg(status_color(s)),
        ));
    }
    let line = Line::from(spans);
    if selected {
        line.style(Style::new().bg(C_SELECTED_BG).bold())
    } else {
        line
    }
}

fn issue_lines(
    map: &MapInfo,
    issues: &[IssueNode],
    inner: usize,
    best_guess: bool,
    sel: Option<&str>,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut current_parent = None;
    for i in sorted(issues) {
        // Sub-issues are indented under a header for their parent.
        let indent = match &i.parent {
            Some(p) => {
                if current_parent != Some(p.number) {
                    current_parent = Some(p.number);
                    out.push(Line::styled(
                        truncate(&format!("#{} {}", p.number, p.title), inner),
                        dim(),
                    ));
                }
                "  "
            }
            None => "",
        };
        let num = format!("{indent}#{}", i.number);
        let prob = match (&i.module, best_guess) {
            (Some(m), false) => format!("  {:.2}", m.confidence),
            _ => String::new(),
        };
        let room = inner.saturating_sub(num.len() + 1 + prob.len());
        out.push(Line::from(vec![
            Span::styled(num, Style::new().fg(C_NUM).bold()),
            Span::from(format!(" {}", truncate(&i.title, room))),
            Span::styled(prob, dim()),
        ]));
        if best_guess {
            let guess = match &i.module {
                Some(m) => format!("  best guess {} {:.2}", map.labels[m.target], m.confidence),
                None => "  unclassified".into(),
            };
            out.push(Line::styled(truncate(&guess, inner), dim().italic()));
        }
        for a in &i.agents {
            out.push(chip(a, sel == Some(a.agent.pane_id.as_str()), false));
        }
    }
    out
}

/// A box's lines at a given inner width.
fn box_lines(
    map: &MapInfo,
    g: &Graph,
    content: &Content,
    inner: usize,
    sel: Option<&str>,
) -> Vec<Line<'static>> {
    match content {
        Content::Module(m) if g.modules[*m].issues.is_empty() => {
            vec![Line::styled("no agents", Style::new().fg(C_IDLE)).centered()]
        }
        Content::Module(m) => issue_lines(map, &g.modules[*m].issues, inner, false, sel),
        Content::Unplaced(issues) => issue_lines(map, issues, inner, true, sel),
        Content::NoIssue(agents) => agents
            .iter()
            .map(|a| chip(a, sel == Some(a.agent.pane_id.as_str()), true))
            .collect(),
    }
}

fn agents_in<'a>(g: &'a Graph, content: &Content<'a>) -> Vec<&'a AgentNode> {
    match content {
        Content::Module(m) => g.modules[*m]
            .issues
            .iter()
            .flat_map(|i| &i.agents)
            .collect(),
        Content::Unplaced(issues) => issues.iter().flat_map(|i| &i.agents).collect(),
        Content::NoIssue(agents) => agents.iter().collect(),
    }
}

const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// Arrow cells as connection bits, so where arrows join or cross they get
/// the right box-drawing character.
#[derive(Default)]
struct Wires {
    cells: HashMap<(u16, u16), (u8, bool)>,
    heads: Vec<(u16, u16, bool)>,
}

impl Wires {
    fn bit(&mut self, x: u16, y: u16, b: u8, hot: bool) {
        let c = self.cells.entry((x, y)).or_default();
        c.0 |= b;
        c.1 |= hot;
    }

    /// A path of horizontal and vertical segments through `pts`.
    fn path(&mut self, pts: &[(u16, u16)], hot: bool) {
        for w in pts.windows(2) {
            let ((x1, y1), (x2, y2)) = (w[0], w[1]);
            if y1 == y2 {
                let (lo, hi) = (x1.min(x2), x1.max(x2));
                for x in lo..=hi {
                    if x > lo {
                        self.bit(x, y1, LEFT, hot);
                    }
                    if x < hi {
                        self.bit(x, y1, RIGHT, hot);
                    }
                }
            } else {
                let (lo, hi) = (y1.min(y2), y1.max(y2));
                for y in lo..=hi {
                    if y > lo {
                        self.bit(x1, y, UP, hot);
                    }
                    if y < hi {
                        self.bit(x1, y, DOWN, hot);
                    }
                }
            }
        }
    }

    fn draw(&self, buf: &mut Buffer) {
        let color = |hot: bool| if hot { C_ACCENT } else { C_EDGE };
        for (&(x, y), &(b, hot)) in &self.cells {
            // A lone end (UP alone, LEFT alone…) draws as a straight line.
            let sym = match b {
                UP | DOWN => "│",
                b if b == UP | DOWN => "│",
                LEFT | RIGHT => "─",
                b if b == LEFT | RIGHT => "─",
                b if b == DOWN | RIGHT => "╭",
                b if b == DOWN | LEFT => "╮",
                b if b == UP | RIGHT => "╰",
                b if b == UP | LEFT => "╯",
                b if b == UP | DOWN | RIGHT => "├",
                b if b == UP | DOWN | LEFT => "┤",
                b if b == DOWN | LEFT | RIGHT => "┬",
                b if b == UP | LEFT | RIGHT => "┴",
                _ => "┼",
            };
            if buf.area.contains(Position { x, y }) {
                buf[(x, y)].set_symbol(sym).set_fg(color(hot));
            }
        }
        for &(x, y, hot) in &self.heads {
            if buf.area.contains(Position { x, y }) {
                buf[(x, y)].set_symbol("▼").set_fg(color(hot));
            }
        }
    }
}

const GAP_X: u16 = 4;
const GAP_Y: u16 = 2;
const MIN_BOX: u16 = 22;
const MAX_BOX: u16 = 50;

/// The whole diagram, and where the selected agent's box is in it.
pub struct Diagram {
    pub buf: Buffer,
    pub selected: Option<Rect>,
}

/// Draws the diagram `width` columns wide, with `sel` highlighted.
pub fn draw(map: &MapInfo, g: &Graph, sel: Option<&Entry>, width: u16) -> Diagram {
    let sel = sel.map(|e| Selection {
        pane: &e.agent.agent.pane_id,
        module: e.module,
        has_issue: e.issue.is_some(),
    });
    let sel_pane = sel.as_ref().map(|s| s.pane);
    let sel_module = sel.as_ref().and_then(|s| s.module);

    // Rows of boxes: the data-flow layers, then a tray for what has no module.
    let layer_rows: Vec<Vec<GBox>> = layers(map)
        .into_iter()
        .map(|layer| {
            layer
                .into_iter()
                .map(|m| GBox::new(Content::Module(m), map.labels[m].to_uppercase()))
                .collect()
        })
        .collect();
    let mut tray = Vec::new();
    if !g.unplaced_issues.is_empty() {
        tray.push(GBox::new(
            Content::Unplaced(&g.unplaced_issues),
            "NOT PLACED".into(),
        ));
    }
    if !g.no_issue_agents.is_empty() {
        tray.push(GBox::new(
            Content::NoIssue(&g.no_issue_agents),
            "NO ISSUE".into(),
        ));
    }

    // A row with more boxes than fit side by side carries on below, and
    // arrows that skip a row run down a channel of their own on the left.
    // The channels take width, which can wrap more rows: repeat until the
    // margin settles. It only grows, and is capped, so this ends.
    let max_margin = width / 4;
    let mut margin = 0;
    let (mut rows, tray_row, long) = loop {
        let fit = ((width.saturating_sub(margin) + GAP_X) / (MIN_BOX + GAP_X)).max(1) as usize;
        let mut rows: Vec<Vec<GBox>> = layer_rows
            .iter()
            .flat_map(|row| row.chunks(fit).map(<[GBox]>::to_vec))
            .collect();
        let tray_row = (!tray.is_empty()).then_some(rows.len());
        rows.extend(tray.chunks(fit).map(<[GBox]>::to_vec));

        let row_of: HashMap<usize, usize> = rows
            .iter()
            .enumerate()
            .flat_map(|(ri, row)| row.iter().filter_map(move |b| Some((b.module()?, ri))))
            .collect();
        let long: Vec<(usize, usize)> = map
            .flow
            .iter()
            .copied()
            .filter(|(a, b)| {
                matches!((row_of.get(a), row_of.get(b)), (Some(ra), Some(rb)) if *rb > ra + 1)
            })
            .collect();
        let wanted = (2 * long.len() as u16).min(max_margin);
        if wanted <= margin {
            break (rows, tray_row, long);
        }
        margin = wanted;
    };
    // More long arrows than channels share them.
    let channels = usize::from(margin / 2).max(1);

    // Size and place the boxes: natural width, squeezed to fit the row,
    // each row centred.
    let avail = width.saturating_sub(margin);
    let mut y: u16 = 0;
    for (ri, row) in rows.iter_mut().enumerate() {
        if Some(ri) == tray_row {
            // The tray has no arrows into it, so it needs no gap for them.
            y = y.saturating_sub(GAP_Y) + 1;
        }
        let k = row.len() as u16;
        let gaps = GAP_X * (k - 1);
        let mut widths: Vec<u16> = row
            .iter()
            .map(|b| {
                let natural = box_lines(map, g, &b.content, usize::MAX, sel_pane)
                    .iter()
                    .map(Line::width)
                    .max()
                    .unwrap_or(0) as u16;
                (natural + 4)
                    .max(b.title.chars().count() as u16 + 8)
                    .clamp(MIN_BOX, MAX_BOX)
            })
            .collect();
        if widths.iter().sum::<u16>() + gaps > avail {
            let w = (avail.saturating_sub(gaps) / k).max(5);
            widths.iter_mut().for_each(|x| *x = w);
        }
        let total = widths.iter().sum::<u16>() + gaps;
        let mut x = margin + avail.saturating_sub(total) / 2;
        let mut tallest = 0;
        for (b, w) in row.iter_mut().zip(widths) {
            let lines = box_lines(map, g, &b.content, (w - 4) as usize, sel_pane);
            let h = lines.len() as u16 + 2;
            b.rect = Rect::new(x, y, w, h);
            x += w + GAP_X;
            tallest = tallest.max(h);
        }
        y += tallest + GAP_Y;
    }
    let height = y.saturating_sub(GAP_Y);
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height));

    let rects: HashMap<usize, Rect> = rows
        .iter()
        .flatten()
        .filter_map(|b| Some((b.module()?, b.rect)))
        .collect();
    let row_bottom = |r: &Rect| {
        rows.iter()
            .flatten()
            .filter(|b| b.rect.y == r.y)
            .map(|b| b.rect.bottom())
            .max()
            .unwrap_or(r.bottom())
    };

    // Arrows first; boxes are drawn over them.
    let mut wires = Wires::default();
    let mut joints = Vec::new();
    for &(up, down) in &map.flow {
        let (Some(ra), Some(rb)) = (rects.get(&up), rects.get(&down)) else {
            continue;
        };
        if rb.y <= ra.bottom() {
            continue; // same row, or back up a cycle: not drawn
        }
        let hot = sel_module == Some(up) || sel_module == Some(down);
        let xa = ra.x + ra.width / 2;
        // Straight down when the upstream's centre is already over the target.
        let xb = if (rb.x + 2..rb.right().saturating_sub(2)).contains(&xa) {
            xa
        } else {
            rb.x + rb.width / 2
        };
        let g0 = row_bottom(ra);
        let (t0, t1) = (rb.y - 2, rb.y - 1);
        let pts = match long.iter().position(|&e| e == (up, down)) {
            None => vec![(xa, ra.bottom()), (xa, g0), (xb, g0), (xb, t1)],
            Some(k) => {
                let cx = 1 + 2 * (k % channels) as u16;
                vec![
                    (xa, ra.bottom()),
                    (xa, g0),
                    (cx, g0),
                    (cx, t0),
                    (xb, t0),
                    (xb, t1),
                ]
            }
        };
        wires.bit(xa, ra.bottom(), UP, hot);
        wires.path(&pts, hot);
        wires.heads.push((xb, t1, hot));
        joints.push((xa, ra.bottom() - 1, hot));
    }
    wires.draw(&mut buf);

    let mut selected = None;
    for b in rows.iter().flatten() {
        let clip = b.rect.intersection(buf.area);
        let agents = agents_in(g, &b.content);
        let statuses: Vec<&str> = agents
            .iter()
            .map(|a| a.agent.agent_status.as_str())
            .collect();
        let busy = busiest(&statuses);
        let is_selected = sel.as_ref().is_some_and(|s| match b.content {
            Content::Module(m) => s.module == Some(m),
            Content::Unplaced(_) => s.module.is_none() && s.has_issue,
            Content::NoIssue(_) => !s.has_issue,
        });
        if is_selected {
            selected = Some(b.rect);
        }
        let color = match (&b.content, busy) {
            _ if is_selected => C_ACCENT,
            (_, Some("blocked" | "error")) => C_BLOCKED,
            (Content::Module(_), Some(_)) => C_ACTIVE,
            // Idle: a module no agent is on.
            (Content::Module(_), None) => C_IDLE,
            _ => C_WARN,
        };
        let mut title = vec![Span::from(" ")];
        if let Some(s) = busy {
            title.extend([dot(s), Span::from(" ")]);
        }
        title.push(Span::styled(
            format!("{} ", b.title),
            Style::new().fg(color).bold(),
        ));
        let block = Block::bordered()
            .border_type(if is_selected {
                BorderType::Double
            } else {
                BorderType::Rounded
            })
            .border_style(Style::new().fg(color))
            .title(Line::from(title).centered())
            .padding(Padding::horizontal(1));
        let lines = box_lines(
            map,
            g,
            &b.content,
            b.rect.width.saturating_sub(4) as usize,
            sel_pane,
        );
        Paragraph::new(lines).block(block).render(clip, &mut buf);
    }
    // Where an arrow leaves a box, notch its bottom border.
    for (x, y, hot) in joints {
        if buf.area.contains(Position { x, y }) {
            let cell = &mut buf[(x, y)];
            let sym = match cell.symbol() {
                "─" => "┬",
                "═" => "╤",
                _ => continue,
            };
            let fg = if hot { C_ACCENT } else { cell.fg };
            cell.set_symbol(sym).set_fg(fg);
        }
    }

    Diagram { buf, selected }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::graph::{Link, LinkSource, ModuleNode};
    use crate::herdr::Agent;
    use crate::modulemap::ModuleMap;

    pub fn map(toml: &str) -> MapInfo {
        MapInfo::from(&ModuleMap::parse(toml, Path::new("/repo")).unwrap())
    }

    pub fn agent(pane: &str, status: &str) -> AgentNode {
        AgentNode {
            agent: Agent {
                agent: "claude".into(),
                agent_status: status.into(),
                pane_id: pane.into(),
                cwd: PathBuf::from("/repo"),
                title: None,
            },
            branch: Some("b".into()),
            worktree: None,
            issue: None,
        }
    }

    pub fn issue(number: u64, module: usize, agents: Vec<AgentNode>) -> IssueNode {
        IssueNode {
            number,
            title: format!("Issue {number}"),
            parent: None,
            module: Some(Link {
                target: module,
                source: LinkSource::Classifier,
                confidence: 0.9,
            }),
            agents,
        }
    }

    /// Server depends on search: search's data flows into the server.
    pub const SERVER_READS_SEARCH: &str = "[[module]]\nid = \"server\"\nlabel = \"MCP server\"\n[[module]]\nid = \"search\"\nlabel = \"Search\"\n[[edge]]\nfrom = \"server\"\nto = \"search\"\n";

    /// One agent working on an issue in the server; search idle.
    pub fn busy_server() -> Graph {
        Graph {
            modules: vec![
                ModuleNode {
                    issues: vec![issue(7, 0, vec![agent("p1", "working")])],
                },
                ModuleNode::default(),
            ],
            ..Default::default()
        }
    }

    fn text(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn row_with(lines: &[String], needle: &str) -> usize {
        lines
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not drawn:\n{}", lines.join("\n")))
    }

    /// The cell where `needle` starts.
    fn find(buf: &Buffer, needle: &str) -> (u16, u16) {
        let lines = text(buf);
        let y = row_with(&lines, needle);
        let x = lines[y][..lines[y].find(needle).unwrap()].chars().count();
        (x as u16, y as u16)
    }

    #[test]
    fn the_arrow_points_from_the_dependency_down_into_the_dependent() {
        let d = draw(&map(SERVER_READS_SEARCH), &busy_server(), None, 80);
        let lines = text(&d.buf);
        let search = row_with(&lines, "SEARCH");
        let server = row_with(&lines, "MCP SERVER");
        assert!(search < server, "{}", lines.join("\n"));
        let head = row_with(&lines, "▼");
        assert_eq!(head, server - 1, "{}", lines.join("\n"));
        assert!(!lines.iter().any(|l| l.contains('▲')));
    }

    #[test]
    fn a_module_with_no_agents_is_still_drawn_grayed_out() {
        let d = draw(&map(SERVER_READS_SEARCH), &busy_server(), None, 80);
        let (x, y) = find(&d.buf, "SEARCH");
        assert_eq!(d.buf[(x, y)].fg, C_IDLE);
        let lines = text(&d.buf);
        assert!(row_with(&lines, "no agents") > y as usize);

        let (x, y) = find(&d.buf, "MCP SERVER");
        assert_eq!(d.buf[(x, y)].fg, C_ACTIVE);
    }

    #[test]
    fn every_module_is_drawn_even_when_no_agent_is_anywhere() {
        let m = map(
            "[[module]]\nid = \"a\"\nlabel = \"Alpha\"\n[[module]]\nid = \"b\"\nlabel = \"Beta\"\n[[module]]\nid = \"c\"\nlabel = \"Gamma\"\n[[edge]]\nfrom = \"b\"\nto = \"a\"\n",
        );
        let g = Graph {
            modules: (0..3).map(|_| ModuleNode::default()).collect(),
            ..Default::default()
        };
        let lines = text(&draw(&m, &g, None, 80).buf);
        for label in ["ALPHA", "BETA", "GAMMA"] {
            row_with(&lines, label);
        }
    }

    #[test]
    fn issues_with_no_module_and_agents_with_no_issue_go_in_the_tray() {
        let mut g = busy_server();
        g.unplaced_issues = vec![IssueNode {
            module: None,
            ..issue(9, 0, vec![agent("p2", "idle")])
        }];
        g.no_issue_agents = vec![agent("p3", "working")];
        let lines = text(&draw(&map(SERVER_READS_SEARCH), &g, None, 80).buf);
        let server = row_with(&lines, "MCP SERVER");
        assert!(row_with(&lines, "NOT PLACED") > server);
        assert!(row_with(&lines, "NO ISSUE") > server);
        row_with(&lines, "unclassified");
    }

    #[test]
    fn the_selected_agents_box_is_reported_and_drawn_double() {
        let m = map(SERVER_READS_SEARCH);
        let g = busy_server();
        let es = entries(&m, &g);
        let d = draw(&m, &g, es.first(), 80);
        let r = d.selected.expect("selected box");
        assert_eq!(d.buf[(r.x, r.y)].symbol(), "╔");
        let (_, y) = find(&d.buf, "MCP SERVER");
        assert_eq!(r.y, y);
    }

    #[test]
    fn a_row_too_wide_for_the_screen_wraps_and_every_module_is_drawn() {
        let ids: Vec<String> = (0..10).map(|i| format!("module{i}")).collect();
        let toml: String = ids
            .iter()
            .map(|id| format!("[[module]]\nid = \"{id}\"\n"))
            .collect();
        let g = Graph {
            modules: (0..10).map(|_| ModuleNode::default()).collect(),
            ..Default::default()
        };
        let d = draw(&map(&toml), &g, None, 80);
        let lines = text(&d.buf);
        for id in &ids {
            row_with(&lines, &format!(" {} ", id.to_uppercase()));
        }
    }

    #[test]
    fn sub_issues_sit_under_their_parent() {
        let mut child = issue(91, 0, vec![agent("p1", "working")]);
        child.parent = Some(crate::world::IssueRef {
            number: 86,
            title: "Cut token use".into(),
        });
        let mut g = busy_server();
        g.modules[0].issues.push(child);
        let lines = text(&draw(&map(SERVER_READS_SEARCH), &g, None, 80).buf);
        let parent = row_with(&lines, "#86 Cut token use");
        assert!(row_with(&lines, "#91 Issue 91") > parent);
        assert!(row_with(&lines, "#7 Issue 7") < parent);
    }

    #[test]
    fn a_narrow_screen_squeezes_boxes_without_panicking() {
        let m = map(
            "[[module]]\nid = \"a\"\n[[module]]\nid = \"b\"\n[[module]]\nid = \"c\"\n[[edge]]\nfrom = \"b\"\nto = \"a\"\n[[edge]]\nfrom = \"c\"\nto = \"a\"\n",
        );
        let g = Graph {
            modules: (0..3).map(|_| ModuleNode::default()).collect(),
            ..Default::default()
        };
        for width in [0, 3, 12, 30] {
            draw(&m, &g, None, width);
        }
    }

    #[test]
    fn selection_order_follows_the_diagram_rows_then_the_tray() {
        let m = map(SERVER_READS_SEARCH);
        let mut g = busy_server();
        g.modules[1].issues = vec![issue(8, 1, vec![agent("p0", "working")])];
        g.no_issue_agents = vec![agent("p9", "idle")];
        let panes: Vec<&str> = entries(&m, &g)
            .iter()
            .map(|e| e.agent.agent.pane_id.as_str())
            .collect();
        // Search is upstream of the server, so its row comes first.
        assert_eq!(panes, ["p0", "p1", "p9"]);
    }
}
