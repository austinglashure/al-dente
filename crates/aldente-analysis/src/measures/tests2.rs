//! Second-set tests: hand-computed small graphs, Zachary's karate club
//! against networkx 3.7, sampled betweenness, determinism, dead code on
//! clj-mini.

use std::collections::BTreeSet;
use std::str::FromStr;

use aldente_model::measures::{ProjectionSpec, Sampling};
use aldente_model::{AggregationLevel, NodeId};

use super::paths::{betweenness_closeness, pivots};
use super::projection::{Ix, Projection};
use super::{coupling, cuts, deadcode, dominators, eigen, god, louvain, scc};

fn id(s: &str) -> NodeId {
    NodeId::from_str(&format!("clj:.//t/{s}")).unwrap()
}

/// A projection over nodes `n0..` from certain arcs between positions.
fn proj(n: usize, arcs: &[(Ix, Ix)]) -> Projection {
    let ids = (0..n).map(|i| id(&format!("n{i:04}"))).collect();
    let arcs: Vec<(Ix, Ix, f64, u32)> = arcs.iter().map(|&(a, b)| (a, b, 1.0, 1)).collect();
    Projection::from_arcs(ids, &arcs)
}

fn both(arcs: &[(Ix, Ix)]) -> Vec<(Ix, Ix)> {
    arcs.iter().flat_map(|&(a, b)| [(a, b), (b, a)]).collect()
}

const EXACT: Sampling = Sampling {
    approximate: false,
    pivots: 0,
    seed: 0,
};

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn chain_and_diamond_match_hand_values() {
    // a → b → c: one path through b, normalized by (n−1)(n−2) = 2.
    let p = proj(3, &[(0, 1), (1, 2)]);
    let (b, c) = betweenness_closeness(&p, EXACT);
    assert_eq!(b, vec![0.0, 0.5, 0.0]);
    assert_eq!(c, vec![0.0, 1.0, 1.5]);
    assert_eq!(dominators::dominated_counts(&p, &[0]), vec![2, 1, 0]);
    let (art, bridges) = cuts::articulation_and_bridges(&p.undirected());
    assert_eq!(art, vec![false, true, false]);
    assert_eq!(bridges, vec![(0, 1), (1, 2)]);
    // Diamond e → x, e → y, x → z, y → z from entry e: two shortest e–z
    // paths, x on one: 0.5 / 6.
    let p = proj(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]);
    let (b, c) = betweenness_closeness(&p, EXACT);
    assert!(close(b[1], 0.5 / 6.0, 1e-12) && close(b[2], 0.5 / 6.0, 1e-12));
    assert!(close(c[3], 1.0 + 1.0 + 0.5, 1e-12));
    assert_eq!(dominators::dominated_counts(&p, &[0]), vec![3, 0, 0, 0]);
    let (art, bridges) = cuts::articulation_and_bridges(&p.undirected());
    assert_eq!(art, vec![false; 4]);
    assert!(bridges.is_empty());
    // Unreachable from the entry: dominates nothing.
    let p = proj(3, &[(0, 1), (2, 1)]);
    assert_eq!(dominators::dominated_counts(&p, &[0]), vec![1, 0, 0]);
}

#[test]
fn eigenvector_runs_per_component() {
    let p = proj(4, &[(0, 1), (1, 2), (2, 0), (2, 3)]);
    let comps = scc::components(&p);
    let (e, conv) = eigen::eigenvector(&p, &comps);
    for v in &e[..3] {
        assert!(close(*v, 1.0 / 3f64.sqrt(), 1e-9), "{v}");
    }
    assert_eq!(e[3], 0.0, "outside any cycle");
    assert!(conv[0].converged);
    let (e, conv) = eigen::eigenvector(&proj(2, &[(0, 1)]), &[]);
    assert_eq!(e, vec![0.0, 0.0]);
    assert!(conv.is_empty());
}

#[test]
fn louvain_splits_two_triangles_and_is_deterministic() {
    let arcs = both(&[(0, 1), (1, 2), (0, 2), (3, 4), (4, 5), (3, 5), (2, 3)]);
    let p = proj(6, &arcs);
    let (c, q) = louvain::louvain(&p, 7);
    assert_eq!(c, vec![0, 0, 0, 1, 1, 1]);
    // Two communities of 7 edges: 2 × (3/7 − (7/14)²).
    assert!(close(q, 2.0 * (3.0 / 7.0 - 0.25), 1e-12), "{q}");
    for seed in [1, 2, 3] {
        assert_eq!(louvain::louvain(&p, seed), louvain::louvain(&p, seed));
    }
}

