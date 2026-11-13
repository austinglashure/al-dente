//! Dead code: symbols nothing reaches from entry points, tests or code that
//! runs when a namespace loads, over every invocation edge at any
//! certainty. The graph over-approximates calls, so what stays
//! unreachable is a strong signal.

use std::collections::{HashMap, HashSet, VecDeque};

use aldente_graph::Graph;
use aldente_model::measures::{DeadCodeReport, DeadSymbol};
use aldente_model::{EdgeKind, NodeId, NodeKind, SymbolFlag, SymbolSubkind};

use super::projection::is_test;

/// Edge kinds that carry reachability forward.
const FOLLOW: &[EdgeKind] = &[
    EdgeKind::Calls,
    EdgeKind::References,
    EdgeKind::Dispatches,
    EdgeKind::Registers,
    EdgeKind::Tests,
];

/// Whether a node is a root: an entry point, test code, or a top-level
/// form (it runs when its namespace loads).
///
/// Example: `-main` and `<top>@L12` are roots; `shop.util/clamp` is not.
fn is_root(graph: &Graph, id: &NodeId) -> bool {
    let Some(n) = graph.nodes.get(id) else {
        return false;
    };
    n.flags.contains(&SymbolFlag::EntryPoint)
        || is_test(graph, n)
        || (n.kind == NodeKind::Impl && n.name.starts_with("<top>@"))
}

/// The dead-code report.
///
/// Example: for clj-mini, the symbols listed under "Dead code" in its
/// `EXPECTED.md`.
pub fn dead_code(graph: &Graph) -> DeadCodeReport {
    let mut out: HashMap<&NodeId, Vec<&NodeId>> = HashMap::new();
    for e in &graph.edges {
        if FOLLOW.contains(&e.kind) {
            out.entry(&e.from).or_default().push(&e.to);
        } else if e.kind == EdgeKind::Implements {
            // A used method or multimethod reaches its implementations, and
            // a used implementation keeps its protocol alive.
            out.entry(&e.to).or_default().push(&e.from);
            out.entry(&e.from).or_default().push(&e.to);
        }
    }
    // A record or type is alive when one of its constructors is used.
    let by_name: HashMap<(Option<&str>, &str), &NodeId> = graph
        .nodes
        .values()
        .filter(|n| matches!(n.subkind, Some(SymbolSubkind::Record | SymbolSubkind::Type)))
        .map(|n| ((n.namespace.as_deref(), n.name.as_str()), &n.id))
        .collect();
    for n in graph.nodes.values() {
        let base = n
            .name
            .strip_prefix("map->")
            .or_else(|| n.name.strip_prefix("->"));
        if let Some(&record) = base.and_then(|b| by_name.get(&(n.namespace.as_deref(), b))) {
            out.entry(&n.id).or_default().push(record);
        }
    }
    let roots: Vec<&NodeId> = graph.nodes.keys().filter(|id| is_root(graph, id)).collect();
    let mut seen: HashSet<&NodeId> = roots.iter().copied().collect();
    let mut queue: VecDeque<&NodeId> = roots.iter().copied().collect();
    // A `def` of a value runs its initializer at load time: what it calls
    // is reached even when the var itself is not used.
    for n in graph.nodes.values() {
        if n.subkind == Some(SymbolSubkind::Var) {
            for &t in out.get(&n.id).map(Vec::as_slice).unwrap_or(&[]) {
                if seen.insert(t) {
                    queue.push_back(t);
                }
            }
        }
    }
    while let Some(v) = queue.pop_front() {
        for &t in out.get(v).map(Vec::as_slice).unwrap_or(&[]) {
            if seen.insert(t) {
                queue.push_back(t);
            }
        }
    }
    let dead = graph
        .nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::Symbol | NodeKind::Impl))
        .filter(|n| !seen.contains(&n.id))
        .map(|n| DeadSymbol {
            id: n.id.clone(),
            name: n.name.clone(),
            namespace: n.namespace.clone(),
            loc: n.loc.clone(),
        })
        .collect();
    DeadCodeReport {
        roots: roots.len() as u32,
        dead,
    }
}
