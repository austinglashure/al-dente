//! The trace-pinned crater: a stack trace says which implementations
//! actually ran. See "Trace-pinned crater" in
//! `docs/plan/08-crater-and-traces.md`.

use std::collections::{BTreeMap, BTreeSet};

use aldente_graph::Graph;
use aldente_model::crater::{
    CraterCounts, CraterResult, Direction, MissingEdge, PinnedTrace, Reach, UnmappedFrame,
};
use aldente_model::trace::{Frame, Trace, TraceReport};
use aldente_model::{Certainty, Edge, EdgeKind, GraphPayload, NodeId, Reason};

use super::{CraterError, CraterIndex, CraterOptions, crater};

/// The crater pinned to the first trace of `report`.
///
/// The mapped, non-noise frames form a path, outermost first. Each
/// consecutive pair gets a `trace-observed` overlay edge (a `dispatches`
/// edge where the graph has one between them, so the overlay names the one
/// impl that ran). The seed is the innermost frame's node. In reverse the
/// crater is the path plus each path node's other callers one hop out
/// (`faint`); forward it is the ordinary forward crater of the seed; `both`
/// joins the two. Pairs the graph does not connect are listed as missing.
///
/// Example: clj-mini's `checkout-book-nil-price` trace gives the path
/// `checkout → price@:book → apply-discount → half-off`; the reverse crater
/// holds `price@:book` and none of `price`'s other impls.
pub fn pinned_crater(
    graph: &Graph,
    index: &CraterIndex,
    report: &TraceReport,
    opts: &CraterOptions,
) -> Result<CraterResult, CraterError> {
    let stack = report.traces.first().map(full_stack).unwrap_or_default();
    let unmapped: Vec<UnmappedFrame> = stack
        .iter()
        .filter(|f| !f.noise && f.node.is_none())
        .map(|f| UnmappedFrame {
            index: f.index,
            raw: f.raw.clone(),
        })
        .collect();
    let mut path: Vec<NodeId> = Vec::new();
    for f in stack.iter().rev() {
        if let (false, Some(node)) = (f.noise, &f.node)
            && path.last() != Some(node)
        {
            path.push(node.clone());
        }
    }
    let mut overlay = Vec::new();
    let mut missing = Vec::new();
    for pair in path.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let kind = if has_edge(index, a, b, &[EdgeKind::Dispatches]) {
            EdgeKind::Dispatches
        } else {
            EdgeKind::Calls
        };
        overlay.push(Edge {
            from: a.clone(),
            to: b.clone(),
            kind,
            certainty: Some(Certainty::Certain),
            reason: Some(Reason::TraceObserved),
            lang: None,
            sites: Vec::new(),
            count: None,
            by_certainty: None,
        });
        if !connected(index, a, b) {
            missing.push(MissingEdge {
                from: a.clone(),
                to: b.clone(),
            });
        }
    }
    let seeds: Vec<NodeId> = path.last().cloned().into_iter().collect();
    let (faint, reverse) = restricted_reverse(graph, index, &path, &overlay, opts);
    let mut result = match opts.direction {
        Direction::Reverse => reverse,
        Direction::Forward => crater(graph, index, &seeds, opts)?,
        Direction::Both => {
            let forward = crater(
                graph,
                index,
                &seeds,
                &CraterOptions {
                    direction: Direction::Forward,
                    ..opts.clone()
                },
            )?;
            join(forward, reverse)
        }
    };
    result.direction = opts.direction;
    result.pinned = Some(PinnedTrace {
        path,
        overlay,
        faint,
        unmapped,
        missing,
    });
    Ok(result)
}

