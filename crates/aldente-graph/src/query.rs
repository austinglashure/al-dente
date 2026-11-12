//! Lookups shared by the CLI and the server: resolving short symbol names,
//! searching, direct neighbors, and neighborhoods.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::str::FromStr;

use aldente_model::query::Neighbor;
use aldente_model::{
    Aggregation, AggregationLevel, Certainty, Edge, EdgeKind, GraphPayload, Node, NodeId, NodeKind,
};

use crate::build::Graph;
use crate::rollup::{MAX_EDGES, MAX_NODES, roll};

/// Edge kinds that count as invocation for callers and callees.
const INVOCATION: &[EdgeKind] = &[EdgeKind::Calls, EdgeKind::References, EdgeKind::Dispatches];

/// The outcome of resolving a user-typed name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Found(NodeId),
    /// Several nodes match; sorted.
    Ambiguous(Vec<NodeId>),
    NotFound,
}

/// Resolves a full ID or an unambiguous suffix of one (`03-graph-model.md`,
/// "Short forms"). A suffix must start at a component boundary: after `/`
/// or `:` in the ID.
///
/// Example: in mono-mini, `"billing.core/charge!"` is
/// `Found(clj:services/billing//billing.core/charge!)`; `"charge!"` is
/// `Ambiguous([…billing…, …orders…])`; `"arge!"` is `NotFound`.
pub fn resolve(graph: &Graph, query: &str) -> Resolution {
    if let Ok(id) = NodeId::from_str(query)
        && graph.nodes.contains_key(&id)
    {
        return Resolution::Found(id);
    }
    let mut matches: Vec<NodeId> = graph
        .nodes
        .keys()
        .filter(|id| suffix_match(&id.to_string(), query))
        .cloned()
        .collect();
    matches.sort();
    match matches.len() {
        0 => Resolution::NotFound,
        1 => Resolution::Found(matches.remove(0)),
        _ => Resolution::Ambiguous(matches),
    }
}

/// Whether `query` is a suffix of `id` starting at a component boundary.
///
/// Example: `suffix_match("clj:.//a.b/f", "b/f")` is `false`; `"a.b/f"` and `"f"` are `true`.
fn suffix_match(id: &str, query: &str) -> bool {
    if query.is_empty() || !id.ends_with(query) {
        return false;
    }
    let start = id.len() - query.len();
    start == 0 || matches!(id.as_bytes().get(start - 1), Some(b'/' | b':'))
}

