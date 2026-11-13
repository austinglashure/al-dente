//! The impact crater: the subgraph a set of seeds can reach, or that can
//! reach them, with the certainty of the best path to each node.
//! See "Impact crater" in `docs/plan/08-crater-and-traces.md`.

mod checklist;
mod index;
mod search;
pub mod seeds;
#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap, HashSet};

use aldente_graph::Graph;
use aldente_model::crater::{CraterCounts, CraterResult, Direction, Reach};
use aldente_model::{Certainty, EdgeKind, GraphPayload, Node, NodeId, NodeKind};
use thiserror::Error;

use crate::glob::glob_match;
pub use checklist::checklist;
pub use index::{CraterIndex, Ix};
use search::{Label, Rules, Side, is_boundary, kind_bit, search};

/// Settings of one crater search. Defaults follow the plan: forward, depth
/// 6, `likely`, the invocation kinds plus `registers`, stop at externals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraterOptions {
    pub direction: Direction,
    /// 0 means unlimited.
    pub depth: u32,
    pub min_certainty: Certainty,
    /// Edge kinds followed. `implements` is handled separately (seeded
    /// methods fan out to their impls) and need not be listed.
    pub kinds: Vec<EdgeKind>,
    /// Wildcard patterns (`*`, `?`) matched against a node's ID and its
    /// `namespace/name`; matching nodes are included but not expanded.
    pub stop_at: Vec<String>,
    pub stop_at_externals: bool,
    /// Do not expand nodes outside the seeds' units.
    pub stop_at_units: bool,
    pub exclude_tests: bool,
    /// Expand past hubs too.
    pub through_hubs: bool,
    /// Degree at which a node is a hub; `None` uses the index's default.
    pub hub_degree: Option<u32>,
    /// Most nodes in the result before it is cut and marked truncated.
    pub node_cap: usize,
}

impl Default for CraterOptions {
    fn default() -> Self {
        CraterOptions {
            direction: Direction::Forward,
            depth: 6,
            min_certainty: Certainty::Likely,
            kinds: vec![
                EdgeKind::Calls,
                EdgeKind::References,
                EdgeKind::Dispatches,
                EdgeKind::Registers,
            ],
            stop_at: vec![],
            stop_at_externals: true,
            stop_at_units: false,
            exclude_tests: false,
            through_hubs: false,
            hub_degree: None,
            node_cap: 10_000,
        }
    }
}

/// Why a crater could not be computed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CraterError {
    #[error("not in the graph: {0}")]
    UnknownSeed(NodeId),
}