/// The stack at the throw point, innermost first: the innermost cause's
/// frames, then the frames it shares with the exceptions around it (the
/// `... N more` tails). Frames are renumbered by position in this stack.
///
/// Example: a cause with 2 frames and `... 17 more` inside a trace with 19
/// frames gives 2 + 17 frames.
pub fn full_stack(trace: &Trace) -> Vec<Frame> {
    let mut chain = vec![trace];
    while let Some(c) = chain.last().and_then(|t| t.causes.first()) {
        chain.push(c);
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut shared = 0usize;
    for (i, t) in chain.iter().rev().enumerate() {
        if i == 0 {
            stack.extend(t.frames.iter().cloned());
        } else {
            let take = shared.min(t.frames.len());
            stack.extend(t.frames[t.frames.len() - take..].iter().cloned());
        }
        shared = t.omitted as usize;
        if i > 0 && shared == 0 {
            break;
        }
    }
    for (i, f) in stack.iter_mut().enumerate() {
        f.index = i as u32;
    }
    stack
}

/// Whether the graph has an edge of one of `kinds` from `a` to `b`.
///
/// Example: in clj-mini, `checkout -dispatches-> price@:book` exists.
fn has_edge(index: &CraterIndex, a: &NodeId, b: &NodeId, kinds: &[EdgeKind]) -> bool {
    let (Some(ia), Some(ib)) = (index.ix(a), index.ix(b)) else {
        return false;
    };
    index
        .outgoing(ia)
        .iter()
        .any(|arc| arc.other == ib && kinds.contains(&index.edge(arc.edge).kind))
}

/// Whether the graph connects `a` to `b` in one step: an invocation or
/// registration edge, or a call to a method that `b` implements.
///
/// Example: `run-script` and `half-off` are not connected (the call went
/// through `eval`).
fn connected(index: &CraterIndex, a: &NodeId, b: &NodeId) -> bool {
    const INVOKE: &[EdgeKind] = &[
        EdgeKind::Calls,
        EdgeKind::References,
        EdgeKind::Dispatches,
        EdgeKind::Registers,
    ];
    if has_edge(index, a, b, INVOKE) {
        return true;
    }
    let (Some(ia), Some(ib)) = (index.ix(a), index.ix(b)) else {
        return false;
    };
    let methods: BTreeSet<u32> = index
        .outgoing(ib)
        .iter()
        .filter(|arc| index.edge(arc.edge).kind == EdgeKind::Implements)
        .map(|arc| arc.other)
        .collect();
    index
        .outgoing(ia)
        .iter()
        .any(|arc| methods.contains(&arc.other) && INVOKE.contains(&index.edge(arc.edge).kind))
}

/// The reverse pinned crater: the path (seed = innermost, distance counted
/// outward, all `certain`) and each path node's other callers one hop out
/// over the followed kinds at `min_certainty` or stronger.
///
/// Example: for the path `[a, b]` and a graph edge `x -calls-> b`, the
/// nodes are `a`, `b` and faint `x`.
fn restricted_reverse(
    graph: &Graph,
    index: &CraterIndex,
    path: &[NodeId],
    overlay: &[Edge],
    opts: &CraterOptions,
) -> (Vec<NodeId>, CraterResult) {
    let mut reach: BTreeMap<NodeId, Reach> = BTreeMap::new();
    for (d, id) in path.iter().rev().enumerate() {
        reach.entry(id.clone()).or_insert(Reach {
            id: id.clone(),
            distance: d as u32,
            path_certainty: Certainty::Certain,
            via: (d > 0).then(|| path[path.len() - d].clone()),
            direction: Direction::Reverse,
        });
    }
    let on_path: BTreeSet<&NodeId> = path.iter().collect();
    let mut faint_edges: Vec<usize> = Vec::new();
    let mut faint: BTreeMap<NodeId, Reach> = BTreeMap::new();
    for id in path {
        let Some(i) = index.ix(id) else { continue };
        let d = reach.get(id).map_or(0, |r| r.distance);
        for arc in index.incoming(i) {
            let e = index.edge(arc.edge);
            let caller = index.id(arc.other);
            if !opts.kinds.contains(&e.kind) || e.certainty < opts.min_certainty {
                continue;
            }
            faint_edges.push(arc.edge as usize);
            if on_path.contains(caller) {
                continue;
            }
            let better = faint.get(caller).is_none_or(|r| {
                (e.certainty, std::cmp::Reverse(d + 1))
                    > (r.path_certainty, std::cmp::Reverse(r.distance))
            });
            if better {
                faint.insert(
                    caller.clone(),
                    Reach {
                        id: caller.clone(),
                        distance: d + 1,
                        path_certainty: e.certainty,
                        via: Some(id.clone()),
                        direction: Direction::Reverse,
                    },
                );
            }
        }
    }
    let faint_ids: Vec<NodeId> = faint.keys().cloned().collect();
    reach.extend(faint);
    faint_edges.sort_unstable();
    faint_edges.dedup();
    let inside: BTreeSet<&NodeId> = reach.keys().collect();
    let mut edges: Vec<Edge> = faint_edges
        .into_iter()
        .map(|e| graph.edges[e].clone())
        .filter(|e| inside.contains(&e.from) && inside.contains(&e.to))
        .chain(overlay.iter().cloned())
        .collect();
    sort_edges(&mut edges);
    let reach: Vec<Reach> = reach.into_values().collect();
    let seeds: Vec<NodeId> = path.last().cloned().into_iter().collect();
    let result = assemble(graph, seeds, Direction::Reverse, opts, reach, edges);
    (faint_ids, result)
}

/// Joins a forward crater with the reverse pinned one: union of nodes
/// (the forward label wins where both have one) and edges.
///
/// Example: forward `{c, d}` and reverse `{a, b, c}` give `{a, b, c, d}`.
fn join(forward: CraterResult, reverse: CraterResult) -> CraterResult {
    let mut reach: BTreeMap<NodeId, Reach> = reverse
        .reach
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect();
    for r in forward.reach {
        reach.insert(r.id.clone(), r);
    }
    let mut nodes: BTreeMap<NodeId, aldente_model::Node> = reverse
        .graph
        .nodes
        .into_iter()
        .map(|n| (n.id.clone(), n))
        .collect();
    nodes.extend(forward.graph.nodes.into_iter().map(|n| (n.id.clone(), n)));
    let mut edges = forward.graph.edges;
    edges.extend(reverse.graph.edges);
    sort_edges(&mut edges);
    edges.dedup();
    let reach: Vec<Reach> = reach.into_values().collect();
    CraterResult {
        counts: counts_of(&nodes, &reach),
        graph: GraphPayload {
            nodes: nodes.into_values().collect(),
            edges,
            truncated: false,
            aggregated: None,
        },
        reach,
        boundary: forward.boundary,
        hubs: forward.hubs,
        stopped: forward.stopped,
        hidden_below_min: forward.hidden_below_min,
        truncated: forward.truncated,
        ..forward
    }
}

/// A crater result from explicit nodes and edges.
///
/// Example: see [`restricted_reverse`].
fn assemble(
    graph: &Graph,
    seeds: Vec<NodeId>,
    direction: Direction,
    opts: &CraterOptions,
    reach: Vec<Reach>,
    edges: Vec<Edge>,
) -> CraterResult {
    let nodes: BTreeMap<NodeId, aldente_model::Node> = reach
        .iter()
        .map(|r| {
            let node = graph
                .nodes
                .get(&r.id)
                .cloned()
                .unwrap_or_else(|| super::bare_node(&r.id));
            (r.id.clone(), node)
        })
        .collect();
    CraterResult {
        seeds,
        direction,
        depth: opts.depth,
        min_certainty: opts.min_certainty,
        counts: counts_of(&nodes, &reach),
        graph: GraphPayload {
            nodes: nodes.into_values().collect(),
            edges,
            truncated: false,
            aggregated: None,
        },
        reach,
        boundary: Vec::new(),
        hubs: Vec::new(),
        stopped: Vec::new(),
        hidden_below_min: 0,
        truncated: false,
        diff: None,
        pinned: None,
    }
}

/// Node counts by path certainty and by unit.
///
/// Example: two certain nodes in `unit:.` give `{certain: 2}`, `{"unit:.": 2}`.
fn counts_of(nodes: &BTreeMap<NodeId, aldente_model::Node>, reach: &[Reach]) -> CraterCounts {
    let mut c = CraterCounts::default();
    for r in reach {
        *c.by_certainty.entry(r.path_certainty).or_default() += 1;
        if let Some(unit) = nodes.get(&r.id).and_then(|n| n.unit.as_ref()) {
            *c.by_unit.entry(unit.to_string()).or_default() += 1;
        }
    }
    c
}

/// Sorts edges by the key that keeps them distinct.
///
/// Example: `a→c` sorts after `a→b`.
fn sort_edges(edges: &mut [Edge]) {
    edges.sort_by(|x, y| {
        (&x.from, &x.to, x.kind, x.reason, x.lang).cmp(&(&y.from, &y.to, y.kind, y.reason, y.lang))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trace::normalize;
    use aldente_testkit::golden::{analyze_fixture, fixture_dir};
    use std::str::FromStr;

    fn id(s: &str) -> NodeId {
        NodeId::from_str(s).unwrap()
    }

    fn pin(graph: &Graph, trace: &str, direction: Direction) -> CraterResult {
        let text = std::fs::read_to_string(fixture_dir("traces/jvm/clj-mini").join(trace)).unwrap();
        let report = normalize(&text, graph).unwrap();
        let index = CraterIndex::new(graph);
        let opts = CraterOptions {
            direction,
            min_certainty: Certainty::Possible,
            ..CraterOptions::default()
        };
        pinned_crater(graph, &index, &report, &opts).unwrap()
    }

    #[test]
    fn a_trace_through_a_multimethod_pins_the_impl_that_ran() {
        let graph = analyze_fixture(&fixture_dir("clj-mini")).graph;
        let r = pin(&graph, "checkout-book-nil-price.txt", Direction::Reverse);
        let p = r.pinned.as_ref().unwrap();
        assert_eq!(
            p.path,
            [
                "clj:.//shop.core/checkout",
                "clj:.//shop.pricing/price@:book",
                "clj:.//shop.util/apply-discount",
                "clj:.//shop.pricing/half-off",
            ]
            .map(id)
        );
        assert_eq!(r.seeds, [id("clj:.//shop.pricing/half-off")]);
        let ids: BTreeSet<String> = r.reach.iter().map(|x| x.id.to_string()).collect();
        for other in ["price@:food", "price@:gift", "price@:default"] {
            assert!(
                !ids.contains(&format!("clj:.//shop.pricing/{other}")),
                "{other} in {ids:?}"
            );
        }
        assert!(p.missing.is_empty(), "{:?}", p.missing);
        assert_eq!(p.overlay.len(), 3);
        assert!(
            p.overlay
                .iter()
                .all(|e| e.reason == Some(Reason::TraceObserved))
        );
        assert_eq!(
            p.overlay[0].kind,
            EdgeKind::Dispatches,
            "checkout dispatches to the impl"
        );
        // The test that dispatches to the same impl is one hop out, faint.
        assert!(
            p.faint
                .contains(&id("clj:.//shop.pricing-test/book-is-half-off")),
            "{:?}",
            p.faint
        );
        // Frames from the generator script map to nothing and are listed.
        assert!(p.unmapped.iter().any(|u| u.raw.contains("user$trace_of")));
        let half = r
            .reach
            .iter()
            .find(|x| x.id == id("clj:.//shop.pricing/half-off"))
            .unwrap();
        assert_eq!(half.distance, 0);
        let checkout = r
            .reach
            .iter()
            .find(|x| x.id == id("clj:.//shop.core/checkout"))
            .unwrap();
        assert_eq!(
            (checkout.distance, checkout.path_certainty),
            (3, Certainty::Certain)
        );
    }

    #[test]
    fn a_call_through_eval_is_reported_missing() {
        let graph = analyze_fixture(&fixture_dir("clj-mini")).graph;
        for trace in ["run-script-eval.txt", "run-script-eval.edn"] {
            let r = pin(&graph, trace, Direction::Reverse);
            let p = r.pinned.unwrap();
            assert_eq!(
                p.missing,
                [MissingEdge {
                    from: id("clj:.//shop.plugins/run-script"),
                    to: id("clj:.//shop.pricing/half-off"),
                }],
                "{trace}"
            );
        }
    }

    #[test]
    fn forward_from_the_innermost_frame_is_the_ordinary_crater() {
        let graph = analyze_fixture(&fixture_dir("clj-mini")).graph;
        let r = pin(&graph, "checkout-bad-qty.txt", Direction::Forward);
        assert_eq!(r.seeds, [id("clj:.//shop.inventory/reserve!")]);
        assert!(
            r.reach
                .iter()
                .any(|x| x.id == id("clj:.//shop.inventory/stock"))
        );
        assert!(
            !r.reach
                .iter()
                .any(|x| x.id == id("clj:.//shop.core/checkout"))
        );
        let both = pin(&graph, "checkout-bad-qty.txt", Direction::Both);
        assert!(
            both.reach
                .iter()
                .any(|x| x.id == id("clj:.//shop.core/checkout"))
        );
        assert!(
            both.reach
                .iter()
                .any(|x| x.id == id("clj:.//shop.inventory/stock"))
        );
    }

    #[test]
    fn the_full_stack_follows_omitted_frames_into_the_enclosing_trace() {
        let text = std::fs::read_to_string(fixture_dir("traces/jvm").join("02-nested-causes.txt"))
            .unwrap();
        let report = normalize(&text, &Graph::default()).unwrap();
        let stack = full_stack(&report.traces[0]);
        // Innermost cause: 3 frames and "... 19 more"; its parent has 2 frames
        // and "... 17 more" of the outermost trace's 17.
        assert_eq!(stack.len(), 3 + 2 + 17);
        assert!(stack[0].raw.contains("Numbers.divide"));
        assert!(stack.last().unwrap().raw.contains("clojure.main.main"));
    }
}
