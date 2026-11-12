//! Merging fact batches into one graph.

use std::collections::{BTreeMap, BTreeSet};

use aldente_adapter::{FactBatch, LiteralFact};
use aldente_model::{
    BlindSpot, Certainty, Diagnostic, Edge, EdgeKind, Lang, NamespaceId, Node, NodeId, NodeKind,
    Reason, SymbolFlag, SymbolId, UnitInfo,
};

/// The whole repository's graph. Nodes are keyed and sorted by ID; edges,
/// blind spots, diagnostics and literals are sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: BTreeMap<NodeId, Node>,
    pub edges: Vec<Edge>,
    pub blindspots: Vec<BlindSpot>,
    pub diagnostics: Vec<Diagnostic>,
    pub literals: Vec<LiteralFact>,
}

/// The key that keeps edges distinct (`03-graph-model.md`), plus language.
type EdgeKey = (NodeId, NodeId, EdgeKind, Option<Reason>, Option<Lang>);

/// Builds the graph: unit nodes (`service` flag from discovery), `unit
/// contains namespace`, node merging across batches, and cross-unit linking
/// of externals.
///
/// Cross-unit linking: an edge to `ext:clj//ns/name` (or a namespace-level
/// `ext:clj//ns`) whose namespace some unit defines is re-targeted to that
/// unit's node. With one candidate the edge keeps its certainty and reason;
/// with several, it goes to each, `possible`, reason `name-match`. Externals
/// left without edges are dropped.
///
/// Example: in mono-mini, `orders.core/total -calls-> ext:clj//common.util/sum`
/// becomes `-calls-> clj:libs/common//common.util/sum`.
pub fn build(units: &[UnitInfo], batches: Vec<FactBatch>) -> Graph {
    let mut g = Graph::default();
    for unit in units {
        let id = NodeId::Unit(unit.dir.clone());
        let mut flags = BTreeSet::new();
        if unit.service {
            flags.insert(SymbolFlag::Service);
        }
        g.nodes.insert(
            id.clone(),
            Node {
                id,
                kind: NodeKind::Unit,
                subkind: None,
                name: unit.dir.to_string(),
                namespace: None,
                unit: None,
                langs: unit.langs.clone(),
                flags,
                loc: None,
                doc: None,
                signature: None,
            },
        );
    }
    let mut edges: BTreeMap<EdgeKey, Edge> = BTreeMap::new();
    for batch in batches {
        for node in batch.nodes {
            merge_node(&mut g.nodes, node);
        }
        for edge in batch.edges {
            add_edge(&mut edges, edge);
        }
        g.blindspots.extend(batch.blindspots);
        g.diagnostics.extend(batch.diagnostics);
        g.literals.extend(batch.literals);
    }
    mark_services(&mut g.nodes);
    let unit_contains: Vec<Edge> = g
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Namespace)
        .filter_map(|n| Some(contains(n.unit.clone()?, n.id.clone())))
        .collect();
    for e in unit_contains {
        add_edge(&mut edges, e);
    }
    let edges = link_across_units(&g.nodes, edges);
    let used: BTreeSet<&NodeId> = edges.values().flat_map(|e| [&e.from, &e.to]).collect();
    let orphan_externals: Vec<NodeId> = g
        .nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::External | NodeKind::Library))
        .filter(|n| !used.contains(&n.id))
        .map(|n| n.id.clone())
        .collect();
    for id in orphan_externals {
        g.nodes.remove(&id);
    }
    // Blind spots about externals that turned out to be repository code.
    g.blindspots.retain(|b| match &b.node {
        Some(id @ NodeId::External(_)) => g.nodes.contains_key(id),
        _ => true,
    });
    g.edges = edges.into_values().collect();
    for e in &mut g.edges {
        e.sites.sort();
        e.sites.dedup();
    }
    g.blindspots.sort();
    g.blindspots.dedup();
    g.diagnostics.sort();
    g.diagnostics.dedup();
    g.literals.sort();
    g
}

