//! Crater engine tests on small synthetic graphs.

use std::collections::BTreeSet;
use std::str::FromStr;

use aldente_graph::Graph;
use aldente_model::crater::{CraterResult, Direction};
use aldente_model::{Certainty, Edge, EdgeKind, Node, NodeId, NodeKind, Reason, SymbolFlag};
use proptest::prelude::*;

use super::{CraterIndex, CraterOptions, crater};

fn id(s: &str) -> NodeId {
    if s.contains(':') {
        NodeId::from_str(s).unwrap()
    } else {
        NodeId::from_str(&format!("clj:.//t/{s}")).unwrap()
    }
}

fn node(s: &str) -> Node {
    let id = id(s);
    Node {
        kind: id.kind(),
        id,
        subkind: None,
        name: s.to_owned(),
        namespace: Some("t".to_owned()),
        unit: Some(NodeId::Unit(aldente_model::RepoPath::root())),
        langs: BTreeSet::new(),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
    }
}

fn edge(from: &str, to: &str, kind: EdgeKind, c: Certainty) -> Edge {
    let structural = !kind.needs_certainty();
    Edge {
        from: id(from),
        to: id(to),
        kind,
        certainty: (!structural).then_some(c),
        reason: (!structural).then_some(Reason::Direct),
        lang: None,
        sites: vec![],
        count: None,
        by_certainty: None,
    }
}

fn calls(from: &str, to: &str, c: Certainty) -> Edge {
    edge(from, to, EdgeKind::Calls, c)
}

/// A graph with a node record for every endpoint that is a symbol or impl.
fn graph(edges: Vec<Edge>) -> Graph {
    let mut g = Graph::default();
    for e in &edges {
        for end in [&e.from, &e.to] {
            if matches!(end.kind(), NodeKind::Symbol | NodeKind::Impl) {
                let short = end.to_string().trim_start_matches("clj:.//t/").to_owned();
                g.nodes.entry(end.clone()).or_insert_with(|| {
                    let mut n = node(&short);
                    n.id = end.clone();
                    n
                });
            }
        }
    }
    g.edges = edges;
    g
}

fn run(g: &Graph, seeds: &[&str], opts: &CraterOptions) -> CraterResult {
    let index = CraterIndex::new(g);
    let seeds: Vec<NodeId> = seeds.iter().map(|s| id(s)).collect();
    crater(g, &index, &seeds, opts).unwrap()
}

fn ids(r: &CraterResult) -> BTreeSet<String> {
    r.reach
        .iter()
        .map(|x| x.id.to_string().trim_start_matches("clj:.//t/").to_owned())
        .collect()
}

fn set(xs: &[&str]) -> BTreeSet<String> {
    xs.iter().map(|s| (*s).to_owned()).collect()
}

fn all_levels() -> CraterOptions {
    CraterOptions {
        min_certainty: Certainty::Possible,
        ..CraterOptions::default()
    }
}

use Certainty::{Certain, Likely, Possible};

#[test]
fn cycles_terminate() {
    let g = graph(vec![
        calls("a", "b", Certain),
        calls("b", "c", Certain),
        calls("c", "a", Certain),
    ]);
    let r = run(
        &g,
        &["a"],
        &CraterOptions {
            depth: 0,
            ..all_levels()
        },
    );
    assert_eq!(ids(&r), set(&["a", "b", "c"]));
    assert!(!r.truncated);
}

#[test]
fn a_certain_long_path_beats_a_possible_short_one() {
    let g = graph(vec![
        calls("a", "b", Certain),
        calls("b", "c", Certain),
        calls("c", "d", Certain),
        calls("a", "d", Possible),
    ]);
    let r = run(&g, &["a"], &all_levels());
    let d = r.reach.iter().find(|x| x.id == id("d")).unwrap();
    assert_eq!(d.path_certainty, Certain);
    assert_eq!(d.distance, 3);
    assert_eq!(d.via, Some(id("c")));
}

