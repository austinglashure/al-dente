//! Graph payloads at each aggregation level (`03-graph-model.md`,
//! "Aggregation levels").
//!
//! - `symbol`: every stored node and edge.
//! - `namespace`: namespaces, namespace-level externals, platforms and holes;
//!   `requires` as stored, plus `calls`, `references` and `dispatches`
//!   rolled up between namespaces (self-loops dropped).
//! - `unit`: units; invocation edges rolled up between different units.
//!   Externals have no library until Phase 2, so they are left out.
//!
//! Unit and namespace nodes carry `members`, how many nodes they contain.
//!
//! Rolled-up edges carry `count`, `by_certainty`, the strongest certainty,
//! and the most frequent reason among their members (ties: the first in
//! `Reason` order), so they still satisfy the edge invariants.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use aldente_model::{
    Aggregation, AggregationLevel, Certainty, Edge, EdgeKind, ExternalId, GraphPayload, Lang,
    NamespaceId, Node, NodeId, NodeKind, Reason,
};

use crate::build::Graph;

/// The most nodes one payload may carry (`02-architecture.md`, "Performance targets").
pub const MAX_NODES: usize = 2000;
/// The most edges one payload may carry.
pub const MAX_EDGES: usize = 10_000;

/// Edge kinds that are invocation (rolled up between groups).
const INVOCATION: &[EdgeKind] = &[EdgeKind::Calls, EdgeKind::References, EdgeKind::Dispatches];

impl Graph {
    /// The payload at `level`, sorted.
    ///
    /// Example: clj-mini at `Namespace` gives 13 namespace nodes plus
    /// externals, with `shop.core -calls-> shop.pricing` (count ≥ 1).
    pub fn payload(&self, level: AggregationLevel) -> GraphPayload {
        match level {
            AggregationLevel::Symbol => GraphPayload {
                nodes: self.nodes.values().cloned().collect(),
                edges: self.edges.clone(),
                truncated: false,
                aggregated: None,
            },
            AggregationLevel::Namespace => self.rolled(level, |g, id| g.namespace_of(id)),
            AggregationLevel::Unit => self.rolled(level, |g, id| g.unit_of(id)),
        }
    }

    /// Groups nodes with `group`, keeps `requires` at namespace level, and
    /// rolls up invocation edges between different groups.
    ///
    /// Example: see [`Graph::payload`].
    fn rolled(
        &self,
        level: AggregationLevel,
        group: impl Fn(&Graph, &NodeId) -> Option<NodeId>,
    ) -> GraphPayload {
        // Groups are interned as indices: an edge's group key is two
        // integers, not two cloned IDs (2,000,000 edges made that the cost).
        let mut ids: Vec<NodeId> = Vec::new();
        let mut index: HashMap<NodeId, u32> = HashMap::new();
        let mut group_of: HashMap<&NodeId, Option<u32>> = HashMap::new();
        let mut intern = |g: Option<NodeId>| -> Option<u32> {
            let g = g?;
            Some(*index.entry(g).or_insert_with_key(|g| {
                ids.push(g.clone());
                (ids.len() - 1) as u32
            }))
        };
        let mut members: HashMap<(u32, u32, EdgeKind), Vec<&Edge>> = HashMap::new();
        let mut kept: Vec<Edge> = Vec::new();
        let mut used: Vec<bool> = Vec::new();
        for edge in &self.edges {
            // Kept as stored: `requires` between namespaces, and between
            // units the `:local/root` links (`depends`).
            let requires = (edge.kind == EdgeKind::Requires
                && level == AggregationLevel::Namespace)
                || (edge.kind == EdgeKind::Depends && level == AggregationLevel::Unit);
            if !requires && !INVOCATION.contains(&edge.kind) {
                continue;
            }
            let from = *group_of
                .entry(&edge.from)
                .or_insert_with(|| intern(group(self, &edge.from)));
            let to = *group_of
                .entry(&edge.to)
                .or_insert_with(|| intern(group(self, &edge.to)));
            let (Some(from), Some(to)) = (from, to) else {
                continue;
            };
            if requires {
                kept.push(edge.clone());
            } else if from != to {
                members.entry((from, to, edge.kind)).or_default().push(edge);
            } else {
                continue;
            }
            for i in [from, to] {
                let i = i as usize;
                if used.len() <= i {
                    used.resize(i + 1, false);
                }
                used[i] = true;
            }
        }
        let mut nodes: HashMap<NodeId, Node> = ids
            .iter()
            .zip(used.iter().chain(std::iter::repeat(&false)))
            .filter(|(_, u)| **u)
            .map(|(id, _)| (id.clone(), self.group_node(id)))
            .collect();
        for node in self.nodes.values() {
            let wanted = match level {
                AggregationLevel::Namespace => node.kind == NodeKind::Namespace,
                _ => node.kind == NodeKind::Unit,
            };
            if wanted {
                nodes.entry(node.id.clone()).or_insert_with(|| node.clone());
            }
        }
        let mut edges: Vec<Edge> = kept;
        edges.extend(members.into_iter().map(|((from, to, kind), es)| {
            roll(
                ids[from as usize].clone(),
                ids[to as usize].clone(),
                kind,
                &es,
            )
        }));
        edges.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
        let mut nodes: Vec<Node> = nodes.into_values().collect();
        self.set_members(&mut nodes);
        nodes.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        GraphPayload {
            nodes,
            edges,
            truncated: false,
            aggregated: Some(Aggregation { level }),
        }
    }

