//! Projections: the weighted, integer-indexed graphs measures run on.
//!
//! Parallel edges are merged. At symbol level a pair's weight is the
//! strongest edge between them and its count the number of edges; at
//! namespace and unit level a pair's weight is the sum of the merged
//! symbol-level weights it rolls up and its count the number of symbol
//! pairs. Self-loops (recursion, calls inside one namespace) are dropped.

use std::collections::{BTreeMap, HashMap, HashSet};

use aldente_graph::Graph;
use aldente_model::measures::{ProjectionKind, ProjectionSpec};
use aldente_model::{
    AggregationLevel, EdgeKind, Node, NodeId, NodeKind, SymbolFlag, SymbolSubkind,
};
use thiserror::Error;

/// A node's position in a projection.
pub type Ix = u32;

/// One merged edge seen from one end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    pub other: Ix,
    pub weight: f64,
    pub count: u32,
}

/// Why a projection cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProjectionError {
    #[error(
        "the structure projection has no symbol level (requires and depends join namespaces and units)"
    )]
    StructureAtSymbolLevel,
}

/// A weighted directed graph in CSR form, nodes sorted by ID.
#[derive(Debug, Clone)]
pub struct Projection {
    ids: Vec<NodeId>,
    out_start: Vec<usize>,
    out: Vec<Arc>,
    in_start: Vec<usize>,
    inc: Vec<Arc>,
}

impl Projection {
    /// Builds a projection from node IDs and `(from, to, weight, count)`
    /// arcs over their positions; parallel arcs are summed, loops dropped.
    /// Nodes are re-sorted by ID.
    ///
    /// Example: `from_arcs(vec![a, b], &[(0, 1, 1.0, 1)])` has one arc `a → b`.
    pub fn from_arcs(ids: Vec<NodeId>, arcs: &[(Ix, Ix, f64, u32)]) -> Projection {
        let mut order: Vec<usize> = (0..ids.len()).collect();
        order.sort_by(|&a, &b| ids[a].cmp(&ids[b]));
        let mut new_ix = vec![0 as Ix; ids.len()];
        for (new, &old) in order.iter().enumerate() {
            new_ix[old] = new as Ix;
        }
        let mut merged: BTreeMap<(Ix, Ix), (f64, u32)> = BTreeMap::new();
        for &(f, t, w, c) in arcs {
            let (f, t) = (new_ix[f as usize], new_ix[t as usize]);
            if f == t {
                continue;
            }
            let e = merged.entry((f, t)).or_insert((0.0, 0));
            e.0 += w;
            e.1 += c;
        }
        let sorted_ids: Vec<NodeId> = order.iter().map(|&o| ids[o].clone()).collect();
        let n = sorted_ids.len();
        let (out_start, out) = csr(n, merged.iter().map(|(&(f, t), &(w, c))| (f, t, w, c)));
        let (in_start, inc) = csr(n, merged.iter().map(|(&(f, t), &(w, c))| (t, f, w, c)));
        Projection {
            ids: sorted_ids,
            out_start,
            out,
            in_start,
            inc,
        }
    }

    /// Number of nodes.
    ///
    /// Example: a projection of `a → b` has `len() == 2`.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether there are no nodes.
    ///
    /// Example: `Projection::from_arcs(vec![], &[]).is_empty()` is `true`.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Number of merged arcs.
    ///
    /// Example: a projection of `a → b` has `arc_count() == 1`.
    pub fn arc_count(&self) -> usize {
        self.out.len()
    }

    /// The node at `i`.
    ///
    /// Example: `p.id(0)` is the smallest ID.
    pub fn id(&self, i: Ix) -> &NodeId {
        &self.ids[i as usize]
    }

    /// Every node ID, sorted.
    ///
    /// Example: `p.ids()[0]` is `p.id(0)`.
    pub fn ids(&self) -> &[NodeId] {
        &self.ids
    }

    /// Arcs leaving `i`, by target.
    ///
    /// Example: for `a → b`, `outgoing(a)` holds one arc with `other == b`.
    pub fn outgoing(&self, i: Ix) -> &[Arc] {
        &self.out[self.out_start[i as usize]..self.out_start[i as usize + 1]]
    }

    /// Arcs arriving at `i`, by source.
    ///
    /// Example: for `a → b`, `incoming(b)` holds one arc with `other == a`.
    pub fn incoming(&self, i: Ix) -> &[Arc] {
        &self.inc[self.in_start[i as usize]..self.in_start[i as usize + 1]]
    }

