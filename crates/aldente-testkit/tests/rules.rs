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

#[test]
fn r6_protocol_dispatch_matches_expected() {
    use Certainty::{Likely, Possible};
    let types = [
        "Circle",
        "Square",
        "Tri",
        "nil",
        "Object",
        "java.util.Map",
        "reify:L36",
    ];
    let mut expected: BTreeSet<(String, String, Certainty)> = BTreeSet::new();
    let mut add = |from: &str, to: String, c: Certainty| {
        expected.insert((format!("pr.use/{from}"), format!("pr.shapes/{to}"), c));
    };
    add("circle-area", "area@Circle".into(), Likely);
    add("square-area", "area@Square".into(), Likely);
    add("tri-area", "area@Tri".into(), Likely);
    for t in types {
        add("any-area", format!("area@{t}"), Possible);
        add("blob-label", format!("label@{t}"), Possible);
    }
    let actual = edges_of("clj-rules/protocols", EdgeKind::Dispatches);
    let missing: Vec<_> = expected.difference(&actual).collect();
    let extra: Vec<_> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing {missing:#?}\nextra {extra:#?}"
    );
}

#[test]
fn r6_impls_implement_their_protocol_and_reify_bodies_are_scoped() {
    let g = analyze_fixture(&fixture_dir("clj-rules/protocols")).graph;
    let implements: BTreeSet<String> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Implements && e.to.to_string() == "clj:.//pr.shapes/Shape")
        .map(|e| e.from.to_string())
        .collect();
    assert_eq!(implements.len(), 14, "{implements:#?}");
    let inc_callers: Vec<String> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Calls && e.to.to_string() == "ext:clj//clojure.core/inc")
        .map(|e| e.from.to_string())
        .collect();
    assert_eq!(inc_callers, ["clj:.//pr.shapes/area@reify:L36"]);
    let area = "clj:.//pr.shapes/area".parse::<NodeId>().unwrap();
    assert_eq!(
        aldente_graph::query::children(&g, &area).len(),
        7,
        "show <protocol-method> lists its impls"
    );
}

#[test]
fn multimethod_impls_implement_their_multimethod() {
    let g = analyze_fixture(&fixture_dir("clj-rules/multimethods")).graph;
    let n = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Implements && e.to.to_string() == "clj:.//mm.shapes/area")
        .count();
    assert_eq!(n, 4);
}

#[test]
fn protocols_golden() {
    assert_graph_snapshot("clj-rules/protocols", AggregationLevel::Symbol);
}

#[test]
fn r7_layer_1_values_handed_on_are_likely_references() {
    let refs = edges_of("clj-rules/higher-order", EdgeKind::References);
    for (from, to) in [
        ("mapped", "inc2"),
        ("composed", "inc2"),
        ("composed", "dbl"),
        ("partial-dbl", "dbl"),
        ("table", "inc2"),
        ("returns-fn", "dbl"),
        ("use-apply", "inc2"),
    ] {
        let edge = (
            format!("ho.core/{from}"),
            format!("ho.core/{to}"),
            Certainty::Likely,
        );
        assert!(refs.contains(&edge), "missing {edge:?}");
    }
}

#[test]
fn r7_layer_2_param_flow_matches_expected() {
    let g = analyze_fixture(&fixture_dir("clj-rules/higher-order")).graph;
    let flows: BTreeSet<(String, String, Option<Certainty>)> = g
        .edges
        .iter()
        .filter(|e| e.reason == Some(aldente_model::Reason::ParamFlow))
        .map(|e| {
            assert_eq!(e.kind, EdgeKind::Calls);
            let short = |id: &NodeId| {
                id.to_string()
                    .trim_start_matches("clj:.//ho.core/")
                    .to_owned()
            };
            (short(&e.from), short(&e.to), e.certainty)
        })
        .collect();
    let p = Some(Certainty::Possible);
    let expected: BTreeSet<(String, String, Option<Certainty>)> =
        [("apply-it", "inc2"), ("apply-it", "dbl"), ("twice", "dbl")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned(), p))
            .collect();
    assert_eq!(flows, expected);
}

#[test]
fn higher_order_golden() {
    assert_graph_snapshot("clj-rules/higher-order", AggregationLevel::Symbol);
}