/// Computes the crater of `seeds` over `graph` (whose index is `index`).
///
/// Namespace seeds stand for the symbols they contain and library seeds for
/// the externals and namespaces they provide; both are listed in the result
/// as seeds at distance 0.
///
/// No seeds give an empty crater.
///
/// Example: for `a -calls-> b -calls-> c`, the forward crater of `a` holds
/// `a` (distance 0), `b` (1, via `a`) and `c` (2, via `b`), all `certain`.
pub fn crater(
    graph: &Graph,
    index: &CraterIndex,
    seeds: &[NodeId],
    opts: &CraterOptions,
) -> Result<CraterResult, CraterError> {
    let mut seed_ix = Vec::new();
    for s in seeds {
        seed_ix.push(
            index
                .ix(s)
                .ok_or_else(|| CraterError::UnknownSeed(s.clone()))?,
        );
    }
    let seed_ix = expand_seeds(index, seed_ix);
    let mut is_seed = vec![false; index.len()];
    for &s in &seed_ix {
        is_seed[s as usize] = true;
    }
    let hub = opts.hub_degree.unwrap_or_else(|| index.hub_threshold());
    let seed_units: HashSet<Ix> = seed_ix.iter().filter_map(|&s| index.node(s).unit).collect();
    let is_hub = |i: Ix| !opts.through_hubs && index.degree(i) >= hub;
    let is_stopped = |i: Ix| stop_match(graph, index.id(i), &opts.stop_at);
    let expandable = |i: Ix| {
        let info = index.node(i);
        !is_hub(i)
            && !(opts.stop_at_externals && info.kind == NodeKind::External)
            && !(opts.stop_at_units && info.unit.is_some_and(|u| !seed_units.contains(&u)))
            && !is_stopped(i)
    };
    let excluded = |i: Ix| opts.exclude_tests && index.node(i).test && !is_seed[i as usize];
    let kind_mask = opts.kinds.iter().fold(0, |m, k| m | kind_bit(*k));
    let side = |direction| {
        search(&Rules {
            index,
            direction,
            depth: opts.depth,
            min_certainty: opts.min_certainty,
            kind_mask,
            seeds: &seed_ix,
            is_seed: &is_seed,
            expandable: &expandable,
            excluded: &excluded,
            node_cap: opts.node_cap,
        })
    };
    let sides: Vec<(Direction, Side)> = match opts.direction {
        Direction::Both => vec![
            (Direction::Forward, side(Direction::Forward)),
            (Direction::Reverse, side(Direction::Reverse)),
        ],
        d => vec![(d, side(d))],
    };
    let merged = merge(index.len(), &sides);
    let truncated = sides.iter().any(|(_, s)| s.truncated);
    let mut included: Vec<Ix> = Vec::new();
    let mut hidden = 0u64;
    for (i, label) in merged.iter().enumerate() {
        if let Some((label, _)) = label {
            if label.certainty >= opts.min_certainty {
                included.push(i as Ix);
            } else {
                hidden += 1;
            }
        }
    }
    let mut ids_sorted: Vec<(NodeId, Ix)> =
        included.iter().map(|&i| (index.id(i).clone(), i)).collect();
    ids_sorted.sort();
    let pick = |pred: &dyn Fn(Ix) -> bool| -> Vec<NodeId> {
        ids_sorted
            .iter()
            .filter(|(_, i)| pred(*i))
            .map(|(id, _)| id.clone())
            .collect()
    };
    let not_seed = |i: Ix| !is_seed[i as usize];
    let reach: Vec<Reach> = ids_sorted
        .iter()
        .filter_map(|(id, i)| {
            let (label, direction) = merged[*i as usize]?;
            Some(Reach {
                id: id.clone(),
                distance: label.distance,
                path_certainty: label.certainty,
                via: label.via.map(|v| index.id(v).clone()),
                direction,
            })
        })
        .collect();
    let mut seeds_out: Vec<NodeId> = seeds.to_vec();
    seeds_out.sort();
    seeds_out.dedup();
    Ok(CraterResult {
        seeds: seeds_out,
        direction: opts.direction,
        depth: opts.depth,
        min_certainty: opts.min_certainty,
        graph: payload(
            graph,
            index,
            &ids_sorted,
            &merged,
            kind_mask,
            opts.min_certainty,
        ),
        counts: counts(index, &ids_sorted, &merged),
        boundary: pick(&|i| is_boundary(index.node(i).kind)),
        hubs: pick(&|i| not_seed(i) && is_hub(i)),
        stopped: pick(&|i| not_seed(i) && is_stopped(i)),
        reach,
        hidden_below_min: hidden,
        truncated,
        diff: None,
    })
}

