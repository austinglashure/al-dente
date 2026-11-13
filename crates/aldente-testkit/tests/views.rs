//! The per-session cache of rolled-up views and the edge index must give
//! exactly what the uncached functions give.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::borrow::Cow;

use aldente_graph::cache::Cache;
use aldente_graph::query::{Direction, neighborhood, neighborhood_in, neighbors};
use aldente_model::{AggregationLevel, NodeId};
use aldente_testkit::{analyze_fixture, fixture_dir};

#[test]
fn rollups_are_computed_once_and_equal_the_uncached_ones() {
    let g = analyze_fixture(&fixture_dir("mono-mini")).graph;
    let cache = Cache::default();
    for level in [AggregationLevel::Namespace, AggregationLevel::Unit] {
        let first = cache.payload(&g, level);
        let second = cache.payload(&g, level);
        assert!(matches!(first, Cow::Borrowed(_)), "{level:?} is cached");
        assert!(
            std::ptr::eq(first.as_ref(), second.as_ref()),
            "{level:?} computed once"
        );
        assert_eq!(first.as_ref(), &g.payload(level));
    }
}

#[test]
fn neighbors_with_a_cache_equal_neighbors_without() {
    let g = analyze_fixture(&fixture_dir("clj-mini")).graph;
    let cache = Cache::default();
    for id in g.nodes.keys() {
        assert_eq!(
            neighbors(&g, &cache, id),
            neighbors(&g, &Cache::default(), id),
            "{id}"
        );
    }
}

#[test]
fn indexed_neighborhood_equals_the_payload_walk() {
    for fixture in ["clj-mini", "mono-mini"] {
        let g = analyze_fixture(&fixture_dir(fixture)).graph;
        let cache = Cache::default();
        let full = g.payload(AggregationLevel::Symbol);
        let ids: Vec<&NodeId> = g.nodes.keys().collect();
        for id in ids {
            for depth in [1, 2] {
                for dir in [Direction::Out, Direction::In, Direction::Both] {
                    let mut want = neighborhood(&full, id, depth, dir);
                    let mut got = neighborhood_in(&g, &cache, id, depth, dir);
                    for p in [&mut want, &mut got] {
                        p.nodes.sort_by(|a, b| a.id.cmp(&b.id));
                        p.edges.sort_by(|a, b| {
                            (&a.from, &a.to, a.kind, a.reason, a.lang)
                                .cmp(&(&b.from, &b.to, b.kind, b.reason, b.lang))
                        });
                    }
                    assert_eq!(got, want, "{fixture} {id} depth {depth} {dir:?}");
                }
            }
        }
    }
}

#[test]
fn indexed_expand_equals_the_scanning_expand() {
    use aldente_graph::query::{expand, expand_in};
    use aldente_model::NodeKind;
    for fixture in ["clj-mini", "mono-mini"] {
        let g = analyze_fixture(&fixture_dir(fixture)).graph;
        let cache = Cache::default();
        let aggregates: Vec<&NodeId> = g
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Namespace | NodeKind::Unit))
            .map(|n| &n.id)
            .collect();
        for id in &aggregates {
            // Visible: the other aggregates (what the canvas typically shows).
            let visible: Vec<NodeId> = aggregates
                .iter()
                .filter(|a| *a != id)
                .map(|a| (*a).clone())
                .collect();
            for limit in [0, 2] {
                assert_eq!(
                    expand_in(&g, &cache, id, &visible, limit),
                    expand(&g, id, &visible, limit),
                    "{fixture} {id} limit {limit}"
                );
            }
        }
    }
}
