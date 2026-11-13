//! Measure tests: hand-computed small graphs, Zachary's karate club against
//! networkx 3.7, certainty weighting, projections, caching and scale.

use std::collections::BTreeSet;
use std::str::FromStr;

use aldente_graph::Graph;
use aldente_model::measures::{MeasureReport, ProjectionKind, ProjectionSpec};
use aldente_model::{
    AggregationLevel, Certainty, Edge, EdgeKind, Node, NodeId, NodeKind, Reason, SymbolFlag,
};

use super::cache::{MemoryStore, ResultStore, measures};
use super::projection::{Ix, Projection, project};
use super::{first_set, first_set_on, hits, kcore, pagerank, scc};
use Certainty::{Certain, Likely, Possible};

fn id(s: &str) -> NodeId {
    if s.contains(':') {
        NodeId::from_str(s).unwrap()
    } else {
        NodeId::from_str(&format!("clj:.//t/{s}")).unwrap()
    }
}

fn node_in(ns: &str, name: &str) -> Node {
    let id = NodeId::from_str(&format!("clj:.//{ns}/{name}")).unwrap();
    Node {
        kind: id.kind(),
        id,
        subkind: None,
        name: name.to_owned(),
        namespace: Some(ns.to_owned()),
        unit: Some(NodeId::Unit(aldente_model::RepoPath::root())),
        langs: BTreeSet::new(),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
    }
}

fn edge(from: NodeId, to: NodeId, kind: EdgeKind, c: Certainty) -> Edge {
    let structural = !kind.needs_certainty();
    Edge {
        from,
        to,
        kind,
        certainty: (!structural).then_some(c),
        reason: (!structural).then_some(Reason::Direct),
        lang: None,
        sites: vec![],
        count: None,
        by_certainty: None,
    }
}

/// A graph over symbols in namespace `t` from `(from, to, certainty)` calls.
fn graph(calls: &[(&str, &str, Certainty)]) -> Graph {
    let mut g = Graph::default();
    for (a, b, c) in calls {
        for s in [a, b] {
            let n = node_in("t", s);
            g.nodes.entry(n.id.clone()).or_insert(n);
        }
        g.edges.push(edge(id(a), id(b), EdgeKind::Calls, *c));
    }
    g
}

fn report(g: &Graph) -> MeasureReport {
    first_set(g, &ProjectionSpec::default()).unwrap()
}

fn row<'a>(r: &'a MeasureReport, name: &str) -> &'a aldente_model::measures::NodeMeasures {
    r.nodes.iter().find(|n| n.id == id(name)).unwrap()
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn chain_matches_hand_values() {
    // a → b → c, all certain. PageRank by hand: c is dangling, so every node
    // gets base = 0.05 + 0.85·c/3; a = base, b = base + 0.85·a,
    // c = base + 0.85·b = 2.5725·base, so base = 0.05 / (1 − 0.85·2.5725/3).
    let r = report(&graph(&[("a", "b", Certain), ("b", "c", Certain)]));
    let base = 0.05 / (1.0 - 0.85 * 2.5725 / 3.0);
    let (a, b, c) = (row(&r, "a"), row(&r, "b"), row(&r, "c"));
    assert!(close(a.pagerank, base, 1e-9), "{}", a.pagerank);
    assert!(close(b.pagerank, 1.85 * base, 1e-9));
    assert!(close(c.pagerank, 2.5725 * base, 1e-9));
    // Reversed, c → b → a: the same values mirrored.
    assert!(close(c.pagerank_reverse, base, 1e-9));
    assert!(close(a.pagerank_reverse, 2.5725 * base, 1e-9));
    assert_eq!(
        (a.fan_in, a.fan_out, b.fan_in, b.fan_out, c.fan_in),
        (0, 1, 1, 1, 1)
    );
    // HITS: a and b each point at one authority of equal value.
    assert!(close(a.hub, 0.5, 1e-9) && close(b.hub, 0.5, 1e-9) && c.hub == 0.0);
    assert!(a.authority == 0.0 && close(b.authority, 0.5, 1e-9));
    assert_eq!([a.core, b.core, c.core], [1, 1, 1]);
    assert!(r.components.is_empty());
    assert!(r.convergence.values().all(|c| c.converged));
}

