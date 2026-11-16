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
    use aldente_graph::query::{Page, expand, expand_in};
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
            for (offset, limit) in [(0, 0), (0, 2), (2, 2)] {
                let page = Page { offset, limit };
                assert_eq!(
                    expand_in(&g, &cache, id, &visible, page),
                    expand(&g, id, &visible, page),
                    "{fixture} {id} {page:?}"
                );
            }
        }
    }
}

/// WP5.2: `unit_of` and `namespace_of` read a namespace, symbol or impl ID
/// instead of looking the node up; on every fixture that must equal what
/// the node itself says.
#[test]
fn groups_read_from_ids_equal_the_stored_node_fields() {
    use aldente_model::{Lang, NamespaceId};
    for fixture in ["clj-mini", "mono-mini", "clj-rules"] {
        let g = analyze_fixture(&fixture_dir(fixture)).graph;
        // Edge ends with a unit in their ID are nodes of the graph, so
        // reading the unit from the ID never invents one.
        for e in &g.edges {
            for end in [&e.from, &e.to] {
                if matches!(
                    end,
                    NodeId::Namespace(_) | NodeId::Symbol(_) | NodeId::Impl(_)
                ) {
                    assert!(g.nodes.contains_key(end), "{fixture}: {end} is not a node");
                }
            }
        }
        for n in g.nodes.values() {
            if !matches!(
                n.id,
                NodeId::Namespace(_) | NodeId::Symbol(_) | NodeId::Impl(_)
            ) {
                continue;
            }
            assert_eq!(
                g.unit_of(&n.id),
                n.unit.clone(),
                "{fixture} unit of {}",
                n.id
            );
            if matches!(n.id, NodeId::Symbol(_) | NodeId::Impl(_)) {
                let NodeId::Unit(dir) = n.unit.clone().unwrap() else {
                    panic!("{} has no unit", n.id);
                };
                let stored = NamespaceId::new(Lang::Clj, dir, n.namespace.as_deref().unwrap())
                    .map(NodeId::Namespace)
                    .ok();
                assert_eq!(
                    g.namespace_of(&n.id),
                    stored,
                    "{fixture} namespace of {}",
                    n.id
                );
            }
        }
    }
}

/// WP5.5: `:local/root` links units (`depends`), the unit view keeps those
/// links and says how many symbols each unit has, and a unit's links say
/// how many symbols it uses from each other unit.
#[test]
fn units_link_by_local_root_and_by_calls() {
    use aldente_graph::query::unit_links;
    use aldente_model::EdgeKind;
    let g = analyze_fixture(&fixture_dir("mono-mini")).graph;
    let orders: NodeId = "unit:services/orders".parse().unwrap();
    let common: NodeId = "unit:libs/common".parse().unwrap();
    let depends = |p: &[aldente_model::Edge]| {
        p.iter()
            .any(|e| e.kind == EdgeKind::Depends && e.from == orders && e.to == common)
    };
    assert!(
        depends(&g.edges),
        "the stored graph has orders -depends-> common"
    );
    let cache = Cache::default();
    let units = cache.payload(&g, AggregationLevel::Unit);
    assert!(depends(&units.edges), "the unit view keeps it");
    for n in &units.nodes {
        assert!(
            n.symbols.is_some_and(|c| c > 0),
            "{} has a symbol count",
            n.id
        );
    }
    let links = unit_links(&g, &orders);
    let to_common = links
        .uses
        .iter()
        .find(|l| l.unit == common)
        .expect("orders uses common");
    assert!(to_common.depends);
    assert!(to_common.symbols >= 1 && to_common.calls >= to_common.symbols);
    let back = unit_links(&g, &common);
    assert!(back.used_by.iter().any(|l| l.unit == orders));
}