/// Adds what namespace and library seeds stand for: a namespace's contained
/// symbols (and their impls), a library's provided externals and namespaces
/// (and what those contain). Sorted, without duplicates.
///
/// Example: a seed `clj:.//app.core` that contains `a` and `b` gives
/// `[app.core, a, b]` (as indexes).
fn expand_seeds(index: &CraterIndex, seeds: Vec<Ix>) -> Vec<Ix> {
    let mut out: BTreeSet<Ix> = BTreeSet::new();
    // (node, whether it stands in for a namespace or library seed)
    let mut stack: Vec<(Ix, bool)> = seeds.into_iter().map(|s| (s, false)).collect();
    while let Some((s, derived)) = stack.pop() {
        if !out.insert(s) {
            continue;
        }
        let follow = match index.node(s).kind {
            NodeKind::Namespace => Some(EdgeKind::Contains),
            NodeKind::Library => Some(EdgeKind::Provides),
            // A symbol seed is itself; one standing in for its namespace
            // also brings the impls it contains.
            NodeKind::Symbol if derived => Some(EdgeKind::Contains),
            _ => None,
        };
        if let Some(kind) = follow {
            for a in index.outgoing(s) {
                if index.edge(a.edge).kind == kind {
                    stack.push((a.other, true));
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Merges the sides of a `both` crater: per node, the stronger certainty,
/// then the shorter distance, then forward.
///
/// Example: a node reached forward `likely` at 1 and reverse `certain` at 3
/// keeps the reverse label.
fn merge(n: usize, sides: &[(Direction, Side)]) -> Vec<Option<(Label, Direction)>> {
    let mut out: Vec<Option<(Label, Direction)>> = vec![None; n];
    for (direction, side) in sides {
        for (i, label) in side.labels.iter().enumerate() {
            let Some(label) = label else { continue };
            let better = match &out[i] {
                None => true,
                Some((old, _)) => {
                    (label.certainty, std::cmp::Reverse(label.distance))
                        > (old.certainty, std::cmp::Reverse(old.distance))
                }
            };
            if better {
                out[i] = Some((*label, *direction));
            }
        }
    }
    out
}

/// Whether `id` matches any stop pattern, by ID or by `namespace/name`.
///
/// Example: `*log*` matches `clj:.//app.log/info`.
fn stop_match(graph: &Graph, id: &NodeId, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let text = id.to_string();
    let qualified = graph
        .nodes
        .get(id)
        .and_then(|n| n.namespace.as_ref().map(|ns| format!("{ns}/{}", n.name)));
    patterns
        .iter()
        .any(|p| glob_match(p, &text) || qualified.as_deref().is_some_and(|q| glob_match(p, q)))
}

/// The crater as a graph payload: node records (a bare record for endpoints
/// the graph has none for) and the edges among included nodes that the
/// search could follow at `min` or stronger, in `Graph::edges` order.
///
/// Example: for the crater `{a, b}` of `a -calls-> b`, one edge.
fn payload(
    graph: &Graph,
    index: &CraterIndex,
    ids: &[(NodeId, Ix)],
    merged: &[Option<(Label, Direction)>],
    kind_mask: u32,
    min: Certainty,
) -> GraphPayload {
    let inside = |i: Ix| merged[i as usize].is_some_and(|(l, _)| l.certainty >= min);
    let followed = kind_mask | kind_bit(EdgeKind::Implements);
    let mut edges: Vec<u32> = Vec::new();
    for &(_, i) in ids {
        for a in index.outgoing(i) {
            let info = index.edge(a.edge);
            if kind_bit(info.kind) & followed != 0 && info.certainty >= min && inside(a.other) {
                edges.push(a.edge);
            }
        }
    }
    edges.sort_unstable();
    GraphPayload {
        nodes: ids
            .iter()
            .map(|(id, _)| {
                graph
                    .nodes
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| bare_node(id))
            })
            .collect(),
        edges: edges
            .into_iter()
            .map(|e| graph.edges[e as usize].clone())
            .collect(),
        truncated: false,
        aggregated: None,
    }
}

/// A minimal node record for an ID the graph has no record for.
///
/// Example: `bare_node(ext:clj//x/y)` has kind `external` and name the ID.
fn bare_node(id: &NodeId) -> Node {
    Node {
        id: id.clone(),
        kind: id.kind(),
        subkind: None,
        name: id.to_string(),
        namespace: None,
        unit: None,
        langs: BTreeSet::new(),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
    }
}

/// Node counts by path certainty and by unit.
///
/// Example: two certain nodes in `unit:.` give `{certain: 2}` and `{"unit:.": 2}`.
fn counts(
    index: &CraterIndex,
    ids: &[(NodeId, Ix)],
    merged: &[Option<(Label, Direction)>],
) -> CraterCounts {
    let mut c = CraterCounts::default();
    let mut by_unit: HashMap<Ix, u64> = HashMap::new();
    for &(_, i) in ids {
        if let Some((label, _)) = merged[i as usize] {
            *c.by_certainty.entry(label.certainty).or_default() += 1;
        }
        if let Some(u) = index.node(i).unit {
            *by_unit.entry(u).or_default() += 1;
        }
    }
    c.by_unit = by_unit
        .into_iter()
        .map(|(u, n)| (index.id(u).to_string(), n))
        .collect();
    c
}
