//! Graph payloads at each aggregation level (`03-graph-model.md`,
//! "Aggregation levels").
//!
//! - `symbol`: every stored node and edge.
//! - `namespace`: namespaces, namespace-level externals and platforms;
//!   `requires` as stored, plus `calls`, `references` and `dispatches`
//!   rolled up between namespaces (self-loops dropped).
//! - `unit`: units; invocation edges rolled up between different units.
//!   Externals have no library until Phase 2, so they are left out.
//!
//! Rolled-up edges carry `count`, `by_certainty`, the strongest certainty,
//! and the most frequent reason among their members (ties: the first in
//! `Reason` order), so they still satisfy the edge invariants.

use std::collections::{BTreeMap, BTreeSet};

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
        let mut nodes: BTreeMap<NodeId, Node> = BTreeMap::new();
        let mut members: BTreeMap<(NodeId, NodeId, EdgeKind), Vec<&Edge>> = BTreeMap::new();
        let mut kept: Vec<Edge> = Vec::new();
        for edge in &self.edges {
            let (Some(from), Some(to)) = (group(self, &edge.from), group(self, &edge.to)) else {
                continue;
            };
            if edge.kind == EdgeKind::Requires && level == AggregationLevel::Namespace {
                kept.push(edge.clone());
            } else if INVOCATION.contains(&edge.kind) && from != to {
                members
                    .entry((from.clone(), to.clone(), edge.kind))
                    .or_default()
                    .push(edge);
            } else {
                continue;
            }
            for id in [from, to] {
                if !nodes.contains_key(&id) {
                    nodes.insert(id.clone(), self.group_node(&id));
                }
            }
        }
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
        edges.extend(
            members
                .into_iter()
                .map(|((from, to, kind), es)| roll(from, to, kind, &es)),
        );
        edges.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
        GraphPayload {
            nodes: nodes.into_values().collect(),
            edges,
            truncated: false,
            aggregated: Some(Aggregation { level }),
        }
    }

    /// The namespace-level node a node rolls up into: its namespace (from
    /// `namespace` and `unit`), the namespace-level external for externals,
    /// itself for namespaces and platforms; units and holes have none.
    ///
    /// Example: `clj:.//shop.core/checkout` gives `clj:.//shop.core`;
    /// `ext:clj//clojure.core/map` gives `ext:clj//clojure.core`.
    fn namespace_of(&self, id: &NodeId) -> Option<NodeId> {
        match id {
            NodeId::Namespace(_) | NodeId::Platform(_) => Some(id.clone()),
            NodeId::External(ext) => ExternalId::namespace(ext.lang(), ext.ns())
                .ok()
                .map(NodeId::External),
            NodeId::Symbol(_) | NodeId::Impl(_) => {
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
    fn unit_of(&self, id: &NodeId) -> Option<NodeId> {
        match id {
            NodeId::Unit(_) => Some(id.clone()),
            _ => self.nodes.get(id)?.unit.clone(),
        }
    }

    /// The node for a group ID: the stored node if there is one, otherwise a
    /// synthesized namespace-level external.
    ///
    /// Example: `ext:clj//clojure.core` gives an external node named `clojure.core`.
    fn group_node(&self, id: &NodeId) -> Node {
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
        })
    }
}

/// One rolled-up edge from its members.
///
/// Example: three `calls` edges (2 certain, 1 likely) give `count: 3`,
/// `certainty: certain`, `by_certainty: {certain: 2, likely: 1}`.
fn roll(from: NodeId, to: NodeId, kind: EdgeKind, members: &[&Edge]) -> Edge {
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
