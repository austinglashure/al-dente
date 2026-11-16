//! Lookups shared by the CLI and the server: resolving short symbol names,
//! searching, direct neighbors, and neighborhoods.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::str::FromStr;

use aldente_model::query::{Child, Neighbor};
use aldente_model::{
    Aggregation, AggregationLevel, Certainty, Edge, EdgeKind, GraphPayload, Node, NodeId, NodeKind,
    SymbolFlag, SymbolSubkind,
};

use crate::build::Graph;
use crate::cache::Cache;
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

/// Which nodes a search returns besides the pattern.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilter {
    /// Node kinds; with `subkinds`, a node matching either is kept. Both
    /// empty means symbols, impls and namespaces.
    pub kinds: Vec<NodeKind>,
    pub subkinds: Vec<SymbolSubkind>,
    /// Flags a node must all have.
    pub flags: Vec<SymbolFlag>,
}

impl SearchFilter {
    /// Whether `n` passes the kind and flag filters.
    ///
    /// Example: `{subkinds: [test]}` keeps `deftest` vars only.
    fn keeps(&self, n: &Node) -> bool {
        let default = [NodeKind::Symbol, NodeKind::Impl, NodeKind::Namespace];
        let kind = if self.kinds.is_empty() && self.subkinds.is_empty() {
            default.contains(&n.kind)
        } else {
            self.kinds.contains(&n.kind) || n.subkind.is_some_and(|s| self.subkinds.contains(&s))
        };
        kind && self.flags.iter().all(|f| n.flags.contains(f))
    }
}

/// What a search matches against a node's name or ID.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Case-insensitive substring; held lowercased.
    Substring(String),
    /// Regular expression, unanchored and as written (case-sensitive unless
    /// it says `(?i)`).
    Regex(regex::Regex),
}

impl Pattern {
    /// A case-insensitive substring pattern.
    ///
    /// Example: `Pattern::substring("Charge")` matches `billing.core/charge!`.
    pub fn substring(s: &str) -> Pattern {
        Pattern::Substring(s.to_lowercase())
    }

    /// A regular-expression pattern, or the parse error.
    ///
    /// Example: `Pattern::regex("^charge!$")` is `Ok`; `Pattern::regex("(")` is `Err`.
    pub fn regex(s: &str) -> Result<Pattern, regex::Error> {
        regex::Regex::new(s).map(Pattern::Regex)
    }

    /// Whether the node's name or full ID matches.
    ///
    /// Example: `Pattern::regex("^charge!$")` matches the node named `charge!`.
    fn matches(&self, n: &Node) -> bool {
        match self {
            Pattern::Substring(needle) => {
                n.name.to_lowercase().contains(needle)
                    || n.id.to_string().to_lowercase().contains(needle)
            }
            Pattern::Regex(re) => re.is_match(&n.name) || re.is_match(&n.id.to_string()),
        }
    }
}

/// Nodes whose name or ID contains `pattern` (case-insensitive), of the
/// given kinds (symbols, impls and namespaces when `kinds` is empty), sorted
/// by ID, at most `limit` (0 = no limit). Returns whether more matched.
///
/// Example: `search(g, "charge", &[], 50)` on clj-mini gives `shop.payment/charge`
/// and its impls.
pub fn search(graph: &Graph, pattern: &str, kinds: &[NodeKind], limit: usize) -> (Vec<Node>, bool) {
    let filter = SearchFilter {
        kinds: kinds.to_vec(),
        ..SearchFilter::default()
    };
    search_filtered(graph, &Pattern::substring(pattern), &filter, limit)
}

/// [`search`] with any [`Pattern`] and with subkind and flag filters. An
/// empty substring matches every node the filter keeps.
///
/// Example: `search_filtered(g, &Pattern::substring(""), &{flags: [entry-point]}, 0)`
/// lists the entry points.
pub fn search_filtered(
    graph: &Graph,
    pattern: &Pattern,
    filter: &SearchFilter,
    limit: usize,
) -> (Vec<Node>, bool) {
    let mut hits: Vec<Node> = graph
        .nodes
        .values()
        .filter(|n| filter.keeps(n) && pattern.matches(n))
        .cloned()
        .collect();
    let truncated = limit > 0 && hits.len() > limit;
    if truncated {
        hits.truncate(limit);
    }
    (hits, truncated)
}