#[test]
fn cycle_with_tail_matches_hand_values() {
    // a → b → c → a (c → a possible) and d → a.
    let g = graph(&[
        ("a", "b", Certain),
        ("b", "c", Certain),
        ("c", "a", Possible),
        ("d", "a", Likely),
    ]);
    let r = report(&g);
    assert_eq!(r.components.len(), 1);
    let comp = &r.components[0];
    assert_eq!(comp.nodes, vec![id("a"), id("b"), id("c")]);
    // The weakest arc goes.
    assert_eq!(comp.cut.len(), 1);
    assert_eq!(
        (comp.cut[0].from.clone(), comp.cut[0].to.clone()),
        (id("c"), id("a"))
    );
    assert!(comp.cut_minimal);
    assert_eq!(row(&r, "a").component, Some(0));
    assert_eq!(row(&r, "d").component, None);
    // Triangle plus a pendant.
    let cores: Vec<u32> = ["a", "b", "c", "d"]
        .iter()
        .map(|n| row(&r, n).core)
        .collect();
    assert_eq!(cores, vec![2, 2, 2, 1]);
    assert!(close(row(&r, "a").fan_in_weighted, 0.25 + 0.6, 1e-12));
    assert_eq!(row(&r, "a").fan_in, 2);
    // A certain 3-cycle: PageRank and HITS are uniform.
    let r = report(&graph(&[
        ("a", "b", Certain),
        ("b", "c", Certain),
        ("c", "a", Certain),
    ]));
    for n in ["a", "b", "c"] {
        assert!(close(row(&r, n).pagerank, 1.0 / 3.0, 1e-9));
        assert!(close(row(&r, n).hub, 1.0 / 3.0, 1e-9));
    }
}

#[test]
fn cut_breaks_every_cycle_and_prefers_weak_arcs() {
    // Two cycles sharing b → c: a → b → c → a and b → c → d → b; the
    // certain shared arc stays, the two possible back arcs go.
    let g = graph(&[
        ("a", "b", Certain),
        ("b", "c", Certain),
        ("c", "a", Possible),
        ("c", "d", Certain),
        ("d", "b", Possible),
    ]);
    let r = report(&g);
    let cut: Vec<(NodeId, NodeId)> = r.components[0]
        .cut
        .iter()
        .map(|c| (c.from.clone(), c.to.clone()))
        .collect();
    assert_eq!(cut, vec![(id("c"), id("a")), (id("d"), id("b"))]);
}

/// Zachary's karate club (as networkx 3.7 `karate_club_graph()`, unweighted).
const KARATE: &[(u32, u32)] = &[
    (0, 1),
    (0, 2),
    (0, 3),
    (0, 4),
    (0, 5),
    (0, 6),
    (0, 7),
    (0, 8),
    (0, 10),
    (0, 11),
    (0, 12),
    (0, 13),
    (0, 17),
    (0, 19),
    (0, 21),
    (0, 31),
    (1, 2),
    (1, 3),
    (1, 7),
    (1, 13),
    (1, 17),
    (1, 19),
    (1, 21),
    (1, 30),
    (2, 3),
    (2, 7),
    (2, 8),
    (2, 9),
    (2, 13),
    (2, 27),
    (2, 28),
    (2, 32),
    (3, 7),
    (3, 12),
    (3, 13),
    (4, 6),
    (4, 10),
    (5, 6),
    (5, 10),
    (5, 16),
    (6, 16),
    (8, 30),
    (8, 32),
    (8, 33),
    (9, 33),
    (13, 33),
    (14, 32),
    (14, 33),
    (15, 32),
    (15, 33),
    (18, 32),
    (18, 33),
    (19, 33),
    (20, 32),
    (20, 33),
    (22, 32),
    (22, 33),
    (23, 25),
    (23, 27),
    (23, 29),
    (23, 32),
    (23, 33),
    (24, 25),
    (24, 27),
    (24, 31),
    (25, 31),
    (26, 29),
    (26, 33),
    (27, 33),
    (28, 31),
    (28, 33),
    (29, 32),
    (29, 33),
    (30, 32),
    (30, 33),
    (31, 32),
    (31, 33),
    (32, 33),
];

/// `nx.pagerank(G, alpha=0.85, weight=None, tol=1e-12)`, rounded to 9 places.
const KARATE_PAGERANK: [f64; 34] = [
    0.096997285,
    0.052876924,
    0.057078509,
    0.035859858,
    0.021977952,
    0.029111155,
    0.029111155,
    0.024490497,
    0.029766056,
    0.014309397,
    0.021977952,
    0.009564745,
    0.014644892,
    0.029536456,
    0.014535994,
    0.014535994,
    0.016784005,
    0.014558677,
    0.014535994,
    0.019604636,
    0.014535994,
    0.014558677,
    0.014535994,
    0.031522515,
    0.021076034,
    0.021006197,
    0.015044038,
    0.025639767,
    0.019573459,
    0.026288538,
    0.024590155,
    0.037158087,
    0.071693226,
    0.100919182,
];