/// Nodes whose name or ID contains `pattern` (case-insensitive), of the
/// given kinds (symbols, impls and namespaces when `kinds` is empty), sorted
/// by ID, at most `limit` (0 = no limit). Returns whether more matched.
///
/// Example: `search(g, "charge", &[], 50)` on clj-mini gives `shop.payment/charge`
/// and its impls.
pub fn search(graph: &Graph, pattern: &str, kinds: &[NodeKind], limit: usize) -> (Vec<Node>, bool) {
    let needle = pattern.to_lowercase();
    let default = [NodeKind::Symbol, NodeKind::Impl, NodeKind::Namespace];
    let kinds: &[NodeKind] = if kinds.is_empty() { &default } else { kinds };
    let mut hits: Vec<Node> = graph
        .nodes
        .values()
        .filter(|n| kinds.contains(&n.kind))
        .filter(|n| {
            n.name.to_lowercase().contains(&needle)
                || n.id.to_string().to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    let truncated = limit > 0 && hits.len() > limit;
    if truncated {
        hits.truncate(limit);
    }
    (hits, truncated)
}

/// Direct callers and callees of `id` over invocation edges, strongest
/// certainty first, then by ID.
///
/// Example: `neighbors(g, shop.pricing/half-off)` gives caller
/// `shop.pricing/price@:book` (references, likely) and callee `clojure.core//`.
pub fn neighbors(graph: &Graph, id: &NodeId) -> (Vec<Neighbor>, Vec<Neighbor>) {
    let make = |e: &Edge, other: &NodeId| Neighbor {
        id: other.clone(),
        name: graph
            .nodes
            .get(other)
            .map(|n| n.name.clone())
            .unwrap_or_default(),
        kind: e.kind,
        certainty: e.certainty,
        reason: e.reason,
        sites: e.sites.clone(),
    };
    let invocation = graph.edges.iter().filter(|e| INVOCATION.contains(&e.kind));
    let mut callers: Vec<Neighbor> = invocation
        .clone()
        .filter(|e| &e.to == id)
        .map(|e| make(e, &e.from))
        .collect();
    let mut callees: Vec<Neighbor> = invocation
        .filter(|e| &e.from == id)
        .map(|e| make(e, &e.to))
        .collect();
    for list in [&mut callers, &mut callees] {
        list.sort_by(|a, b| {
            b.certainty
                .cmp(&a.certainty)
                .then_with(|| a.id.cmp(&b.id))
                .then(a.kind.cmp(&b.kind))
        });
    }
    (callers, callees)
}

/// Which way to walk from a focus node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
    Both,
}

/// The part of `payload` within `depth` steps of `focus`, following every
/// edge kind except `contains`, in `direction`. Keeps the edges between the
/// nodes it keeps.
///
/// Example: clj-mini at symbol level, focus `shop.core/checkout`, depth 1,
/// `Out` gives checkout and the eight nodes it calls or references.
pub fn neighborhood(
    payload: &GraphPayload,
    focus: &NodeId,
    depth: usize,
    direction: Direction,
) -> GraphPayload {
    let mut keep: BTreeSet<NodeId> = BTreeSet::from([focus.clone()]);
    let mut queue = VecDeque::from([(focus.clone(), 0usize)]);
    let walkable = |e: &&Edge| e.kind != EdgeKind::Contains;
    while let Some((id, d)) = queue.pop_front() {
        if d == depth {
            continue;
        }
        for e in payload.edges.iter().filter(walkable) {
            let next = match direction {
                Direction::Out if e.from == id => &e.to,
                Direction::In if e.to == id => &e.from,
                Direction::Both if e.from == id => &e.to,
                Direction::Both if e.to == id => &e.from,
                _ => continue,
            };
            if keep.insert(next.clone()) {
                queue.push_back((next.clone(), d + 1));
            }
        }
    }
    GraphPayload {
        nodes: payload
            .nodes
            .iter()
            .filter(|n| keep.contains(&n.id))
            .cloned()
            .collect(),
        edges: payload
            .edges
            .iter()
            .filter(walkable)
            .filter(|e| keep.contains(&e.from) && keep.contains(&e.to))
            .cloned()
            .collect(),
        truncated: payload.truncated,
        aggregated: payload.aggregated.clone(),
    }
}

/// Narrows a graph for the API.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Drop edges weaker than this (edges without a certainty are kept).
    pub min_certainty: Option<Certainty>,
    /// Keep only these edge kinds (all when empty).
    pub kinds: Vec<EdgeKind>,
    /// Keep only nodes of this unit, plus the targets of their edges.
    pub unit: Option<NodeId>,
}

/// Applies `filter` to a payload.
///
/// Example: `min_certainty: Some(Likely)` drops `possible` edges.
pub fn filter(graph: &Graph, payload: GraphPayload, filter: &Filter) -> GraphPayload {
    let keep_edge = |e: &Edge| {
        filter
            .min_certainty
            .is_none_or(|min| e.certainty.is_none_or(|c| c >= min))
            && (filter.kinds.is_empty() || filter.kinds.contains(&e.kind))
    };
    let mut edges: Vec<Edge> = payload.edges.into_iter().filter(keep_edge).collect();
    let mut nodes = payload.nodes;
    if let Some(unit) = &filter.unit {
        let in_unit = |id: &NodeId| id == unit || graph.unit_of(id).as_ref() == Some(unit);
        edges.retain(|e| in_unit(&e.from));
        let targets: BTreeSet<NodeId> = edges.iter().map(|e| e.to.clone()).collect();
        nodes.retain(|n| in_unit(&n.id) || targets.contains(&n.id));
    }
    GraphPayload {
        nodes,
        edges,
        truncated: payload.truncated,
        aggregated: payload.aggregated,
    }
}

/// The graph at `level` (filtered) if it fits the payload cap; otherwise the
/// next coarser level that fits (marked by `aggregated`); if even the unit
/// level does not fit, its best-connected nodes up to the cap, `truncated`.
///
/// Example: a 50,000-symbol repository asked at `Symbol` level gets its
/// namespace graph with `aggregated: namespace`.
pub fn fit(graph: &Graph, level: AggregationLevel, f: &Filter) -> GraphPayload {
    let order = [
        AggregationLevel::Symbol,
        AggregationLevel::Namespace,
        AggregationLevel::Unit,
    ];
    let start = order.iter().position(|l| *l == level).unwrap_or(1);
    let mut last = GraphPayload::default();
    for l in &order[start..] {
        let mut p = filter(graph, graph.payload(*l), f);
        if p.nodes.len() <= MAX_NODES && p.edges.len() <= MAX_EDGES {
            if *l != level {
                p.aggregated = Some(Aggregation { level: *l });
            }
            return p;
        }
        last = p;
    }
    cap(last)
}