#[test]
fn r8_dynamic_code_matches_expected() {
    use aldente_model::Reason;
    let g = analyze_fixture(&fixture_dir("clj-rules/dynamic")).graph;
    let short = |id: &NodeId| id.to_string().trim_start_matches("clj:.//").to_owned();
    let interesting: BTreeSet<(String, String, Option<Certainty>, Option<Reason>)> = g
        .edges
        .iter()
        .filter(|e| matches!(e.reason, Some(Reason::QuotedResolve | Reason::Dynamic)))
        .map(|e| (short(&e.from), short(&e.to), e.certainty, e.reason))
        .collect();
    let q = |f: &str, t: &str| {
        (
            format!("dyn.core/{f}"),
            format!("dyn.target/{t}"),
            Some(Certainty::Likely),
            Some(Reason::QuotedResolve),
        )
    };
    let h = |f: &str, cat: &str, at: &str| {
        (
            format!("dyn.core/{f}"),
            format!("hole:{cat}@src/dyn/core.clj:{at}"),
            Some(Certainty::Certain),
            Some(Reason::Dynamic),
        )
    };
    let expected: BTreeSet<_> = [
        q("by-resolve", "handler"),
        q("by-requiring-resolve", "other"),
        q("by-ns-resolve", "handler"),
        q("by-find-var", "other"),
        h("computed", "dynamic-resolve", "12:23"),
        h("computed-ns", "dynamic-resolve", "14:28"),
        h("evaluate", "dynamic-eval", "16:20"),
        h("load-it", "dynamic-eval", "18:19"),
        h("load-a-file", "dynamic-eval", "20:26"),
        h("load-reader-it", "dynamic-eval", "22:26"),
        h("load-lib", "dynamic-eval", "24:19"),
    ]
    .into_iter()
    .collect();
    assert_eq!(interesting, expected);
    let holes = g
        .nodes
        .values()
        .filter(|n| n.kind == aldente_model::NodeKind::Hole)
        .count();
    assert_eq!(holes, 7);
    let hole_spots = g
        .blindspots
        .iter()
        .filter(|b| {
            matches!(
                b.category,
                aldente_model::BlindSpotCategory::DynamicEval
                    | aldente_model::BlindSpotCategory::DynamicResolve
            )
        })
        .count();
    assert_eq!(hole_spots, 7, "every hole is a blind spot");
    for var in ["counter", "made", "handler"] {
        let id: NodeId = format!("clj:.//dyn.target/{var}").parse().unwrap();
        let node = g.nodes.get(&id).unwrap_or_else(|| panic!("no node {var}"));
        assert!(
            node.flags.contains(&aldente_model::SymbolFlag::Dynamic),
            "{var} not flagged dynamic"
        );
    }
}

#[test]
fn dynamic_golden() {
    assert_graph_snapshot("clj-rules/dynamic", AggregationLevel::Symbol);
}

#[test]
fn r9_interop_exits_match_expected() {
    use aldente_model::Reason;
    let g = analyze_fixture(&fixture_dir("clj-rules/interop")).graph;
    let short = |id: &NodeId| id.to_string().trim_start_matches("clj:.//").to_owned();
    let to_platform: BTreeSet<(String, EdgeKind, String)> = g
        .edges
        .iter()
        .filter(|e| matches!(e.to, NodeId::Platform(_)))
        .inspect(|e| {
            if e.kind != EdgeKind::Requires {
                assert_eq!(
                    (e.certainty, e.reason),
                    (Some(Certainty::Certain), Some(Reason::Interop)),
                    "{e:?}"
                );
            }
        })
        .map(|e| (short(&e.from), e.kind, e.to.to_string()))
        .collect();
    let c = EdgeKind::Calls;
    let r = EdgeKind::References;
    let expected: BTreeSet<(String, EdgeKind, String)> = [
        ("io.jvm", r, "java:java.util.ArrayList"),
        ("io.jvm", r, "java:java.util.UUID"),
        ("io.jvm", r, "java:java.io.File"),
        ("io.jvm/static-call", c, "java:java.lang.Math#abs"),
        ("io.jvm/ctor", c, "java:java.util.ArrayList"),
        ("io.jvm/ctor-new", c, "java:java.io.File"),
        ("io.jvm/hinted", c, "java:java.lang.String#trim"),
        ("io.jvm/hinted-inline", c, "java:java.lang.String#length"),
        ("io.jvm/unhinted", c, "java:?#close"),
        ("io.jvm/uuid", c, "java:java.util.UUID#randomUUID"),
        ("io.jvm/cmp", r, "java:java.util.Comparator"),
        ("io.web/log", c, "js:console.log"),
        ("io.web/method", c, "js:?.focus"),
        ("io.web/prop", r, "js:?.value"),
        ("io.web/fmt", c, "js:goog.string.format"),
        ("io.web", EdgeKind::Requires, "js:goog.string"),
    ]
    .into_iter()
    .map(|(a, k, b)| (a.to_owned(), k, b.to_owned()))
    .collect();
    let missing: Vec<_> = expected.difference(&to_platform).collect();
    let extra: Vec<_> = to_platform.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing {missing:#?}\nextra {extra:#?}"
    );
    assert!(!g.edges.iter().any(|e| matches!(
        e.to.to_string().as_str(),
        "ext:clj//clojure.core/." | "ext:clj//cljs.core/." | "ext:clj//clojure.core/new"
    )));
    let platforms = g
        .nodes
        .values()
        .filter(|n| n.kind == aldente_model::NodeKind::Platform)
        .count();
    let spots = g
        .blindspots
        .iter()
        .filter(|b| b.category == aldente_model::BlindSpotCategory::Interop)
        .count();
    assert_eq!(
        spots, platforms,
        "one interop blind spot per platform target"
    );
}

#[test]
fn interop_golden() {
    assert_graph_snapshot("clj-rules/interop", AggregationLevel::Symbol);
}
