//! The module map as the diagram sees it: module labels, and the `[[edge]]`s
//! turned around to point the way data flows, laid out in rows.

use crate::modulemap::ModuleMap;

/// What the UI needs from the module map; the map itself stays with the
/// collector thread.
pub struct MapInfo {
    pub name: String,
    pub labels: Vec<String>,
    /// The map's edges as data flow, (upstream, downstream): an edge
    /// `from = A, to = B` means A depends on B, so data flows from B into A.
    pub flow: Vec<(usize, usize)>,
}

impl From<&ModuleMap> for MapInfo {
    fn from(map: &ModuleMap) -> Self {
        // Edge endpoints are validated against module ids when the map loads.
        let idx = |id: &str| map.modules.iter().position(|m| m.id == id);
        Self {
            name: map.name.clone(),
            labels: map.modules.iter().map(|m| m.label.clone()).collect(),
            flow: map
                .edges
                .iter()
                .filter_map(|e| Some((idx(&e.to)?, idx(&e.from)?)))
                .collect(),
        }
    }
}

/// Rows of module indices, upstream above downstream. Every module is in
/// exactly one row; modules with no edges share a last row of their own.
///
/// A module's row is the longest data-flow path reaching it, so it sits
/// below every module it reads from. Each row is ordered by where the
/// modules feeding it sit in the row above, so arrows cross less.
pub fn layers(map: &MapInfo) -> Vec<Vec<usize>> {
    let n = map.labels.len();
    let connected = |m: usize| map.flow.iter().any(|&(a, b)| a == m || b == m);
    let mut depth = vec![0usize; n];
    // Bounded relaxation, so a cycle in the map can't loop forever.
    for _ in 0..n {
        for &(up, down) in &map.flow {
            if up != down {
                depth[down] = depth[down].max((depth[up] + 1).min(n));
            }
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
    fn an_edge_flows_from_the_dependency_into_the_dependent() {
        // server depends on search: search's data flows into server.
        let m = info(&["server", "search"], &[("server", "search")]);
        assert_eq!(m.flow, [(1, 0)]);
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
    fn a_cycle_still_places_every_module_once() {
        let m = info(&["a", "b", "c"], &[("a", "b"), ("b", "a"), ("c", "a")]);
        let mut all = layers(&m).concat();
        all.sort();
        assert_eq!(all, [0, 1, 2]);
    }
}