/// Keeps the best-connected nodes up to the cap and the edges among them,
/// marking the payload truncated. Payloads within the cap are unchanged.
///
/// Example: 3,000 nodes become the 2,000 with the highest degree.
pub fn cap(mut p: GraphPayload) -> GraphPayload {
    if p.nodes.len() <= MAX_NODES && p.edges.len() <= MAX_EDGES {
        return p;
    }
    let mut degree: BTreeMap<&NodeId, usize> = BTreeMap::new();
    for e in &p.edges {
        *degree.entry(&e.from).or_default() += 1;
        *degree.entry(&e.to).or_default() += 1;
    }
    let mut ranked: Vec<(usize, NodeId)> = p
        .nodes
        .iter()
        .map(|n| (degree.get(&n.id).copied().unwrap_or(0), n.id.clone()))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let keep: BTreeSet<NodeId> = ranked
        .into_iter()
        .take(MAX_NODES)
        .map(|(_, id)| id)
        .collect();
    p.nodes.retain(|n| keep.contains(&n.id));
    p.edges
        .retain(|e| keep.contains(&e.from) && keep.contains(&e.to));
    p.edges.truncate(MAX_EDGES);
    p.truncated = true;
    p
}

/// Children of an aggregate (unit → namespaces, namespace → its symbols and
/// impls, multimethod or protocol method → impls), best-connected first, at
/// most `limit`, plus invocation and `requires` edges from or to them. An
/// edge end that is not a child is shown as itself if `visible`, else as its
/// namespace or unit if that is visible (rolled up with a count), else dropped.
///
/// Example: expanding `clj:.//shop.pricing` with `shop.util` visible gives
/// `half-off`, `price` and the four `price@…` impls, with
/// `price@:book -calls-> shop.util` rolled up.
pub fn expand(graph: &Graph, id: &NodeId, visible: &[NodeId], limit: usize) -> GraphPayload {
    let mut children: Vec<NodeId> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains && &e.from == id)
        .map(|e| e.to.clone())
        .collect();
    let degree = |n: &NodeId| {
        graph
            .edges
            .iter()
            .filter(|e| INVOCATION.contains(&e.kind) && (&e.from == n || &e.to == n))
            .count()
    };
    children.sort_by(|a, b| degree(b).cmp(&degree(a)).then_with(|| a.cmp(b)));
    let truncated = limit > 0 && children.len() > limit;
    if truncated {
        children.truncate(limit);
    }
    let kids: BTreeSet<&NodeId> = children.iter().collect();
    let shown: BTreeSet<&NodeId> = visible.iter().collect();
    let place = |n: &NodeId| -> Option<NodeId> {
        if kids.contains(n) || shown.contains(n) {
            return Some(n.clone());
        }
        [graph.namespace_of(n), graph.unit_of(n)]
            .into_iter()
            .flatten()
            .find(|g| shown.contains(g))
    };
    let mut groups: BTreeMap<(NodeId, NodeId, EdgeKind), Vec<&Edge>> = BTreeMap::new();
    for e in graph
        .edges
        .iter()
        .filter(|e| INVOCATION.contains(&e.kind) || e.kind == EdgeKind::Requires)
    {
        if !kids.contains(&e.from) && !kids.contains(&e.to) {
            continue;
        }
        let (Some(from), Some(to)) = (place(&e.from), place(&e.to)) else {
            continue;
        };
        if from != to {
            groups.entry((from, to, e.kind)).or_default().push(e);
        }
    }
    let edges = groups
        .into_iter()
        .map(|((from, to, kind), members)| match members.as_slice() {
            [one] if one.from == from && one.to == to => (*one).clone(),
            _ => roll(from, to, kind, &members),
        })
        .collect();
    GraphPayload {
        nodes: children.iter().map(|c| graph.group_node(c)).collect(),
        edges,
        truncated,
        aggregated: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{AggregationLevel, Certainty, Reason};
    use std::collections::BTreeSet;

    fn node(id: &str) -> Node {
        let id = NodeId::from_str(id).unwrap();
        Node {
            name: id
                .to_string()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_owned(),
            kind: id.kind(),
            id,
            subkind: None,
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
        }
    }

    fn edge(from: &str, to: &str, kind: EdgeKind, c: Option<Certainty>) -> Edge {
        Edge {
            from: NodeId::from_str(from).unwrap(),
            to: NodeId::from_str(to).unwrap(),
            kind,
            certainty: c,
            reason: c.map(|_| Reason::Direct),
            lang: None,
            sites: vec![],
            count: None,
            by_certainty: None,
        }
    }

    fn graph() -> Graph {
        let mut g = Graph::default();
        for id in [
            "clj:services/billing//billing.core/charge!",
            "clj:services/orders//orders.core/charge!",
            "clj:services/orders//orders.core/total",
            "clj:services/orders//orders.core",
        ] {
            let n = node(id);
            g.nodes.insert(n.id.clone(), n);
        }
        g.edges = vec![
            edge(
                "clj:services/orders//orders.core/total",
                "clj:services/orders//orders.core/charge!",
                EdgeKind::Calls,
                Some(Certainty::Certain),
            ),
            edge(
                "clj:services/orders//orders.core",
                "clj:services/orders//orders.core/total",
                EdgeKind::Contains,
                None,
            ),
        ];
        g
    }

    #[test]
    fn short_forms_resolve_or_report_ambiguity() {
        let g = graph();
        let found = |q| match resolve(&g, q) {
            Resolution::Found(id) => id.to_string(),
            other => format!("{other:?}"),
        };
        assert_eq!(
            found("billing.core/charge!"),
            "clj:services/billing//billing.core/charge!"
        );
        assert_eq!(found("total"), "clj:services/orders//orders.core/total");
        assert_eq!(
            found("clj:services/orders//orders.core"),
            "clj:services/orders//orders.core"
        );
        assert!(matches!(resolve(&g, "charge!"), Resolution::Ambiguous(v) if v.len() == 2));
        assert_eq!(resolve(&g, "arge!"), Resolution::NotFound);
        assert_eq!(resolve(&g, ""), Resolution::NotFound);
    }

    #[test]
    fn search_is_case_insensitive_and_limited() {
        let g = graph();
        let (hits, truncated) = search(&g, "CHARGE", &[], 1);
        assert_eq!(hits.len(), 1);
        assert!(truncated);
        let (all, truncated) = search(&g, "charge", &[], 0);
        assert_eq!(all.len(), 2);
        assert!(!truncated);
    }

    #[test]
    fn filters_and_expand() {
        let g = graph();
        let p = g.payload(AggregationLevel::Symbol);
        let strong = filter(
            &g,
            p.clone(),
            &Filter {
                min_certainty: Some(Certainty::Certain),
                ..Filter::default()
            },
        );
        assert_eq!(strong.edges.len(), 2, "contains has no certainty and stays");
        let only_calls = filter(
            &g,
            p,
            &Filter {
                kinds: vec![EdgeKind::Calls],
                ..Filter::default()
            },
        );
        assert_eq!(only_calls.edges.len(), 1);
        let ns = NodeId::from_str("clj:services/orders//orders.core").unwrap();
        let e = expand(&g, &ns, &[], 10);
        assert_eq!(e.nodes.len(), 1, "orders.core contains total");
        assert!(e.edges.is_empty(), "charge! is neither a child nor visible");
        let charge = NodeId::from_str("clj:services/orders//orders.core/charge!").unwrap();
        let e = expand(&g, &ns, std::slice::from_ref(&charge), 10);
        assert_eq!(e.edges.len(), 1);
        assert_eq!(e.edges[0].count, None, "a single raw edge is kept as is");
        assert!(expand(&g, &ns, &[], 0).nodes.len() == 1);
    }

    #[test]
    fn fit_keeps_small_graphs_and_cap_trims_big_ones() {
        let g = graph();
        let p = fit(&g, AggregationLevel::Symbol, &Filter::default());
        assert!(!p.truncated);
        assert_eq!(p.aggregated, None);
        let mut big = GraphPayload::default();
        for i in 0..(MAX_NODES + 5) {
            big.nodes.push(node(&format!("clj:.//n/x{i}")));
        }
        let capped = cap(big);
        assert!(capped.truncated);
        assert_eq!(capped.nodes.len(), MAX_NODES);
    }

    #[test]
    fn neighbors_and_neighborhoods() {
        let g = graph();
        let charge = NodeId::from_str("clj:services/orders//orders.core/charge!").unwrap();
        let (callers, callees) = neighbors(&g, &charge);
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].name, "total");
        assert!(callees.is_empty());
        let p = g.payload(AggregationLevel::Symbol);
        let hood = neighborhood(&p, &charge, 1, Direction::In);
        assert_eq!(hood.nodes.len(), 2);
        assert_eq!(hood.edges.len(), 1, "contains edges are not walked or kept");
        assert_eq!(neighborhood(&p, &charge, 1, Direction::Out).nodes.len(), 1);
        assert_eq!(neighborhood(&p, &charge, 0, Direction::Both).nodes.len(), 1);
    }
}