#[test]
fn reverse_from_an_impl_reaches_only_callers_that_dispatch_to_it() {
    let g = graph(vec![
        edge("m@one", "m", EdgeKind::Implements, Certain),
        edge("m@two", "m", EdgeKind::Implements, Certain),
        calls("x", "m", Certain),
        edge("x", "m@one", EdgeKind::Dispatches, Likely),
        calls("y", "m", Certain),
        edge("y", "m@two", EdgeKind::Dispatches, Likely),
        calls("w", "x", Certain),
    ]);
    let r = run(
        &g,
        &["clj:.//t/m@one"],
        &CraterOptions {
            direction: Direction::Reverse,
            ..all_levels()
        },
    );
    let got = ids(&r);
    assert!(got.contains("x") && got.contains("w"));
    assert!(!got.contains("y"), "y dispatches only to m@two: {got:?}");
    assert!(!got.contains("m@two"));
}

#[test]
fn a_crater_seeded_at_a_multimethod_continues_into_its_impls() {
    let g = graph(vec![
        edge("m@one", "m", EdgeKind::Implements, Certain),
        edge("m@two", "m", EdgeKind::Implements, Certain),
        calls("m@one", "helper", Certain),
        calls("x", "m", Certain),
    ]);
    let r = run(&g, &["m"], &all_levels());
    assert_eq!(ids(&r), set(&["m", "m@one", "m@two", "helper"]));
    // Reaching the multimethod through a call does not fan out to its impls:
    // the caller's own `dispatches` edges say which impls it may run.
    let r = run(&g, &["x"], &all_levels());
    assert_eq!(ids(&r), set(&["x", "m"]));
}

#[test]
fn hub_damping_stops_at_the_hub_and_lists_it() {
    let g = graph(vec![
        calls("a", "log", Certain),
        calls("b", "log", Certain),
        calls("c", "log", Certain),
        calls("log", "z", Certain),
    ]);
    let opts = CraterOptions {
        hub_degree: Some(3),
        ..all_levels()
    };
    let r = run(&g, &["a"], &opts);
    assert_eq!(ids(&r), set(&["a", "log"]));
    assert_eq!(r.hubs, vec![id("log")]);
    let through = CraterOptions {
        through_hubs: true,
        ..opts
    };
    assert!(ids(&run(&g, &["a"], &through)).contains("z"));
}

#[test]
fn a_seed_that_is_a_hub_is_still_expanded() {
    let g = graph(vec![
        calls("a", "log", Certain),
        calls("b", "log", Certain),
        calls("log", "z", Certain),
    ]);
    let r = run(
        &g,
        &["log"],
        &CraterOptions {
            hub_degree: Some(2),
            ..all_levels()
        },
    );
    assert_eq!(ids(&r), set(&["log", "z"]));
    assert!(r.hubs.is_empty());
}

#[test]
fn min_certainty_hides_weaker_nodes_and_counts_them() {
    let g = graph(vec![
        calls("a", "b", Likely),
        calls("b", "c", Possible),
        calls("c", "d", Certain),
    ]);
    let r = run(&g, &["a"], &CraterOptions::default());
    assert_eq!(ids(&r), set(&["a", "b"]));
    assert_eq!(r.hidden_below_min, 2);
    let b = r.reach.iter().find(|x| x.id == id("b")).unwrap();
    assert_eq!(b.path_certainty, Likely);
    assert_eq!(r.counts.by_certainty.get(&Likely), Some(&1));
    assert_eq!(r.graph.edges.len(), 1);
}

#[test]
fn depth_limit_truncates_and_says_so() {
    let g = graph(vec![
        calls("a", "b", Certain),
        calls("b", "c", Certain),
        calls("c", "d", Certain),
    ]);
    let r = run(
        &g,
        &["a"],
        &CraterOptions {
            depth: 2,
            ..all_levels()
        },
    );
    assert_eq!(ids(&r), set(&["a", "b", "c"]));
    assert!(r.truncated);
    let r = run(
        &g,
        &["a"],
        &CraterOptions {
            depth: 3,
            ..all_levels()
        },
    );
    assert!(!r.truncated);
}

