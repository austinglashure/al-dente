//! A compact, integer-indexed copy of the graph's adjacency for fast
//! traversal. Built once per graph; craters run over it.

use std::collections::HashMap;

use aldente_graph::Graph;
use aldente_model::{Certainty, EdgeKind, NodeId, NodeKind, SymbolFlag, SymbolSubkind};

/// A node's position in the index.
pub type Ix = u32;

/// Degree at or above which a node is never a hub, however large the graph,
/// unless set explicitly. Keeps the "top 0.1 percent" rule from calling a
/// node with a handful of edges a hub.
const MIN_HUB_DEGREE: u32 = 32;

/// One edge seen from one end: the node at the other end and the edge's
/// position in `Graph::edges`.
#[derive(Debug, Clone, Copy)]
pub struct Arc {
    pub other: Ix,
    pub edge: u32,
}

/// What the search needs to know about a node without touching its `Node`.
#[derive(Debug, Clone, Copy)]
pub struct NodeInfo {
    pub kind: NodeKind,
    pub test: bool,
    /// Index of the node's unit, if it has one.
    pub unit: Option<Ix>,
}

/// What the search needs to know about an edge.
#[derive(Debug, Clone, Copy)]
pub struct EdgeInfo {
    pub kind: EdgeKind,
    /// Structural edges without a certainty count as certain.
    pub certainty: Certainty,
}

/// Compressed forward and reverse adjacency (CSR) over `Graph::edges`.
#[derive(Debug, Clone)]
pub struct CraterIndex {
    ids: Vec<NodeId>,
    ix: HashMap<NodeId, Ix>,
    nodes: Vec<NodeInfo>,
    edges: Vec<EdgeInfo>,
    out_start: Vec<u32>,
    out: Vec<Arc>,
    in_start: Vec<u32>,
    inc: Vec<Arc>,
    hub_threshold: u32,
}

impl CraterIndex {
    /// Indexes every node of `graph` and every edge endpoint (endpoints
    /// without a node record get kind from their ID).
    ///
    /// Example: for a graph `a -calls-> b`, `outgoing(ix(a))` has one arc to
    /// `ix(b)`.
    pub fn new(graph: &Graph) -> CraterIndex {
        let mut ids: Vec<NodeId> = graph.nodes.keys().cloned().collect();
        let mut ix: HashMap<NodeId, Ix> = HashMap::with_capacity(ids.len());
        for (i, id) in ids.iter().enumerate() {
            ix.insert(id.clone(), i as Ix);
        }
        for e in &graph.edges {
            for id in [&e.from, &e.to] {
                if !ix.contains_key(id) {
                    ix.insert(id.clone(), ids.len() as Ix);
                    ids.push(id.clone());
                }
            }
        }
        let nodes = ids
            .iter()
            .map(|id| node_info(graph, &ix, id))
            .collect::<Vec<_>>();
        let edges = graph
            .edges
            .iter()
            .map(|e| EdgeInfo {
                kind: e.kind,
                certainty: e.certainty.unwrap_or(Certainty::Certain),
            })
            .collect::<Vec<_>>();
        let pairs: Vec<(Ix, Ix)> = graph
            .edges
            .iter()
            .map(|e| (ix[&e.from], ix[&e.to]))
            .collect();
        let (out_start, out) = csr(ids.len(), pairs.iter().map(|&(f, t)| (f, t)));
        let (in_start, inc) = csr(ids.len(), pairs.iter().map(|&(f, t)| (t, f)));
        let degrees: Vec<u32> = (0..ids.len())
            .map(|i| (out_start[i + 1] - out_start[i]) + (in_start[i + 1] - in_start[i]))
            .collect();
        let hub_threshold = default_hub_threshold(&degrees);
        CraterIndex {
            ids,
            ix,
            nodes,
            edges,
            out_start,
            out,
            in_start,
            inc,
            hub_threshold,
        }
    }

    /// The index of `id`, if the graph has it.
    ///
    /// Example: `index.ix(&a)` is `Some(0)` for the first node by ID.
    pub fn ix(&self, id: &NodeId) -> Option<Ix> {
        self.ix.get(id).copied()
    }