/// Inserts a node, merging languages and flags into an existing one.
///
/// Example: `ext:clj//clojure.core/map` from two units becomes one node.
fn merge_node(nodes: &mut BTreeMap<NodeId, Node>, node: Node) {
    match nodes.get_mut(&node.id) {
        Some(existing) => {
            existing.langs.extend(node.langs);
            existing.flags.extend(node.flags);
            if existing.loc.is_none() {
                existing.loc = node.loc;
            }
        }
        None => {
            nodes.insert(node.id.clone(), node);
        }
    }
}

/// Adds an edge, merging sites into an existing edge with the same key.
///
/// Example: two batches each with one site on `a -calls-> b` give one edge with two sites.
fn add_edge(edges: &mut BTreeMap<EdgeKey, Edge>, edge: Edge) {
    let key = (
        edge.from.clone(),
        edge.to.clone(),
        edge.kind,
        edge.reason,
        edge.lang,
    );
    match edges.get_mut(&key) {
        Some(existing) => existing.sites.extend(edge.sites),
        None => {
            edges.insert(key, edge);
        }
    }
}

/// A `contains` edge.
///
/// Example: `contains(unit:., clj:.//a)`.
fn contains(from: NodeId, to: NodeId) -> Edge {
    Edge {
        from,
        to,
        kind: EdgeKind::Contains,
        certainty: None,
        reason: None,
        lang: None,
        sites: Vec::new(),
        count: None,
        by_certainty: None,
    }
}

/// R13: a unit with an entry point that is not a test is a service.
///
/// Example: a unit holding `app.core/-main` gets the `service` flag; one
/// whose only entry points are `deftest`s does not.
fn mark_services(nodes: &mut BTreeMap<NodeId, Node>) {
    let units: BTreeSet<NodeId> = nodes
        .values()
        .filter(|n| {
            n.flags.contains(&SymbolFlag::EntryPoint) && !n.flags.contains(&SymbolFlag::Test)
        })
        .filter_map(|n| n.unit.clone())
        .collect();
    for id in units {
        if let Some(unit) = nodes.get_mut(&id) {
            unit.flags.insert(SymbolFlag::Service);
        }
    }
}