#[test]
fn node_cap_truncates() {
    let g = graph(vec![
        calls("a", "b", Certain),
        calls("a", "c", Certain),
        calls("a", "d", Certain),
    ]);
    let r = run(
        &g,
        &["a"],
        &CraterOptions {
            node_cap: 2,
            ..all_levels()
        },
    );
    assert_eq!(r.reach.len(), 2);
    assert!(r.truncated);
}

#[test]
fn externals_platforms_and_holes_form_the_boundary() {
    let g = graph(vec![
        calls("a", "ext:clj//lib.core/f", Certain),
        calls("a", "java:java.io.File#delete", Certain),
        calls("b", "ext:clj//lib.core/f", Certain),
    ]);
    let r = run(&g, &["a"], &all_levels());
    assert_eq!(
        r.boundary,
        vec![id("ext:clj//lib.core/f"), id("java:java.io.File#delete")]
    );
    // An external seed is expanded even though externals stop the search.
    let r = run(
        &g,
        &["ext:clj//lib.core/f"],
        &CraterOptions {
            direction: Direction::Reverse,
            ..all_levels()
        },
    );
    assert!(ids(&r).contains("a") && ids(&r).contains("b"));
}

#[test]
fn stop_patterns_include_but_do_not_expand() {
    let g = graph(vec![
        calls("a", "log-it", Certain),
        calls("log-it", "z", Certain),
    ]);
    let r = run(
        &g,
        &["a"],
        &CraterOptions {
            stop_at: vec!["*log*".to_owned()],
            ..all_levels()
        },
    );
    assert_eq!(ids(&r), set(&["a", "log-it"]));
    assert_eq!(r.stopped, vec![id("log-it")]);
}

#[test]
fn tests_can_be_excluded() {
    let mut g = graph(vec![calls("t1", "a", Certain), calls("b", "a", Certain)]);
    g.nodes
        .get_mut(&id("t1"))
        .unwrap()
        .flags
        .insert(SymbolFlag::Test);
    let reverse = CraterOptions {
        direction: Direction::Reverse,
        ..all_levels()
    };
    assert!(ids(&run(&g, &["a"], &reverse)).contains("t1"));
    let r = run(
        &g,
        &["a"],
        &CraterOptions {
            exclude_tests: true,
            ..reverse
        },
    );
    assert_eq!(ids(&r), set(&["a", "b"]));
}

#[test]
fn both_directions_merge() {
    let g = graph(vec![calls("a", "b", Certain), calls("b", "c", Likely)]);
    let r = run(
        &g,
        &["b"],
        &CraterOptions {
            direction: Direction::Both,
            ..all_levels()
        },
    );
    assert_eq!(ids(&r), set(&["a", "b", "c"]));
    let a = r.reach.iter().find(|x| x.id == id("a")).unwrap();
    assert_eq!(a.direction, Direction::Reverse);
    let c = r.reach.iter().find(|x| x.id == id("c")).unwrap();
    assert_eq!(
        (c.direction, c.path_certainty),
        (Direction::Forward, Likely)
    );
}

#[test]
fn a_namespace_seed_stands_for_its_symbols() {
    let mut g = graph(vec![calls("a", "b", Certain), calls("x", "a", Certain)]);
    let ns = id("clj:.//t");
    let mut n = node("t");
    n.id = ns.clone();
    n.kind = NodeKind::Namespace;
    g.nodes.insert(ns.clone(), n);
    g.edges
        .push(edge("clj:.//t", "a", EdgeKind::Contains, Certain));
    let r = run(&g, &["clj:.//t"], &all_levels());
    assert_eq!(ids(&r), set(&["clj:.//t", "a", "b"]));
}