    /// The namespace-level node a node rolls up into: its namespace (from
    /// `namespace` and `unit`), the namespace-level external for externals,
    /// itself for namespaces, platforms and holes (blind spots stay visible
    /// when zoomed out); units have none.
    ///
    /// Example: `clj:.//shop.core/checkout` gives `clj:.//shop.core`;
    /// `ext:clj//clojure.core/map` gives `ext:clj//clojure.core`.
    pub fn namespace_of(&self, id: &NodeId) -> Option<NodeId> {
        match id {
            NodeId::Namespace(_) | NodeId::Platform(_) | NodeId::Hole(_) => Some(id.clone()),
            NodeId::External(ext) => ExternalId::namespace(ext.lang(), ext.ns())
                .ok()
                .map(NodeId::External),
            // A symbol's namespace is in its ID: no lookup in the node map,
            // which dominated rollups on big graphs (pinned to the stored
            // fields by a test). An impl's ID names the method's namespace,
            // not the one its `defmethod` is in, so impls are looked up.
            NodeId::Symbol(s) => {
                let ns = s.namespace();
                NamespaceId::new(Lang::Clj, ns.unit().clone(), ns.ns())
                    .ok()
                    .map(NodeId::Namespace)
            }
            NodeId::Impl(_) => {
                let node = self.nodes.get(id)?;
                let NodeId::Unit(dir) = node.unit.as_ref()? else {
                    return None;
                };
                NamespaceId::new(Lang::Clj, dir.clone(), node.namespace.as_deref()?)
                    .ok()
                    .map(NodeId::Namespace)
            }
            _ => None,
        }
    }

    /// The unit a node belongs to; externals and platforms have none yet.
    ///
    /// Example: `clj:services/orders//orders.core/total` gives `unit:services/orders`.
    pub fn unit_of(&self, id: &NodeId) -> Option<NodeId> {
        match id {
            NodeId::Unit(_) => Some(id.clone()),
            // Read from the ID, as in `namespace_of` (edge ends are always
            // nodes; a test pins both).
            NodeId::Namespace(ns) => Some(NodeId::Unit(ns.unit().clone())),
            NodeId::Symbol(s) => Some(NodeId::Unit(s.namespace().unit().clone())),
            _ => self.nodes.get(id)?.unit.clone(),
        }
    }

    /// Sets `members` on the unit and namespace nodes: how many nodes each
    /// contains directly.
    ///
    /// Example: a namespace with functions `f` and `g` gets `members: Some(2)`.
    pub(crate) fn set_members(&self, nodes: &mut [Node]) {
        let mut contained: HashMap<&NodeId, u64> = HashMap::new();
        for e in self.edges.iter().filter(|e| e.kind == EdgeKind::Contains) {
            *contained.entry(&e.from).or_default() += 1;
        }
        let units = nodes.iter().any(|n| n.kind == NodeKind::Unit);
        let mut symbols: HashMap<NodeId, u64> = HashMap::new();
        if units {
            for n in self
                .nodes
                .values()
                .filter(|n| matches!(n.kind, NodeKind::Symbol | NodeKind::Impl))
            {
                if let Some(u) = self.unit_of(&n.id) {
                    *symbols.entry(u).or_default() += 1;
                }
            }
        }
        for n in nodes {
            if matches!(n.kind, NodeKind::Namespace | NodeKind::Unit) {
                n.members = Some(contained.get(&n.id).copied().unwrap_or(0));
            }
            if n.kind == NodeKind::Unit {
                n.symbols = Some(symbols.get(&n.id).copied().unwrap_or(0));
            }
        }
    }

    /// The node for a group ID: the stored node if there is one, otherwise a
    /// synthesized namespace-level external.
    ///
    /// Example: `ext:clj//clojure.core` gives an external node named `clojure.core`.
    pub(crate) fn group_node(&self, id: &NodeId) -> Node {
        self.nodes.get(id).cloned().unwrap_or_else(|| Node {
            id: id.clone(),
            kind: id.kind(),
            subkind: None,
            name: match id {
                NodeId::External(ext) => ext.ns().to_owned(),
                other => other.to_string(),
            },
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
            members: None,
            symbols: None,
        })
    }
}