/// `nx.hits(nx.Graph(G.edges()), tol=1e-12)` hubs (unweighted; equal to
/// authorities on an undirected graph).
const KARATE_HUBS: [f64; 34] = [
    0.071412729,
    0.053427231,
    0.063719065,
    0.042422737,
    0.01526096,
    0.015966914,
    0.015966914,
    0.034343167,
    0.045681925,
    0.020625668,
    0.01526096,
    0.010617892,
    0.016925451,
    0.045494864,
    0.020370346,
    0.020370346,
    0.004748032,
    0.018561637,
    0.020370346,
    0.029713334,
    0.020370346,
    0.018561637,
    0.020370346,
    0.030156498,
    0.011460952,
    0.011893664,
    0.015182734,
    0.026813494,
    0.026331506,
    0.02711154,
    0.035106238,
    0.038375742,
    0.062001846,
    0.075002942,
];

/// `nx.core_number(G)`.
const KARATE_CORES: [u32; 34] = [
    4, 4, 4, 4, 3, 3, 3, 4, 4, 2, 3, 1, 2, 4, 2, 2, 2, 2, 2, 3, 2, 2, 2, 3, 3, 3, 2, 3, 3, 3, 4, 3,
    4, 4,
];

/// The karate club as a projection, each undirected edge both ways; IDs
/// `k00..k33` keep node order equal to the club's numbering.
fn karate() -> Projection {
    let ids = (0..34).map(|i| id(&format!("k{i:02}"))).collect();
    let arcs: Vec<(Ix, Ix, f64, u32)> = KARATE
        .iter()
        .flat_map(|&(a, b)| [(a, b, 1.0, 1), (b, a, 1.0, 1)])
        .collect();
    Projection::from_arcs(ids, &arcs)
}

#[test]
fn karate_club_matches_networkx() {
    let p = karate();
    let (pr, conv) = pagerank::pagerank(&p);
    assert!(conv.converged);
    let (hub, auth, conv) = hits::hits(&p);
    assert!(conv.converged);
    let core = kcore::core_numbers(&p.undirected());
    for i in 0..34 {
        assert!(
            close(pr[i], KARATE_PAGERANK[i], 2e-9),
            "pagerank {i}: {}",
            pr[i]
        );
        assert!(close(hub[i], KARATE_HUBS[i], 2e-9), "hub {i}: {}", hub[i]);
        assert!(
            close(auth[i], KARATE_HUBS[i], 2e-9),
            "authority {i}: {}",
            auth[i]
        );
    }
    assert_eq!(core, KARATE_CORES);
    // Undirected and connected: one component of all 34.
    assert_eq!(scc::components(&p).len(), 1);
}

#[test]
fn lowering_edges_to_possible_lowers_their_targets_pagerank() {
    let calls = [
        ("a", "t1", Certain),
        ("b", "t1", Certain),
        ("c", "t2", Certain),
        ("a", "x", Certain),
        ("t1", "y", Certain),
        ("t2", "y", Certain),
    ];
    let before = report(&graph(&calls));
    let lowered: Vec<_> = calls
        .iter()
        .map(|&(f, t, c)| {
            if t.starts_with('t') {
                (f, t, Possible)
            } else {
                (f, t, c)
            }
        })
        .collect();
    let after = report(&graph(&lowered));
    for t in ["t1", "t2"] {
        assert!(
            row(&after, t).pagerank < row(&before, t).pagerank,
            "{t}: {} !< {}",
            row(&after, t).pagerank,
            row(&before, t).pagerank
        );
    }
    let sum: f64 = after.nodes.iter().map(|n| n.pagerank).sum();
    assert!(close(sum, 1.0, 1e-9));
}

#[test]
fn projection_excludes_tests_and_externals_by_default() {
    let mut g = graph(&[("a", "b", Certain)]);
    let mut t = node_in("t-test", "check");
    t.flags.insert(SymbolFlag::Test);
    g.nodes.insert(t.id.clone(), t.clone());
    g.edges
        .push(edge(t.id.clone(), id("a"), EdgeKind::Calls, Certain));
    let ext = id("ext:clj//clojure.core/map");
    g.edges
        .push(edge(id("a"), ext.clone(), EdgeKind::Calls, Certain));
    let p = project(&g, &ProjectionSpec::default()).unwrap();
    assert_eq!(p.ids(), &[id("a"), id("b")]);
    let all = ProjectionSpec {
        include_tests: true,
        include_externals: true,
        ..ProjectionSpec::default()
    };
    let p = project(&g, &all).unwrap();
    assert_eq!(p.len(), 4);
    assert_eq!(p.arc_count(), 3);
    // Symbols in a test namespace are test code even without the flag.
    let mut helper = node_in("t-test", "helper");
    helper.flags.clear();
    let mut ns = node_in("t-test", "x");
    ns.id = id("clj:.//t-test");
    ns.kind = NodeKind::Namespace;
    ns.flags.insert(SymbolFlag::Test);
    g.nodes.insert(ns.id.clone(), ns);
    g.nodes.insert(helper.id.clone(), helper.clone());
    assert!(super::projection::is_test(&g, &helper));
}