    /// The same graph with every arc reversed.
    ///
    /// Example: the reverse of `a → b` is `b → a`.
    pub fn reversed(&self) -> Projection {
        Projection {
            ids: self.ids.clone(),
            out_start: self.in_start.clone(),
            out: self.inc.clone(),
            in_start: self.out_start.clone(),
            inc: self.out.clone(),
        }
    }

    /// Undirected simple neighbor lists (no weights), sorted.
    ///
    /// Example: for `a → b` and `b → a`, `a`'s neighbors are `[b]`.
    pub fn undirected(&self) -> Vec<Vec<Ix>> {
        (0..self.len() as Ix)
            .map(|i| {
                let mut v: Vec<Ix> = self
                    .outgoing(i)
                    .iter()
                    .chain(self.incoming(i))
                    .map(|a| a.other)
                    .collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect()
    }
}

/// CSR arrays from arcs sorted by source.
///
/// Example: `csr(2, [(0, 1, 1.0, 1)])` gives starts `[0, 1, 1]`.
fn csr(n: usize, arcs: impl Iterator<Item = (Ix, Ix, f64, u32)>) -> (Vec<usize>, Vec<Arc>) {
    let mut by: Vec<Vec<Arc>> = vec![Vec::new(); n];
    for (f, t, weight, count) in arcs {
        by[f as usize].push(Arc {
            other: t,
            weight,
            count,
        });
    }
    let mut start = Vec::with_capacity(n + 1);
    let mut flat = Vec::new();
    for mut list in by {
        start.push(flat.len());
        list.sort_by_key(|a| a.other);
        flat.extend(list);
    }
    start.push(flat.len());
    (start, flat)
}

/// Whether a node is itself a test (R14): a `deftest`-like var or a
/// top-level test form, not merely code in a test namespace.
///
/// Example: `app.core-test/run-works` (a `deftest`) is; the helper
/// `app.core-test/make` in the same namespace is not.
pub fn is_test_itself(node: &Node) -> bool {
    node.flags.contains(&SymbolFlag::Test) || node.subkind == Some(SymbolSubkind::Test)
}

/// Whether a node is test code: flagged or of subkind `test`, or in a test
/// namespace.
///
/// Example: a `deftest` var is test code; `shop.core/checkout` is not.
pub fn is_test(graph: &Graph, node: &Node) -> bool {
    is_test_itself(node)
        || graph
            .namespace_of(&node.id)
            .filter(|ns| ns != &node.id)
            .and_then(|ns| graph.nodes.get(&ns))
            .is_some_and(|ns| ns.flags.contains(&SymbolFlag::Test))
}

/// Whether an ID names code outside the repository (libraries, Java, JS).
///
/// Example: `ext:clj//clojure.core/map` is external; `clj:.//a/f` is not.
fn is_external(id: &NodeId) -> bool {
    matches!(
        id.kind(),
        NodeKind::External | NodeKind::Platform | NodeKind::Library
    )
}

/// Builds the projection `spec` describes over `graph`.
///
/// Example: for clj-mini, the default spec (invocation, symbol level, no
/// tests, no externals) has a node per non-test function and impl.
pub fn project<'g>(graph: &'g Graph, spec: &ProjectionSpec) -> Result<Projection, ProjectionError> {
    let kinds: &[EdgeKind] = match spec.kind {
        ProjectionKind::Invocation => &[
            EdgeKind::Calls,
            EdgeKind::References,
            EdgeKind::Dispatches,
            EdgeKind::Registers,
        ],
        ProjectionKind::Structure => match spec.level {
            AggregationLevel::Symbol => return Err(ProjectionError::StructureAtSymbolLevel),
            AggregationLevel::Namespace => &[EdgeKind::Requires],
            AggregationLevel::Unit => &[EdgeKind::Depends],
        },
    };
    let tests: HashSet<&NodeId> = if spec.include_tests {
        HashSet::new()
    } else {
        graph
            .nodes
            .iter()
            .filter(|(_, n)| is_test(graph, n))
            .map(|(id, _)| id)
            .collect()
    };
    // The node an endpoint stands for at this level, or None to drop it.
    let group = |id: &NodeId| -> Option<NodeId> {
        if is_external(id) {
            if !spec.include_externals {
                return None;
            }
            return match spec.level {
                AggregationLevel::Symbol => Some(id.clone()),
                _ => graph.namespace_of(id).or_else(|| Some(id.clone())),
            };
        }
        if tests.contains(id) {
            return None;
        }
        match spec.level {
            AggregationLevel::Symbol => {
                matches!(id.kind(), NodeKind::Symbol | NodeKind::Impl).then(|| id.clone())
            }
            AggregationLevel::Namespace => graph
                .namespace_of(id)
                .filter(|ns| ns.kind() == NodeKind::Namespace),
            AggregationLevel::Unit => graph.unit_of(id),
        }
    };
    let level_kind = match spec.level {
        AggregationLevel::Symbol => None,
        AggregationLevel::Namespace => Some(NodeKind::Namespace),
        AggregationLevel::Unit => Some(NodeKind::Unit),
    };
    let mut ix: HashMap<NodeId, Ix> = HashMap::new();
    let mut ids: Vec<NodeId> = Vec::new();
    let mut add = |id: NodeId, ids: &mut Vec<NodeId>| -> Ix {
        *ix.entry(id.clone()).or_insert_with(|| {
            ids.push(id);
            (ids.len() - 1) as Ix
        })
    };
    // Every node of the level is present, connected or not.
    for (id, node) in &graph.nodes {
        let of_level = match level_kind {
            None => matches!(node.kind, NodeKind::Symbol | NodeKind::Impl),
            Some(k) => node.kind == k,
        };
        if of_level && !tests.contains(id) {
            add(id.clone(), &mut ids);
        }
    }
    // Each endpoint is resolved to its group once: `raw` numbers endpoints,
    // `group_of[raw]` is the group's position (None: dropped).
    let mut raw: HashMap<&'g NodeId, u32> = HashMap::new();
    let mut group_of: Vec<Option<Ix>> = Vec::new();
    let mut endpoint = |id: &'g NodeId, ids: &mut Vec<NodeId>| -> (u32, Option<Ix>) {
        if let Some(&r) = raw.get(id) {
            return (r, group_of[r as usize]);
        }
        let g = group(id).map(|g| add(g, ids));
        let r = group_of.len() as u32;
        group_of.push(g);
        raw.insert(id, r);
        (r, g)
    };
    // Endpoint pairs first, strongest edge and edge count per pair.
    let mut pairs: HashMap<(u32, u32), (Ix, Ix, f64, u32)> = HashMap::new();
    for e in &graph.edges {
        if !kinds.contains(&e.kind) || e.certainty.is_some_and(|c| c < spec.min_certainty) {
            continue;
        }
        let (rf, gf) = endpoint(&e.from, &mut ids);
        let (rt, gt) = endpoint(&e.to, &mut ids);
        let (Some(gf), Some(gt)) = (gf, gt) else {
            continue;
        };
        let w = spec.weights.of(e.certainty);
        let p = pairs.entry((rf, rt)).or_insert((gf, gt, 0.0, 0));
        p.2 = p.2.max(w);
        p.3 += 1;
    }
    let symbol_level = spec.level == AggregationLevel::Symbol;
    let mut arcs: Vec<(Ix, Ix, f64, u32)> = pairs
        .into_values()
        .map(|(f, t, w, c)| (f, t, w, if symbol_level { c } else { 1 }))
        .collect();
    // Merge order must not depend on hash order: sum in a fixed order.
    arcs.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)).then(a.2.total_cmp(&b.2)));
    Ok(Projection::from_arcs(ids, &arcs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn id(s: &str) -> NodeId {
        NodeId::from_str(s).unwrap()
    }

    #[test]
    fn from_arcs_sorts_merges_and_drops_loops() {
        let p = Projection::from_arcs(
            vec![id("clj:.//b/g"), id("clj:.//a/f")],
            &[(1, 0, 1.0, 1), (1, 0, 0.5, 2), (0, 0, 1.0, 1)],
        );
        assert_eq!(p.id(0), &id("clj:.//a/f"));
        assert_eq!(p.arc_count(), 1);
        assert_eq!(
            p.outgoing(0),
            &[Arc {
                other: 1,
                weight: 1.5,
                count: 3
            }]
        );
        assert_eq!(p.incoming(1)[0].other, 0);
        assert_eq!(p.reversed().outgoing(1)[0].other, 0);
        assert_eq!(p.undirected(), vec![vec![1], vec![0]]);
    }
}