/// Direct callers and callees of `id`, strongest certainty first, then by
/// ID. For a symbol or impl: invocation edges. For a namespace or unit: the
/// rolled-up edges at that level (`requires` too), with counts.
///
/// Example: `neighbors(g, shop.pricing/half-off)` gives caller
/// `shop.pricing/price@:book` (references, likely); `neighbors(g, shop.core)`
/// gives callee `shop.pricing` (calls, count 1) and the `requires` edge.
pub fn neighbors(graph: &Graph, cache: &Cache, id: &NodeId) -> (Vec<Neighbor>, Vec<Neighbor>) {
    let level = match graph.nodes.get(id).map(|n| n.kind) {
        Some(NodeKind::Namespace) => Some(AggregationLevel::Namespace),
        Some(NodeKind::Unit) => Some(AggregationLevel::Unit),
        _ => None,
    };
    // Rolled-up edges and group nodes for aggregates; stored ones otherwise.
    let rolled = level.map(|l| cache.payload(graph, l));
    let (edges, nodes): (Vec<&Edge>, Vec<&Node>) = match rolled.as_deref() {
        Some(p) => (
            p.edges
                .iter()
                .filter(|e| INVOCATION.contains(&e.kind) || e.kind == EdgeKind::Requires)
                .collect(),
            p.nodes.iter().collect(),
        ),
        None => {
            // Only this node's edges, from the index.
            let adj = cache.adjacency(graph);
            let edges: Vec<&Edge> = adj
                .incoming(id)
                .iter()
                .chain(adj.outgoing(id))
                .filter_map(|&i| graph.edges.get(i))
                .filter(|e| INVOCATION.contains(&e.kind))
                .collect();
            let nodes = edges
                .iter()
                .flat_map(|e| [&e.from, &e.to])
                .filter_map(|n| graph.nodes.get(n))
                .collect();
            (edges, nodes)
        }
    };
    let names: BTreeMap<&NodeId, &str> = nodes.iter().map(|n| (&n.id, n.name.as_str())).collect();
    let names = |n: &NodeId| names.get(n).map(|s| (*s).to_owned()).unwrap_or_default();
    let make = |e: &Edge, other: &NodeId| Neighbor {
        id: other.clone(),
        name: names(other),
        kind: e.kind,
        certainty: e.certainty,
        reason: e.reason,
        sites: e.sites.clone(),
        count: e.count,
    };
    let mut callers: Vec<Neighbor> = edges
        .iter()
        .filter(|e| &e.to == id)
        .map(|e| make(e, &e.from))
        .collect();
    let mut callees: Vec<Neighbor> = edges
        .iter()
        .filter(|e| &e.from == id)
        .map(|e| make(e, &e.to))
        .collect();
    for list in [&mut callers, &mut callees] {
        list.sort_by(|a, b| {
            b.certainty
                .cmp(&a.certainty)
                .then_with(|| a.id.cmp(&b.id))
                .then(a.kind.cmp(&b.kind))
                .then(a.reason.cmp(&b.reason))
        });
        merge_languages(list);
    }
    (callers, callees)
}

/// Merges neighbors that differ only by language (the `clj` and `cljs`
/// edges of one `.cljc` call) into one, with their sites combined. The list
/// must be sorted so such neighbors are adjacent.
///
/// Example: two `calls direct` entries for `map-data` become one.
fn merge_languages(list: &mut Vec<Neighbor>) {
    let mut merged: Vec<Neighbor> = Vec::with_capacity(list.len());
    for n in list.drain(..) {
        match merged.last_mut() {
            Some(last)
                if last.id == n.id
                    && last.kind == n.kind
                    && last.reason == n.reason
                    && last.certainty == n.certainty =>
            {
                for site in n.sites {
                    if !last.sites.contains(&site) {
                        last.sites.push(site);
                    }
                }
                last.count = match (last.count, n.count) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
            }
            _ => merged.push(n),
        }
    }
    *list = merged;
}

/// What `id` contains (its `contains` edges), by kind then name.
///
/// Example: `children(g, shop.pricing)` gives `half-off` and `price`.
pub fn children(graph: &Graph, id: &NodeId) -> Vec<Child> {
    let mut out: Vec<Child> = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains && &e.from == id)
        .filter_map(|e| graph.nodes.get(&e.to))
        .map(|n| Child {
            id: n.id.clone(),
            name: n.name.clone(),
            kind: n.kind,
            subkind: n.subkind,
        })
        .collect();
    out.sort_by(|a, b| (a.kind, &a.name, &a.id).cmp(&(b.kind, &b.name, &b.id)));
    out
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