/// Re-targets edges into externals that a repository unit defines.
///
/// Example: see [`build`].
fn link_across_units(
    nodes: &BTreeMap<NodeId, Node>,
    edges: BTreeMap<EdgeKey, Edge>,
) -> BTreeMap<EdgeKey, Edge> {
    let mut defined: BTreeMap<&str, Vec<&NamespaceId>> = BTreeMap::new();
    for id in nodes.keys() {
        if let NodeId::Namespace(ns) = id {
            defined.entry(ns.ns()).or_default().push(ns);
        }
    }
    let mut out = BTreeMap::new();
    for edge in edges.into_values() {
        let candidates: Vec<NodeId> = match &edge.to {
            NodeId::External(ext) => {
                let namespaces = defined.get(ext.ns()).cloned().unwrap_or_default();
                namespaces
                    .into_iter()
                    .filter_map(|ns| match ext.name() {
                        None => Some(NodeId::Namespace(ns.clone())),
                        Some(name) => SymbolId::new(ns.clone(), name).ok().map(NodeId::Symbol),
                    })
                    .filter(|id| nodes.contains_key(id))
                    .collect()
            }
            _ => Vec::new(),
        };
        // A library does not provide a namespace the repository defines.
        if edge.kind == EdgeKind::Provides && !candidates.is_empty() {
            continue;
        }
        match candidates.len() {
            0 => add_edge(&mut out, edge),
            1 => {
                let mut e = edge;
                e.to = candidates
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| e.to.clone());
                add_edge(&mut out, e);
            }
            _ => {
                for to in candidates {
                    let mut e = edge.clone();
                    e.to = to;
                    if e.certainty.is_some() {
                        e.certainty = Some(Certainty::Possible);
                        e.reason = Some(Reason::NameMatch);
                    }
                    add_edge(&mut out, e);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{RepoPath, Site};
    use std::str::FromStr;

    fn id(s: &str) -> NodeId {
        NodeId::from_str(s).unwrap()
    }

    fn node(s: &str, kind: NodeKind) -> Node {
        Node {
            id: id(s),
            kind,
            subkind: None,
            name: s.into(),
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
        }
    }

    fn calls(from: &str, to: &str, line: u32) -> Edge {
        Edge {
            from: id(from),
            to: id(to),
            kind: EdgeKind::Calls,
            certainty: Some(Certainty::Certain),
            reason: Some(Reason::Direct),
            lang: None,
            sites: vec![Site {
                file: RepoPath::parse("a.clj").unwrap(),
                line,
                col: 1,
            }],
            count: None,
            by_certainty: None,
        }
    }

    fn unit(dir: &str) -> UnitInfo {
        UnitInfo {
            id: NodeId::Unit(RepoPath::parse(dir).unwrap()),
            dir: RepoPath::parse(dir).unwrap(),
            manifests: vec![],
            langs: [Lang::Clj].into(),
            roots: vec![],
            service: dir == "b",
            entry_points: Vec::new(),
            deps: Vec::new(),
            files: 1,
        }
    }

    #[test]
    fn externals_defined_by_one_unit_are_relinked() {
        let a = FactBatch {
            nodes: vec![
                node("clj:a//x", NodeKind::Namespace),
                node("clj:a//x/f", NodeKind::Symbol),
                node("ext:clj//y/g", NodeKind::External),
            ],
            edges: vec![calls("clj:a//x/f", "ext:clj//y/g", 3)],
            ..FactBatch::default()
        };
        let b = FactBatch {
            nodes: vec![
                node("clj:b//y", NodeKind::Namespace),
                node("clj:b//y/g", NodeKind::Symbol),
            ],
            ..FactBatch::default()
        };
        let g = build(&[unit("a"), unit("b")], vec![a, b]);
        let call = g.edges.iter().find(|e| e.kind == EdgeKind::Calls).unwrap();
        assert_eq!(call.to.to_string(), "clj:b//y/g");
        assert_eq!(call.certainty, Some(Certainty::Certain));
        assert!(
            !g.nodes.contains_key(&id("ext:clj//y/g")),
            "unused external dropped"
        );
        assert!(g.nodes[&id("unit:b")].flags.contains(&SymbolFlag::Service));
    }

    #[test]
    fn several_candidates_become_possible_name_matches() {
        let caller = FactBatch {
            nodes: vec![
                node("clj:a//x/f", NodeKind::Symbol),
                node("ext:clj//config/port", NodeKind::External),
            ],
            edges: vec![calls("clj:a//x/f", "ext:clj//config/port", 1)],
            ..FactBatch::default()
        };
        let defs = FactBatch {
            nodes: vec![
                node("clj:b//config", NodeKind::Namespace),
                node("clj:b//config/port", NodeKind::Symbol),
                node("clj:c//config", NodeKind::Namespace),
                node("clj:c//config/port", NodeKind::Symbol),
            ],
            ..FactBatch::default()
        };
        let g = build(&[unit("a"), unit("b"), unit("c")], vec![caller, defs]);
        let to: Vec<String> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .inspect(|e| {
                assert_eq!(e.certainty, Some(Certainty::Possible));
                assert_eq!(e.reason, Some(Reason::NameMatch));
            })
            .map(|e| e.to.to_string())
            .collect();
        assert_eq!(to, ["clj:b//config/port", "clj:c//config/port"]);
    }

    #[test]
    fn same_edges_from_two_batches_merge_their_sites() {
        let one = FactBatch {
            edges: vec![calls("clj:a//x/f", "clj:a//x/g", 1)],
            ..FactBatch::default()
        };
        let two = FactBatch {
            edges: vec![calls("clj:a//x/f", "clj:a//x/g", 2)],
            ..FactBatch::default()
        };
        let g = build(&[], vec![one, two]);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].sites.len(), 2);
    }

    #[test]
    fn units_contain_their_namespaces() {
        let mut ns = node("clj:a//x", NodeKind::Namespace);
        ns.unit = Some(id("unit:a"));
        let g = build(
            &[unit("a")],
            vec![FactBatch {
                nodes: vec![ns],
                ..FactBatch::default()
            }],
        );
        assert!(
            g.edges
                .iter()
                .any(|e| e.kind == EdgeKind::Contains && e.from.to_string() == "unit:a")
        );
    }
}