    /// The ID at index `i`.
    ///
    /// Example: `index.id(index.ix(&a)?)` is `&a`.
    pub fn id(&self, i: Ix) -> &NodeId {
        &self.ids[i as usize]
    }

    /// Number of indexed nodes.
    ///
    /// Example: 2 for `a -calls-> b`.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the index has no nodes.
    ///
    /// Example: `true` for an empty graph.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Facts about node `i`.
    ///
    /// Example: `index.node(i).kind` is `NodeKind::Symbol` for a function.
    pub fn node(&self, i: Ix) -> NodeInfo {
        self.nodes[i as usize]
    }

    /// Facts about the edge at position `e` of `Graph::edges`.
    ///
    /// Example: `index.edge(0).kind` is `EdgeKind::Calls` for `a -calls-> b`.
    pub fn edge(&self, e: u32) -> EdgeInfo {
        self.edges[e as usize]
    }

    /// Edges leaving node `i`.
    ///
    /// Example: see [`CraterIndex::new`].
    pub fn outgoing(&self, i: Ix) -> &[Arc] {
        let i = i as usize;
        &self.out[self.out_start[i] as usize..self.out_start[i + 1] as usize]
    }

    /// Edges arriving at node `i`.
    ///
    /// Example: `incoming(ix(b))` has one arc to `ix(a)` for `a -calls-> b`.
    pub fn incoming(&self, i: Ix) -> &[Arc] {
        let i = i as usize;
        &self.inc[self.in_start[i] as usize..self.in_start[i + 1] as usize]
    }

    /// Edges in and out of node `i`.
    ///
    /// Example: 1 for both ends of `a -calls-> b`.
    pub fn degree(&self, i: Ix) -> u32 {
        (self.outgoing(i).len() + self.incoming(i).len()) as u32
    }

    /// The default hub threshold: the degree of the node at the top 0.1
    /// percent, but never below [`MIN_HUB_DEGREE`]; `u32::MAX` when the graph
    /// has under 1,000 nodes.
    ///
    /// Example: in a 2,000-node graph whose two busiest nodes have degree 500
    /// and 40, the threshold is 40.
    pub fn hub_threshold(&self) -> u32 {
        self.hub_threshold
    }
}

/// Builds a CSR layout from `(node, other)` pairs, keeping edge positions.
///
/// Example: `csr(2, [(0, 1)])` gives starts `[0, 1, 1]` and one arc to 1.
fn csr(n: usize, pairs: impl Iterator<Item = (Ix, Ix)> + Clone) -> (Vec<u32>, Vec<Arc>) {
    let mut start = vec![0u32; n + 1];
    for (a, _) in pairs.clone() {
        start[a as usize + 1] += 1;
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut arcs = vec![Arc { other: 0, edge: 0 }; start[n] as usize];
    for (e, (a, b)) in pairs.enumerate() {
        let slot = &mut fill[a as usize];
        arcs[*slot as usize] = Arc {
            other: b,
            edge: e as u32,
        };
        *slot += 1;
    }
    (start, arcs)
}

/// See [`CraterIndex::hub_threshold`].
///
/// Example: `default_hub_threshold(&[1; 10])` is `u32::MAX`.
fn default_hub_threshold(degrees: &[u32]) -> u32 {
    let k = degrees.len() / 1000;
    if k == 0 {
        return u32::MAX;
    }
    let mut sorted = degrees.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    sorted[k - 1].max(MIN_HUB_DEGREE)
}

/// Kind, test flag and unit of `id`.
///
/// Example: a test var gives `NodeInfo { kind: Symbol, test: true, .. }`.
fn node_info(graph: &Graph, ix: &HashMap<NodeId, Ix>, id: &NodeId) -> NodeInfo {
    match graph.nodes.get(id) {
        Some(n) => NodeInfo {
            kind: n.kind,
            test: n.flags.contains(&SymbolFlag::Test) || n.subkind == Some(SymbolSubkind::Test),
            unit: n.unit.as_ref().and_then(|u| ix.get(u).copied()),
        },
        None => NodeInfo {
            kind: id.kind(),
            test: false,
            unit: None,
        },
    }
}