#[test]
fn coupling_and_layers_match_hand_values() {
    // a → b → c, plus a cycle c ⇄ d.
    let p = proj(4, &[(0, 1), (1, 2), (2, 3), (3, 2)]);
    let comps = scc::components(&p);
    assert_eq!(coupling::layers(&p, &comps), vec![2, 1, 0, 0]);
    assert_eq!(
        god::percentile_ranks(&[1.0, 2.0, 2.0, 3.0]),
        vec![0.0, 0.25, 0.25, 0.75]
    );
}

/// Zachary's karate club, undirected (each edge both ways).
fn karate() -> Projection {
    let arcs: Vec<(Ix, Ix)> = super::tests::KARATE.to_vec();
    proj(34, &both(&arcs))
}

/// `nx.betweenness_centrality(G)` on the unweighted club.
const KARATE_BETWEENNESS: [f64; 34] = [
    0.437635281,
    0.053936688,
    0.143656806,
    0.011909271,
    0.000631313,
    0.029987374,
    0.029987374,
    0.0,
    0.055926828,
    0.000847763,
    0.000631313,
    0.0,
    0.0,
    0.045863396,
    0.0,
    0.0,
    0.0,
    0.0,
    0.0,
    0.032475048,
    0.0,
    0.0,
    0.0,
    0.017613636,
    0.002209596,
    0.003840488,
    0.0,
    0.022333454,
    0.001794733,
    0.002922078,
    0.014411977,
    0.138275613,
    0.145247114,
    0.304074976,
];

/// `nx.harmonic_centrality(G)`.
const KARATE_HARMONIC: [f64; 34] = [
    23.166666667,
    19.166666667,
    21.0,
    17.666666667,
    14.666666667,
    15.166666667,
    15.166666667,
    16.416666667,
    18.5,
    15.583333333,
    14.666666667,
    13.5,
    14.0,
    18.5,
    14.2,
    14.2,
    11.1,
    14.166666667,
    14.2,
    17.5,
    14.2,
    14.166666667,
    14.2,
    16.033333333,
    13.916666667,
    13.916666667,
    13.95,
    16.916666667,
    16.416666667,
    15.366666667,
    16.916666667,
    19.333333333,
    20.916666667,
    23.25,
];

/// `nx.eigenvector_centrality(G, tol=1e-12)`.
const KARATE_EIGENVECTOR: [f64; 34] = [
    0.355491445,
    0.26595992,
    0.317192504,
    0.21117972,
    0.075968818,
    0.079483045,
    0.079483045,
    0.170959748,
    0.227403907,
    0.102674251,
    0.075968818,
    0.052855697,
    0.084254629,
    0.22647272,
    0.101403262,
    0.101403262,
    0.023635628,
    0.092399538,
    0.101403262,
    0.14791251,
    0.101403262,
    0.092399538,
    0.101403262,
    0.150118572,
    0.057052441,
    0.059206475,
    0.075579413,
    0.133477153,
    0.131077823,
    0.134960819,
    0.174758302,
    0.191033841,
    0.30864422,
    0.37336347,
];

#[test]
fn karate_club_matches_networkx() {
    let p = karate();
    let (b, h) = betweenness_closeness(&p, EXACT);
    let (e, conv) = eigen::eigenvector(&p, &scc::components(&p));
    assert!(conv[0].converged);
    for i in 0..34 {
        assert!(
            close(b[i], KARATE_BETWEENNESS[i], 2e-9),
            "betweenness {i}: {}",
            b[i]
        );
        assert!(
            close(h[i], KARATE_HARMONIC[i], 2e-9),
            "harmonic {i}: {}",
            h[i]
        );
        assert!(
            close(e[i], KARATE_EIGENVECTOR[i], 1e-6),
            "eigenvector {i}: {}",
            e[i]
        );
    }
    // `nx.articulation_points` and `nx.bridges`.
    let (art, bridges) = cuts::articulation_and_bridges(&p.undirected());
    let cut: Vec<usize> = (0..34).filter(|&i| art[i]).collect();
    assert_eq!(cut, vec![0]);
    assert_eq!(bridges, vec![(0, 11)]);
    // networkx's Louvain reaches 0.4151 to 0.4188 over seeds 0–4.
    let (_, q) = louvain::louvain(&p, louvain::DEFAULT_SEED);
    assert!(q > 0.40, "modularity {q}");
}

