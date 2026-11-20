//! Size per node: lines and symbols, for ranking namespaces and units.
//!
//! Lines are counted from the graph, not by reading files (so a snapshot
//! answers without the repository): for each source file of a namespace, the
//! last line of its last definition, summed over the files. Comments and
//! blank lines after the last definition are not counted.

use std::collections::{BTreeMap, BTreeSet};

use aldente_graph::Graph;
use aldente_model::measures::NodeSize;
use aldente_model::{AggregationLevel, NodeId, NodeKind, RepoPath};

/// Lines and symbols for every repository node at `level`, sorted by ID.
///
/// Example: a namespace whose file `src/a.clj` defines `f` on lines 3–6 and
/// `g` on line 8 has `lines: 8, symbols: 2`; its unit sums its namespaces.
pub fn sizes(graph: &Graph, level: AggregationLevel) -> Vec<NodeSize> {
    match level {
        AggregationLevel::Symbol => graph
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Symbol | NodeKind::Impl))
            .map(|n| NodeSize {
                id: n.id.clone(),
                lines: n
                    .loc
                    .as_ref()
                    .map_or(0, |l| l.end_line.saturating_sub(l.line) + 1),
                symbols: 1,
            })
            .collect(),
        AggregationLevel::Namespace | AggregationLevel::Unit => {
            // Last line per (group, file), and symbols per group.
            let mut last: BTreeMap<(NodeId, RepoPath), u32> = BTreeMap::new();
            let mut symbols: BTreeMap<NodeId, u32> = BTreeMap::new();
            let mut groups: BTreeSet<NodeId> = BTreeSet::new();
            for n in graph.nodes.values() {
                let ns = match n.kind {
                    NodeKind::Namespace | NodeKind::Symbol | NodeKind::Impl => {
                        graph.namespace_of(&n.id)
                    }
                    _ => None,
                };
                let Some(ns) = ns else { continue };
                let group = if level == AggregationLevel::Unit {
                    match graph.unit_of(&ns) {
                        Some(u) => u,
                        None => continue,
                    }
                } else {
                    ns
                };
                groups.insert(group.clone());
                if n.kind != NodeKind::Namespace {
                    *symbols.entry(group.clone()).or_default() += 1;
                }
                if let Some(l) = &n.loc {
                    let e = last.entry((group, l.file.clone())).or_default();
                    *e = (*e).max(l.end_line);
                }
            }
            let mut lines: BTreeMap<NodeId, u32> = BTreeMap::new();
            for ((group, _), l) in last {
                *lines.entry(group).or_default() += l;
            }
            groups
                .into_iter()
                .map(|id| NodeSize {
                    lines: lines.get(&id).copied().unwrap_or(0),
                    symbols: symbols.get(&id).copied().unwrap_or(0),
                    id,
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{Location, Node};
    use std::str::FromStr;

    fn at(id: &str, file: &str, line: u32, end: u32) -> Node {
        let id = NodeId::from_str(id).unwrap();
        Node {
            kind: id.kind(),
            name: id.to_string(),
            id,
            subkind: None,
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: Some(Location {
                file: RepoPath::parse(file).unwrap(),
                line,
                col: 1,
                end_line: end,
                end_col: 1,
            }),
            doc: None,
            signature: None,
            members: None,
            symbols: None,
        }
    }

    fn graph() -> Graph {
        let mut g = Graph::default();
        for n in [
            at("clj:svc//a", "svc/src/a.clj", 1, 1),
            at("clj:svc//a/f", "svc/src/a.clj", 3, 6),
            at("clj:svc//a/g", "svc/src/a.clj", 8, 8),
            // The same namespace also in a .cljs file.
            at("clj:svc//a/h", "svc/src/a.cljs", 2, 10),
            at("clj:svc//b", "svc/src/b.clj", 1, 1),
            at("clj:svc//b/k", "svc/src/b.clj", 3, 4),
        ] {
            g.nodes.insert(n.id.clone(), n);
        }
        let ext = NodeId::from_str("ext:clj//clojure.core/map").unwrap();
        let mut e = at("clj:svc//b/k", "svc/src/b.clj", 1, 1);
        e.id = ext.clone();
        e.kind = NodeKind::External;
        e.loc = None;
        g.nodes.insert(ext, e);
        g
    }

    fn row(r: &[NodeSize], id: &str) -> (u32, u32) {
        let s = r.iter().find(|s| s.id.to_string() == id).unwrap();
        (s.lines, s.symbols)
    }

    #[test]
    fn namespaces_sum_the_last_definition_line_of_each_file() {
        let r = sizes(&graph(), AggregationLevel::Namespace);
        assert_eq!(r.len(), 2, "externals are not sized");
        assert_eq!(row(&r, "clj:svc//a"), (8 + 10, 3));
        assert_eq!(row(&r, "clj:svc//b"), (4, 1));
    }

    #[test]
    fn units_sum_their_namespaces_and_symbols_count_their_lines() {
        let r = sizes(&graph(), AggregationLevel::Unit);
        assert_eq!(row(&r, "unit:svc"), (8 + 10 + 4, 4));
        let s = sizes(&graph(), AggregationLevel::Symbol);
        assert_eq!(row(&s, "clj:svc//a/f"), (4, 1));
    }
}
