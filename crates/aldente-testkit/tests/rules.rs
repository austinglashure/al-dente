//! Resolution rules on their fixtures under `fixtures/clj-rules/`, checked
//! against each fixture's hand-written `EXPECTED.md` table.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use aldente_graph::query::{Direction, neighborhood};
use aldente_model::{AggregationLevel, Certainty, EdgeKind, NodeId};
use aldente_testkit::{analyze_fixture, assert_graph_snapshot, fixture_dir};

/// `(from, to, certainty)` of every edge of `kind`, with `clj:.//` stripped.
///
/// Example: `("mm.use/circle-area", "mm.shapes/area@:circle", Likely)`.
fn edges_of(fixture: &str, kind: EdgeKind) -> BTreeSet<(String, String, Certainty)> {
    let g = analyze_fixture(&fixture_dir(fixture)).graph;
    let short = |id: &NodeId| id.to_string().trim_start_matches("clj:.//").to_owned();
    g.edges
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| {
            (
                short(&e.from),
                short(&e.to),
                e.certainty.expect("dispatches carry certainty"),
            )
        })
        .collect()
}

#[test]
fn r5_multimethod_dispatch_matches_expected() {
    use Certainty::{Likely, Possible};
    let all_area = [
        "area@:circle",
        "area@:square",
        "area@:default",
        "area@:triangle",
    ];
    let mut expected: BTreeSet<(String, String, Certainty)> = BTreeSet::new();
    let mut add = |from: &str, to: &str, c: Certainty| {
        expected.insert((format!("mm.use/{from}"), format!("mm.shapes/{to}"), c));
    };
    add("circle-area", "area@:circle", Likely);
    add("circle-area", "area@:default", Possible);
    for impl_ in all_area {
        add("unknown-area", impl_, Possible);
        add("as-value", impl_, Possible);
    }
    add("unmatched-area", "area@:default", Likely);
    add("pair", "describe@[String String]", Possible);
    add("pair", "describe@:default", Possible);
    add("french", "greet@:fr", Likely);
    add("poodle", "speak@:mm.shapes/dog", Possible);
    add("poodle", "speak@:default", Possible);
    let actual = edges_of("clj-rules/multimethods", EdgeKind::Dispatches);
    let missing: Vec<_> = expected.difference(&actual).collect();
    let extra: Vec<_> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing {missing:#?}\nextra {extra:#?}"
    );
}

#[test]
fn r5_a_forward_walk_from_a_caller_reaches_code_inside_the_methods() {
    let g = analyze_fixture(&fixture_dir("clj-rules/multimethods")).graph;
    let p = g.payload(AggregationLevel::Symbol);
    let from: NodeId = "clj:.//mm.use/unknown-area".parse().unwrap();
    let hood = neighborhood(&p, &from, 2, Direction::Out);
    let reached: BTreeSet<String> = hood.nodes.iter().map(|n| n.id.to_string()).collect();
    // `*` is called in the :circle and :square bodies, `/` in :triangle's.
    assert!(reached.contains("ext:clj//clojure.core/*"), "{reached:#?}");
    assert!(reached.contains("ext:clj//clojure.core//"), "{reached:#?}");
}

#[test]
fn multimethods_golden() {
    assert_graph_snapshot("clj-rules/multimethods", AggregationLevel::Symbol);
}