#[test]
fn sampled_betweenness_finds_the_true_top_ten() {
    // An undirected preferential-attachment graph (two links per new node).
    let n = 3000;
    let mut rng = super::paths::Rng::new(11);
    let mut ends: Vec<Ix> = vec![0, 1];
    let mut arcs = vec![(0, 1)];
    for v in 2..n as Ix {
        let mut picked = BTreeSet::new();
        while picked.len() < 2 {
            picked.insert(ends[rng.below(ends.len())]);
        }
        for t in picked {
            arcs.push((v, t));
            ends.extend([v, t]);
        }
    }
    let p = proj(n, &both(&arcs));
    let top = |b: &[f64], k: usize| -> BTreeSet<usize> {
        let mut ix: Vec<usize> = (0..b.len()).collect();
        ix.sort_by(|&x, &y| b[y].total_cmp(&b[x]).then(x.cmp(&y)));
        ix.into_iter().take(k).collect()
    };
    let (exact, _) = betweenness_closeness(&p, EXACT);
    let sampled = Sampling {
        approximate: true,
        pivots: 200,
        seed: 3,
    };
    let (approx, _) = betweenness_closeness(&p, sampled);
    let missing: Vec<usize> = top(&exact, 10)
        .difference(&top(&approx, 20))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "true top ten not in the sampled top twenty: {missing:?}"
    );
    assert_eq!(pivots(10, 4, 9), pivots(10, 4, 9));
    assert_eq!(pivots(10, 4, 9).len(), 4);
}

#[test]
fn dead_code_on_clj_mini_equals_the_hand_list() {
    let g = aldente_testkit::analyze_fixture(&aldente_testkit::fixture_dir("clj-mini")).graph;
    let report = deadcode::dead_code(&g);
    let dead: BTreeSet<String> = report
        .dead
        .iter()
        .map(|d| d.id.to_string().trim_start_matches("clj:.//").to_owned())
        .collect();
    // fixtures/clj-mini/EXPECTED.md, "Dead code".
    let want: BTreeSet<String> = [
        "shop.interop/new-id",
        "shop.interop/shout",
        "shop.inventory/restock!",
        "shop.inventory/available",
        "shop.payment/free-charger",
        "shop.payment/Voucher",
        "shop.payment/->Voucher",
        "shop.payment/map->Voucher",
        "shop.payment/map->Card",
        "shop.plugins/load-plugin",
        "shop.plugins/run-script",
        "shop.ui.views/cart-view",
        "shop.ui.views/submit-button",
        "shop.ui.events/reg-event-fx@:order/submit",
        "shop.ui.events/reg-event-db@:order/submitted",
        "shop.ui.subs/reg-sub@:cart/items",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(dead, want);
}

#[test]
fn second_set_and_coupling_run_on_clj_mini() {
    let g = aldente_testkit::analyze_fixture(&aldente_testkit::fixture_dir("clj-mini")).graph;
    let spec = ProjectionSpec::default();
    let r = super::second_set(&g, &spec, super::SecondOptions::default()).unwrap();
    assert!(!r.sampling.approximate);
    assert_eq!(r.entry_points, 2, "-main and init");
    let main = r
        .nodes
        .iter()
        .find(|n| n.id.to_string().ends_with("shop.core/-main"))
        .unwrap();
    assert!(main.dominates > 0);
    let again = super::second_set(&g, &spec, super::SecondOptions::default()).unwrap();
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        serde_json::to_string(&again).unwrap()
    );
    let ns = ProjectionSpec {
        level: AggregationLevel::Namespace,
        ..spec
    };
    let c = coupling::coupling(&g, &ns).unwrap();
    let get = |n: &str| {
        c.modules
            .iter()
            .find(|m| m.id.to_string().ends_with(n))
            .unwrap()
    };
    // shop.core depends on others and nothing depends on it.
    assert_eq!(get("//shop.core").afferent, 0);
    assert_eq!(get("//shop.core").instability, 1.0);
    // One protocol among shop.payment's definitions.
    assert!(get("//shop.payment").abstractness > 0.0);
    assert!(get("//shop.core").layer > get("//shop.util").layer);
}

/// No time budget in the plan for the second set; this guards against a
/// quadratic regression (it took about 15 s in release on 200k nodes / 1M
/// edges, sampled betweenness and Louvain being most of it).
#[test]
fn second_set_on_200k_nodes_stays_under_a_minute() {
    let n: usize = if cfg!(debug_assertions) {
        2_000
    } else {
        200_000
    };
    let mut rng = super::paths::Rng::new(5);
    let arcs: Vec<(Ix, Ix)> = (0..n * 5)
        .map(|_| (rng.below(n) as Ix, rng.below(n) as Ix))
        .collect();
    let p = proj(n, &arcs);
    let g = aldente_graph::Graph::default();
    let t = std::time::Instant::now();
    let r = super::second_set_on(
        &g,
        &p,
        ProjectionSpec::default(),
        super::SecondOptions::default(),
    );
    let elapsed = t.elapsed();
    eprintln!("second set on {n} nodes took {elapsed:?}");
    assert_eq!(r.nodes.len(), n);
    assert_eq!(r.sampling.approximate, n > super::paths::EXACT_UP_TO);
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_secs_f64() < 60.0, "took {elapsed:?}");
    }
}