#[test]
fn namespace_level_sums_symbol_pairs_and_drops_internal_calls() {
    let mut g = Graph::default();
    for (ns, name) in [("p", "f"), ("p", "g"), ("q", "h"), ("q", "k")] {
        let n = node_in(ns, name);
        g.nodes.insert(n.id.clone(), n);
    }
    for ns in ["p", "q"] {
        let mut n = node_in(ns, "x");
        n.id = id(&format!("clj:.//{ns}"));
        n.kind = NodeKind::Namespace;
        g.nodes.insert(n.id.clone(), n);
    }
    let s = |x: &str| id(&format!("clj:.//{x}"));
    g.edges
        .push(edge(s("p/f"), s("q/h"), EdgeKind::Calls, Certain));
    // Two edges between one pair count once, at the stronger weight.
    g.edges
        .push(edge(s("p/g"), s("q/h"), EdgeKind::Calls, Possible));
    g.edges
        .push(edge(s("p/g"), s("q/h"), EdgeKind::References, Likely));
    g.edges
        .push(edge(s("p/f"), s("p/g"), EdgeKind::Calls, Certain));
    let spec = ProjectionSpec {
        level: AggregationLevel::Namespace,
        ..ProjectionSpec::default()
    };
    let p = project(&g, &spec).unwrap();
    assert_eq!(p.ids(), &[s("p"), s("q")]);
    let arc = p.outgoing(0)[0];
    assert_eq!(arc.other, 1);
    assert!(close(arc.weight, 1.6, 1e-12));
    assert_eq!(arc.count, 2);
    // Structure has no symbol level.
    let bad = ProjectionSpec {
        kind: ProjectionKind::Structure,
        ..ProjectionSpec::default()
    };
    assert!(project(&g, &bad).is_err());
}

#[test]
fn min_certainty_drops_weaker_edges() {
    let g = graph(&[("a", "b", Certain), ("b", "c", Possible)]);
    let spec = ProjectionSpec {
        min_certainty: Likely,
        ..ProjectionSpec::default()
    };
    assert_eq!(project(&g, &spec).unwrap().arc_count(), 1);
}

#[test]
fn results_are_cached_by_projection() {
    let g = graph(&[("a", "b", Certain)]);
    let store = MemoryStore::default();
    let spec = ProjectionSpec::default();
    let (first, warn) = measures(&g, &spec, &store).unwrap();
    assert!(warn.is_none());
    // A different graph with the same key reads the stored result back.
    let other = graph(&[("x", "y", Certain)]);
    let (again, _) = measures(&other, &spec, &store).unwrap();
    assert_eq!(first, again);
    let key = serde_json::to_string(&spec).unwrap();
    assert!(store.get("measures/first-set", &key).is_some());
    // Another projection is computed separately.
    let rev = ProjectionSpec {
        include_tests: true,
        ..spec
    };
    let (third, _) = measures(&other, &rev, &store).unwrap();
    assert_eq!(third.nodes[0].id, id("x"));
}

#[test]
fn results_are_deterministic() {
    let g = graph(&[
        ("a", "b", Certain),
        ("b", "a", Likely),
        ("c", "a", Possible),
    ]);
    let a = serde_json::to_string(&report(&g)).unwrap();
    let b = serde_json::to_string(&report(&g)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn first_set_on_200k_nodes_takes_under_ten_seconds() {
    // Debug builds are far slower; they check the result on a tenth of it.
    let n: u64 = if cfg!(debug_assertions) {
        20_000
    } else {
        200_000
    };
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let levels = [Certain, Likely, Possible];
    let ids: Vec<NodeId> = (0..n).map(|i| id(&format!("f{i}"))).collect();
    let mut g = Graph::default();
    for (i, nid) in ids.iter().enumerate() {
        let mut nd = node_in("t", &format!("f{i}"));
        nd.id = nid.clone();
        g.nodes.insert(nid.clone(), nd);
    }
    for _ in 0..(n * 5) {
        let (a, b) = ((next() % n) as usize, (next() % n) as usize);
        let c = levels[(next() % 3) as usize];
        g.edges
            .push(edge(ids[a].clone(), ids[b].clone(), EdgeKind::Calls, c));
    }
    let start = std::time::Instant::now();
    let p = project(&g, &ProjectionSpec::default()).unwrap();
    let r = first_set_on(&p, ProjectionSpec::default());
    let elapsed = start.elapsed();
    eprintln!("first set on {n} nodes / {} edges took {elapsed:?}", n * 5);
    assert_eq!(r.nodes.len(), n as usize);
    assert!(
        r.convergence.values().all(|c| c.converged),
        "{:?}",
        r.convergence
    );
    assert!(!r.components.is_empty());
    // The budget is for release builds (`cargo test --release -p
    // aldente-analysis 200k`); debug builds only check the result.
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_secs_f64() < 10.0, "took {elapsed:?}");
    }
}