#[test]
fn unknown_seeds_are_an_error() {
    let g = graph(vec![calls("a", "b", Certain)]);
    let index = CraterIndex::new(&g);
    assert!(crater(&g, &index, &[id("nope")], &CraterOptions::default()).is_err());
}

#[test]
fn a_crater_on_500k_edges_takes_under_a_second() {
    let n: u64 = 100_000;
    let mut g = Graph::default();
    for i in 0..n {
        let n = node(&format!("f{i}"));
        g.nodes.insert(n.id.clone(), n);
    }
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let levels = [Certain, Likely, Possible];
    for _ in 0..500_000 {
        let (a, b) = (next() % n, next() % n);
        let c = levels[(next() % 3) as usize];
        g.edges.push(calls(&format!("f{a}"), &format!("f{b}"), c));
    }
    let index = CraterIndex::new(&g);
    let opts = CraterOptions {
        depth: 0,
        min_certainty: Possible,
        node_cap: usize::MAX,
        ..CraterOptions::default()
    };
    let start = std::time::Instant::now();
    let r = crater(&g, &index, &[id("f0")], &opts).unwrap();
    let elapsed = start.elapsed();
    assert!(r.reach.len() > 90_000, "reached {}", r.reach.len());
    eprintln!("crater on 500k edges took {elapsed:?}");
    // The budget is for the shipped (release) build; an unoptimized test
    // build is several times slower, so it only checks the result there.
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_secs_f64() < 1.0, "took {elapsed:?}");
    }
}

/// Small random graphs over symbols `s0..s7` and impls, with every edge kind
/// the crater follows plus `implements`.
fn arb_graph() -> impl Strategy<Value = Graph> {
    let names: Vec<String> = (0..6)
        .map(|i| format!("s{i}"))
        .chain((0..3).map(|i| format!("s0@{i}")))
        .collect();
    let n = names.len();
    let kinds = prop::sample::select(vec![
        EdgeKind::Calls,
        EdgeKind::References,
        EdgeKind::Dispatches,
        EdgeKind::Registers,
        EdgeKind::Implements,
    ]);
    let certainty = prop::sample::select(vec![Certain, Likely, Possible]);
    prop::collection::vec((0..n, 0..n, kinds, certainty), 0..24).prop_map(move |es| {
        let mut g = graph(
            es.into_iter()
                .map(|(a, b, k, c)| edge(&names[a], &names[b], k, c))
                .collect(),
        );
        for name in &names {
            g.nodes.entry(id(name)).or_insert_with(|| {
                let mut nd = node(name);
                nd.kind = id(name).kind();
                nd
            });
        }
        g
    })
}

proptest! {
    #[test]
    fn forward_and_reverse_craters_agree(
        g in arb_graph(),
        min in prop::sample::select(vec![Certain, Likely, Possible]),
    ) {
        let index = CraterIndex::new(&g);
        let ids: Vec<NodeId> = g.nodes.keys().cloned().collect();
        let opts = |direction| CraterOptions {
            direction, depth: 0, min_certainty: min, ..CraterOptions::default()
        };
        let forward: Vec<BTreeSet<NodeId>> = ids.iter().map(|a| {
            crater(&g, &index, std::slice::from_ref(a), &opts(Direction::Forward))
                .unwrap().reach.into_iter().map(|r| r.id).collect()
        }).collect();
        let reverse: Vec<BTreeSet<NodeId>> = ids.iter().map(|b| {
            crater(&g, &index, std::slice::from_ref(b), &opts(Direction::Reverse))
                .unwrap().reach.into_iter().map(|r| r.id).collect()
        }).collect();
        for (i, a) in ids.iter().enumerate() {
            for (j, b) in ids.iter().enumerate() {
                prop_assert_eq!(
                    forward[i].contains(b), reverse[j].contains(a),
                    "forward({}) ∋ {} vs reverse({}) ∋ {}", a, b, b, a
                );
            }
        }
    }
}
