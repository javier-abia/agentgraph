//! The module map as the diagram sees it: module labels, and the `[[edge]]`s
//! turned around to point the way data flows, laid out in rows.

use crate::graph;
use crate::modulemap::ModuleMap;

/// What the UI needs from the module map; the map itself stays with the
/// collector thread.
pub struct MapInfo {
    pub name: String,
    pub labels: Vec<String>,
    /// The map's edges as data flow, (upstream, downstream); see
    /// `graph::data_flow`.
    pub flow: Vec<(usize, usize)>,
}

impl From<&ModuleMap> for MapInfo {
    fn from(map: &ModuleMap) -> Self {
        Self {
            name: map.name.clone(),
            labels: map.modules.iter().map(|m| m.label.clone()).collect(),
            flow: graph::data_flow(map),
        }
    }
}

/// Rows of module indices, upstream above downstream. Every module is in
/// exactly one row; modules with no edges share a last row of their own.
///
/// A module's row is the longest data-flow path reaching it, so it sits
/// below every module it reads from. A cycle is broken at the edge that
/// closes it, walking the map in order. Each row is ordered by where the
/// modules feeding it sit in the row above, so arrows cross less.
pub fn layers(map: &MapInfo) -> Vec<Vec<usize>> {
    let n = map.labels.len();
    let connected = |m: usize| map.flow.iter().any(|&(a, b)| a == m || b == m);
    let forward = acyclic(n, &map.flow);
    let mut depth = vec![0usize; n];
    // Edges in a DAG: n passes reach every longest path.
    for _ in 0..n {
        for &(up, down) in &forward {
            depth[down] = depth[down].max(depth[up] + 1);
        }
    }

    let mut layers: Vec<Vec<usize>> = Vec::new();
    for m in (0..n).filter(|&m| connected(m)) {
        if layers.len() <= depth[m] {
            layers.resize(depth[m] + 1, Vec::new());
        }
        layers[depth[m]].push(m);
    }
    layers.retain(|l| !l.is_empty());
    for i in 1..layers.len() {
        let (above, rest) = layers.split_at_mut(i);
        let above = &above[i - 1];
        // Mean position of the module's upstreams in the row above; modules
        // fed only from further up keep their map order at the end.
        let pos = |m: usize| {
            let ps: Vec<usize> = map
                .flow
                .iter()
                .filter(|&&(_, down)| down == m)
                .filter_map(|&(up, _)| above.iter().position(|&x| x == up))
                .collect();
            if ps.is_empty() {
                f32::MAX
            } else {
                ps.iter().sum::<usize>() as f32 / ps.len() as f32
            }
        };
        rest[0].sort_by(|&a, &b| pos(a).total_cmp(&pos(b)));
    }

    let loose: Vec<usize> = (0..n).filter(|&m| !connected(m)).collect();
    if !loose.is_empty() {
        layers.push(loose);
    }
    layers
}

/// The flow edges minus those that close a cycle (and self-edges): the
/// back edges of a depth-first walk from each module in map order.
fn acyclic(n: usize, flow: &[(usize, usize)]) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        New,
        Open,
        Done,
    }
    fn walk(m: usize, flow: &[(usize, usize)], mark: &mut [Mark], out: &mut Vec<(usize, usize)>) {
        mark[m] = Mark::Open;
        for &(up, down) in flow.iter().filter(|e| e.0 == m) {
            match mark[down] {
                Mark::Open => {} // back up the walk: closes a cycle
                Mark::New => {
                    out.push((up, down));
                    walk(down, flow, mark, out);
                }
                Mark::Done => out.push((up, down)),
            }
        }
        mark[m] = Mark::Done;
    }
    let mut mark = vec![Mark::New; n];
    let mut out = Vec::new();
    for m in 0..n {
        if mark[m] == Mark::New {
            walk(m, flow, &mut mark, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn info(modules: &[&str], edges: &[(&str, &str)]) -> MapInfo {
        let mut toml = String::new();
        for m in modules {
            toml.push_str(&format!("[[module]]\nid = \"{m}\"\n"));
        }
        for (from, to) in edges {
            toml.push_str(&format!("[[edge]]\nfrom = \"{from}\"\nto = \"{to}\"\n"));
        }
        MapInfo::from(&ModuleMap::parse(&toml, Path::new("/repo")).unwrap())
    }

    #[test]
    fn upstream_modules_sit_above_the_modules_they_feed() {
        // urbandocs: ingest → substrate → search → server → evals.
        let m = info(
            &["server", "search", "substrate", "ingest", "evals"],
            &[
                ("server", "search"),
                ("search", "substrate"),
                ("substrate", "ingest"),
                ("evals", "server"),
            ],
        );
        assert_eq!(layers(&m), [vec![3], vec![2], vec![1], vec![0], vec![4]]);
    }

    #[test]
    fn a_module_sits_below_its_lowest_upstream() {
        // b reads a; c reads both a and b, so it goes under b.
        let m = info(&["a", "b", "c"], &[("b", "a"), ("c", "a"), ("c", "b")]);
        assert_eq!(layers(&m), [vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn modules_with_no_edges_share_a_last_row() {
        let m = info(&["lone", "a", "b", "other"], &[("b", "a")]);
        assert_eq!(layers(&m), [vec![1], vec![2], vec![0, 3]]);
    }

    #[test]
    fn a_row_follows_the_order_of_the_modules_feeding_it() {
        // Row 1 is x, y; z is fed by y, w by x, so the next row is w, z.
        let m = info(
            &["x", "y", "z", "w", "top"],
            &[("x", "top"), ("y", "top"), ("z", "y"), ("w", "x")],
        );
        assert_eq!(layers(&m), [vec![4], vec![0, 1], vec![3, 2]]);
    }

    #[test]
    fn a_cycle_keeps_the_modules_it_feeds_below_it() {
        // a and b read each other; c reads a. Breaking the cycle at the
        // edge back into a leaves a above both b and c.
        let m = info(&["a", "b", "c"], &[("a", "b"), ("b", "a"), ("c", "a")]);
        assert_eq!(layers(&m), [vec![0], vec![1, 2]]);
    }

    #[test]
    fn a_cycle_still_places_every_module_once() {
        let m = info(&["a", "b", "c"], &[("a", "b"), ("b", "a"), ("c", "a")]);
        let mut all = layers(&m).concat();
        all.sort();
        assert_eq!(all, [0, 1, 2]);
    }
}