/// One rolled-up edge from its members.
///
/// Example: three `calls` edges (2 certain, 1 likely) give `count: 3`,
/// `certainty: certain`, `by_certainty: {certain: 2, likely: 1}`.
pub(crate) fn roll(from: NodeId, to: NodeId, kind: EdgeKind, members: &[&Edge]) -> Edge {
    let mut by_certainty: BTreeMap<Certainty, u64> = BTreeMap::new();
    let mut reasons: BTreeMap<Reason, u64> = BTreeMap::new();
    for e in members {
        if let Some(c) = e.certainty {
            *by_certainty.entry(c).or_default() += 1;
        }
        if let Some(r) = e.reason {
            *reasons.entry(r).or_default() += 1;
        }
    }
    let reason = reasons
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map(|(r, _)| *r);
    Edge {
        from,
        to,
        kind,
        certainty: by_certainty.keys().next_back().copied(),
        reason,
        lang: None,
        sites: Vec::new(),
        count: Some(members.len() as u64),
        by_certainty: Some(by_certainty),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{RepoPath, Site};
    use std::str::FromStr;

    fn id(s: &str) -> NodeId {
        NodeId::from_str(s).unwrap()
    }

    fn node(s: &str, kind: NodeKind, ns: Option<&str>) -> Node {
        Node {
            id: id(s),
            kind,
            subkind: None,
            name: s.into(),
            namespace: ns.map(Into::into),
            unit: Some(id("unit:.")),
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
            members: None,
            symbols: None,
        }
    }

    fn edge(from: &str, to: &str, kind: EdgeKind, c: Certainty) -> Edge {
        Edge {
            from: id(from),
            to: id(to),
            kind,
            certainty: Some(c),
            reason: Some(Reason::Direct),
            lang: None,
            sites: vec![Site {
                file: RepoPath::parse("a.clj").unwrap(),
                line: 1,
                col: 1,
            }],
            count: None,
            by_certainty: None,
        }
    }

    fn graph() -> Graph {
        let mut g = Graph::default();
        for n in [
            node("unit:.", NodeKind::Unit, None),
            node("clj:.//a", NodeKind::Namespace, None),
            node("clj:.//b", NodeKind::Namespace, None),
            node("clj:.//a/f", NodeKind::Symbol, Some("a")),
            node("clj:.//a/g", NodeKind::Symbol, Some("a")),
            node("clj:.//b/h", NodeKind::Symbol, Some("b")),
            node(
                "ext:clj//clojure.core/map",
                NodeKind::External,
                Some("clojure.core"),
            ),
        ] {
            g.nodes.insert(n.id.clone(), n);
        }
        g.edges = vec![
            edge(
                "clj:.//a/f",
                "clj:.//b/h",
                EdgeKind::Calls,
                Certainty::Certain,
            ),
            edge(
                "clj:.//a/g",
                "clj:.//b/h",
                EdgeKind::Calls,
                Certainty::Likely,
            ),
            edge(
                "clj:.//a/f",
                "clj:.//a/g",
                EdgeKind::Calls,
                Certainty::Certain,
            ),
            edge(
                "clj:.//a/f",
                "ext:clj//clojure.core/map",
                EdgeKind::Calls,
                Certainty::Certain,
            ),
        ];
        g
    }

    #[test]
    fn namespace_level_rolls_up_with_counts() {
        let p = graph().payload(AggregationLevel::Namespace);
        let ab = p
            .edges
            .iter()
            .find(|e| e.to.to_string() == "clj:.//b")
            .unwrap();
        assert_eq!(ab.count, Some(2));
        assert_eq!(ab.certainty, Some(Certainty::Certain));
        assert_eq!(ab.by_certainty.as_ref().unwrap()[&Certainty::Likely], 1);
        assert!(!p.edges.iter().any(|e| e.from == e.to), "no self-loops");
        assert!(
            p.nodes
                .iter()
                .any(|n| n.id.to_string() == "ext:clj//clojure.core")
        );
        for e in &p.edges {
            e.validate().unwrap();
        }
    }

    /// WP5.2: aggregates say how many children they contain.
    #[test]
    fn rolled_up_aggregates_carry_their_member_counts() {
        let mut g = graph();
        for (from, to) in [
            ("unit:.", "clj:.//a"),
            ("unit:.", "clj:.//b"),
            ("clj:.//a", "clj:.//a/f"),
            ("clj:.//a", "clj:.//a/g"),
            ("clj:.//b", "clj:.//b/h"),
        ] {
            let mut e = edge(from, to, EdgeKind::Contains, Certainty::Certain);
            e.certainty = None;
            e.reason = None;
            g.edges.push(e);
        }
        let members = |p: &GraphPayload, s: &str| {
            p.nodes
                .iter()
                .find(|n| n.id == id(s))
                .and_then(|n| n.members)
        };
        let ns = g.payload(AggregationLevel::Namespace);
        assert_eq!(members(&ns, "clj:.//a"), Some(2));
        assert_eq!(members(&ns, "clj:.//b"), Some(1));
        assert_eq!(
            members(&g.payload(AggregationLevel::Unit), "unit:."),
            Some(2)
        );
    }

    #[test]
    fn unit_level_has_only_units() {
        let p = graph().payload(AggregationLevel::Unit);
        assert_eq!(p.nodes.len(), 1);
        assert!(p.edges.is_empty());
    }

    #[test]
    fn symbol_level_is_everything() {
        let g = graph();
        let p = g.payload(AggregationLevel::Symbol);
        assert_eq!(p.nodes.len(), g.nodes.len());
        assert_eq!(p.edges.len(), g.edges.len());
    }
}