/// [`neighborhood`] on the stored (symbol-level) graph, walking the edge
/// index from the focus instead of scanning every edge: the cost depends on
/// the neighborhood, not on the graph.
///
/// Example: `neighborhood_in(g, cache, checkout, 1, Out)` equals
/// `neighborhood(&g.payload(Symbol), checkout, 1, Out)`.
pub fn neighborhood_in(
    graph: &Graph,
    cache: &Cache,
    focus: &NodeId,
    depth: usize,
    direction: Direction,
) -> GraphPayload {
    let adj = cache.adjacency(graph);
    let walkable = |i: &usize| graph.edges.get(*i).filter(|e| e.kind != EdgeKind::Contains);
    let mut keep: BTreeSet<NodeId> = BTreeSet::from([focus.clone()]);
    let mut queue = VecDeque::from([(focus.clone(), 0usize)]);
    while let Some((id, d)) = queue.pop_front() {
        if d == depth {
            continue;
        }
        let out = matches!(direction, Direction::Out | Direction::Both);
        let inc = matches!(direction, Direction::In | Direction::Both);
        let next: Vec<&NodeId> = adj
            .outgoing(&id)
            .iter()
            .filter(|_| out)
            .filter_map(walkable)
            .map(|e| &e.to)
            .chain(
                adj.incoming(&id)
                    .iter()
                    .filter(|_| inc)
                    .filter_map(walkable)
                    .map(|e| &e.from),
            )
            .collect();
        for n in next {
            if keep.insert(n.clone()) {
                queue.push_back((n.clone(), d + 1));
            }
        }
    }
    let nodes = keep
        .iter()
        .filter_map(|id| graph.nodes.get(id))
        .cloned()
        .collect();
    let edges = keep
        .iter()
        .flat_map(|id| adj.outgoing(id).iter().filter_map(walkable))
        .filter(|e| keep.contains(&e.to))
        .cloned()
        .collect();
    GraphPayload {
        nodes,
        edges,
        truncated: false,
        aggregated: None,
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
    /// Keep only repository code and holes: drop externals, platforms and
    /// libraries and the edges that touch them (the GUI's global map).
    pub repo_only: bool,
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
    if filter.repo_only {
        let outside = |k: NodeKind| {
            matches!(
                k,
                NodeKind::External | NodeKind::Platform | NodeKind::Library
            )
        };
        nodes.retain(|n| !outside(n.kind));
        let kept: BTreeSet<&NodeId> = nodes.iter().map(|n| &n.id).collect();
        edges.retain(|e| kept.contains(&e.from) && kept.contains(&e.to));
    }
    GraphPayload {
        nodes,
        edges,
        truncated: payload.truncated,
        aggregated: payload.aggregated,
    }
}

/// The graph at `level` (filtered) if it fits the payload cap; otherwise the
/// next coarser level that fits; if even the unit level does not fit, its
/// best-connected nodes up to the cap, `truncated`. The flag says whether a
/// coarser level than requested was used.
///
/// Example: a 50,000-symbol repository asked at `Symbol` level gets its
/// namespace graph and `true`.
pub fn fit(
    graph: &Graph,
    cache: &Cache,
    level: AggregationLevel,
    f: &Filter,
) -> (GraphPayload, bool) {
    let order = [
        AggregationLevel::Symbol,
        AggregationLevel::Namespace,
        AggregationLevel::Unit,
    ];
    let start = order.iter().position(|l| *l == level).unwrap_or(1);
    let mut last = GraphPayload::default();
    for l in &order[start..] {
        let mut p = filter(graph, cache.payload(graph, *l).into_owned(), f);
        if p.nodes.len() <= MAX_NODES && p.edges.len() <= MAX_EDGES {
            if *l != level {
                p.aggregated = Some(Aggregation { level: *l });
            }
            return (p, *l != level);
        }
        last = p;
    }
    (cap(last), true)
}

/// The `n` nodes with the most calls in and out (invocation edges, each
/// weighted by its rolled-up `count`; `requires` does not count), ties by
/// ID, and the edges among them; at most [`MAX_NODES`] nodes, and the
/// [`MAX_EDGES`] heaviest edges. `truncated` when anything was dropped.
///
/// Example: `top_by_calls(namespace_map, 250)` keeps the 250 busiest namespaces.
pub fn top_by_calls(p: GraphPayload, n: usize) -> GraphPayload {
    let n = n.min(MAX_NODES);
    let weight = |e: &Edge| {
        if e.kind == EdgeKind::Requires {
            0
        } else {
            e.count.unwrap_or(1)
        }
    };
    let mut calls: BTreeMap<&NodeId, u64> = BTreeMap::new();
    for e in &p.edges {
        *calls.entry(&e.from).or_default() += weight(e);
        *calls.entry(&e.to).or_default() += weight(e);
    }
    let mut ranked: Vec<(u64, &NodeId)> = p
        .nodes
        .iter()
        .map(|node| (calls.get(&node.id).copied().unwrap_or(0), &node.id))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let keep: BTreeSet<NodeId> = ranked
        .into_iter()
        .take(n)
        .map(|(_, id)| id.clone())
        .collect();
    let dropped_nodes = keep.len() < p.nodes.len();
    let nodes: Vec<Node> = p
        .nodes
        .into_iter()
        .filter(|x| keep.contains(&x.id))
        .collect();
    let mut edges: Vec<Edge> = p
        .edges
        .into_iter()
        .filter(|e| keep.contains(&e.from) && keep.contains(&e.to))
        .collect();
    let dropped_edges = edges.len() > MAX_EDGES;
    if dropped_edges {
        edges.sort_by_key(|e| std::cmp::Reverse(weight(e)));
        edges.truncate(MAX_EDGES);
        edges.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    }
    GraphPayload {
        nodes,
        edges,
        truncated: p.truncated || dropped_nodes || dropped_edges,
        aggregated: p.aggregated,
    }
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

/// [`expand`] using the edge index: only the children's own edges are
/// read, so the cost depends on the namespace, not on the graph. Equal to
/// [`expand`] (pinned by a test), which stays as the simple reference.
///
/// Example: `expand_in(g, cache, shop.pricing, &[shop.core], 0)`.
pub fn expand_in(
    graph: &Graph,
    cache: &Cache,
    id: &NodeId,
    visible: &[NodeId],
    limit: usize,
) -> GraphPayload {
    let adj = cache.adjacency(graph);
    let edge = |i: &usize| graph.edges.get(*i);
    let mut children: Vec<NodeId> = adj
        .outgoing(id)
        .iter()
        .filter_map(edge)
        .filter(|e| e.kind == EdgeKind::Contains)
        .map(|e| e.to.clone())
        .collect();
    // Invocation edges touching the node, each counted once (a self-loop is
    // both outgoing and incoming).
    let degree = |n: &NodeId| {
        let out = adj.outgoing(n).iter().filter_map(edge);
        let inc = adj
            .incoming(n)
            .iter()
            .filter_map(edge)
            .filter(|e| &e.from != n);
        out.chain(inc)
            .filter(|e| INVOCATION.contains(&e.kind))
            .count()
    };
    let degrees: BTreeMap<NodeId, usize> =
        children.iter().map(|c| (c.clone(), degree(c))).collect();
    let deg = |n: &NodeId| degrees.get(n).copied().unwrap_or(0);
    children.sort_by(|a, b| deg(b).cmp(&deg(a)).then_with(|| a.cmp(b)));
    let truncated = limit > 0 && children.len() > limit;
    if truncated {
        children.truncate(limit);
    }
    let kids: BTreeSet<&NodeId> = children.iter().collect();
    let shown: BTreeSet<&NodeId> = visible.iter().collect();
    let place = |n: &NodeId| -> Option<NodeId> {
        if kids.contains(n)
            || shown.contains(n)
            || matches!(n, NodeId::Platform(_) | NodeId::Hole(_))
        {
            return Some(n.clone());
        }
        [graph.namespace_of(n), graph.unit_of(n)]
            .into_iter()
            .flatten()
            .find(|g| shown.contains(g))
    };
    // The children's edges, in stored order (as `expand` reads them).
    let mut touching: Vec<usize> = children
        .iter()
        .flat_map(|c| adj.outgoing(c).iter().chain(adj.incoming(c)).copied())
        .collect();
    touching.sort_unstable();
    touching.dedup();
    let mut groups: BTreeMap<(NodeId, NodeId, EdgeKind), Vec<&Edge>> = BTreeMap::new();
    for e in touching
        .iter()
        .filter_map(edge)
        .filter(|e| INVOCATION.contains(&e.kind) || e.kind == EdgeKind::Requires)
    {
        let (Some(from), Some(to)) = (place(&e.from), place(&e.to)) else {
            continue;
        };
        if from != to {
            groups.entry((from, to, e.kind)).or_default().push(e);
        }
    }
    let edges: Vec<Edge> = groups
        .into_iter()
        .map(|((from, to, kind), members)| match members.as_slice() {
            [one] if one.from == from && one.to == to => (*one).clone(),
            _ => roll(from, to, kind, &members),
        })
        .collect();
    let leaves: BTreeSet<NodeId> = edges
        .iter()
        .flat_map(|e: &Edge| [e.from.clone(), e.to.clone()])
        .filter(|n| matches!(n, NodeId::Platform(_) | NodeId::Hole(_)))
        .collect();
    GraphPayload {
        nodes: children
            .iter()
            .chain(leaves.iter())
            .map(|c| graph.group_node(c))
            .collect(),
        edges,
        truncated,
        aggregated: None,
    }
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
        // Interop exits and holes are leaves of the code being expanded.
        if kids.contains(n)
            || shown.contains(n)
            || matches!(n, NodeId::Platform(_) | NodeId::Hole(_))
        {
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
    let edges: Vec<Edge> = groups
        .into_iter()
        .map(|((from, to, kind), members)| match members.as_slice() {
            [one] if one.from == from && one.to == to => (*one).clone(),
            _ => roll(from, to, kind, &members),
        })
        .collect();
    let leaves: BTreeSet<NodeId> = edges
        .iter()
        .flat_map(|e: &Edge| [e.from.clone(), e.to.clone()])
        .filter(|n| matches!(n, NodeId::Platform(_) | NodeId::Hole(_)))
        .collect();
    GraphPayload {
        nodes: children
            .iter()
            .chain(leaves.iter())
            .map(|c| graph.group_node(c))
            .collect(),
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
    fn regex_search_matches_name_or_id_as_written() {
        let g = graph();
        let ids = |p: &Pattern| -> Vec<String> {
            search_filtered(&g, p, &SearchFilter::default(), 0)
                .0
                .iter()
                .map(|n| n.id.to_string())
                .collect()
        };
        let charge = ids(&Pattern::regex("^charge!$").unwrap());
        assert_eq!(charge.len(), 2, "{charge:?}");
        assert!(charge.iter().all(|id| id.ends_with("/charge!")));
        // Against the ID too, and case-sensitive unless the pattern says so.
        assert_eq!(
            ids(&Pattern::regex(r"orders\.core/charge").unwrap()).len(),
            1
        );
        assert!(ids(&Pattern::regex("CHARGE").unwrap()).is_empty());
        assert_eq!(ids(&Pattern::regex("(?i)^CHARGE!$").unwrap()).len(), 2);
        assert!(Pattern::regex("(unclosed").is_err());
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
    fn fit_reports_only_real_fallbacks() {
        let g = graph();
        let (p, coarser) = fit(
            &g,
            &Cache::default(),
            AggregationLevel::Namespace,
            &Filter::default(),
        );
        assert!(!coarser, "the requested level fits; no fallback");
        assert!(!p.truncated);
        let mut big = Graph::default();
        for i in 0..(MAX_NODES + 1) {
            let mut n = node(&format!("clj:.//big/f{i}"));
            n.namespace = Some("big".into());
            n.unit = Some(NodeId::from_str("unit:.").unwrap());
            big.nodes.insert(n.id.clone(), n);
        }
        let (p, coarser) = fit(
            &big,
            &Cache::default(),
            AggregationLevel::Symbol,
            &Filter::default(),
        );
        assert!(coarser, "too many symbols: shown at a coarser level");
        assert!(p.nodes.len() <= MAX_NODES);
    }

    #[test]
    fn fit_keeps_small_graphs_and_cap_trims_big_ones() {
        let g = graph();
        let (p, coarser) = fit(
            &g,
            &Cache::default(),
            AggregationLevel::Symbol,
            &Filter::default(),
        );
        assert!(!p.truncated);
        assert!(!coarser);
        assert_eq!(p.aggregated, None);
        let mut big = GraphPayload::default();
        for i in 0..(MAX_NODES + 5) {
            big.nodes.push(node(&format!("clj:.//n/x{i}")));
        }
        let capped = cap(big);
        assert!(capped.truncated);
        assert_eq!(capped.nodes.len(), MAX_NODES);
    }

    /// Two namespaces in one unit: `orders.core/total` calls
    /// `billing.core/charge!`, and `orders.core` requires `billing.core`.
    fn two_namespaces() -> Graph {
        let mut g = Graph::default();
        let mut add = |id: &str, ns: Option<&str>| {
            let mut n = node(id);
            n.namespace = ns.map(Into::into);
            n.unit = Some(NodeId::from_str("unit:.").unwrap());
            g.nodes.insert(n.id.clone(), n);
        };
        add("unit:.", None);
        add("clj:.//orders.core", None);
        add("clj:.//billing.core", None);
        add("clj:.//orders.core/total", Some("orders.core"));
        add("clj:.//orders.core/charge!", Some("orders.core"));
        add("clj:.//billing.core/charge!", Some("billing.core"));
        g.edges = vec![
            edge("unit:.", "clj:.//orders.core", EdgeKind::Contains, None),
            edge("unit:.", "clj:.//billing.core", EdgeKind::Contains, None),
            edge(
                "clj:.//orders.core",
                "clj:.//orders.core/total",
                EdgeKind::Contains,
                None,
            ),
            edge(
                "clj:.//orders.core",
                "clj:.//orders.core/charge!",
                EdgeKind::Contains,
                None,
            ),
            edge(
                "clj:.//billing.core",
                "clj:.//billing.core/charge!",
                EdgeKind::Contains,
                None,
            ),
            edge(
                "clj:.//orders.core",
                "clj:.//billing.core",
                EdgeKind::Requires,
                None,
            ),
            edge(
                "clj:.//orders.core/total",
                "clj:.//billing.core/charge!",
                EdgeKind::Calls,
                Some(Certainty::Certain),
            ),
        ];
        g
    }

    #[test]
    fn edges_differing_only_by_language_are_one_neighbor() {
        let mut g = graph();
        let total = "clj:services/orders//orders.core/total";
        let charge = "clj:services/orders//orders.core/charge!";
        let mut cljs = edge(total, charge, EdgeKind::Calls, Some(Certainty::Certain));
        cljs.lang = Some(aldente_model::Lang::Cljs);
        g.edges.push(cljs);
        let (callers, _) = neighbors(&g, &Cache::default(), &NodeId::from_str(charge).unwrap());
        assert_eq!(callers.len(), 1, "{callers:?}");
    }

    #[test]
    fn expanding_keeps_interop_exits_and_holes() {
        let mut g = two_namespaces();
        let exit = node("java:java.lang.System#currentTimeMillis");
        g.nodes.insert(exit.id.clone(), exit);
        g.edges.push(edge(
            "clj:.//orders.core/total",
            "java:java.lang.System#currentTimeMillis",
            EdgeKind::Calls,
            Some(Certainty::Certain),
        ));
        let p = expand(
            &g,
            &NodeId::from_str("clj:.//orders.core").unwrap(),
            &[],
            10,
        );
        assert!(
            p.nodes
                .iter()
                .any(|n| n.id.to_string() == "java:java.lang.System#currentTimeMillis")
        );
        assert!(
            p.edges
                .iter()
                .any(|e| e.to.to_string() == "java:java.lang.System#currentTimeMillis")
        );
    }

    #[test]
    fn aggregates_list_their_children() {
        let g = two_namespaces();
        let ns = NodeId::from_str("clj:.//orders.core").unwrap();
        let names: Vec<String> = children(&g, &ns).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["charge!", "total"]);
        let unit = NodeId::from_str("unit:.").unwrap();
        assert_eq!(children(&g, &unit).len(), 2);
        let leaf = NodeId::from_str("clj:.//orders.core/total").unwrap();
        assert!(children(&g, &leaf).is_empty());
    }

    #[test]
    fn namespaces_get_rolled_up_neighbors_with_counts() {
        let g = two_namespaces();
        let orders = NodeId::from_str("clj:.//orders.core").unwrap();
        let (callers, callees) = neighbors(&g, &Cache::default(), &orders);
        assert!(callers.is_empty());
        let kinds: Vec<(String, EdgeKind, Option<u64>)> = callees
            .iter()
            .map(|n| (n.id.to_string(), n.kind, n.count))
            .collect();
        assert!(
            kinds.contains(&("clj:.//billing.core".into(), EdgeKind::Calls, Some(1))),
            "{kinds:?}"
        );
        assert!(
            kinds.contains(&("clj:.//billing.core".into(), EdgeKind::Requires, None)),
            "{kinds:?}"
        );
        let billing = NodeId::from_str("clj:.//billing.core").unwrap();
        let (callers, _) = neighbors(&g, &Cache::default(), &billing);
        assert_eq!(callers.len(), 2);
        assert_eq!(callers[0].name, "orders.core");
    }

    #[test]
    fn neighbors_and_neighborhoods() {
        let g = graph();
        let charge = NodeId::from_str("clj:services/orders//orders.core/charge!").unwrap();
        let (callers, callees) = neighbors(&g, &Cache::default(), &charge);
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
