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

#[test]
fn cljs_golden() {
    assert_graph_snapshot("clj-rules/cljs", AggregationLevel::Symbol);
}

#[test]
fn r12_cljc_cljs_npm_and_shadow_entry_points() {
    use aldente_model::{Lang, SymbolFlag};
    let g = analyze_fixture(&fixture_dir("clj-rules/cljs")).graph;
    let id = |s: &str| s.parse::<NodeId>().unwrap();
    let edges = |from: &str, kind: EdgeKind, to: &str| -> Vec<Option<Lang>> {
        g.edges
            .iter()
            .filter(|e| e.from == id(from) && e.kind == kind && e.to == id(to))
            .map(|e| e.lang)
            .collect()
    };
    let now = &g.nodes[&id("clj:.//app.shared/now")];
    assert_eq!(now.langs, [Lang::Clj, Lang::Cljs].into());
    assert_eq!(
        edges(
            "clj:.//app.shared/now",
            EdgeKind::Calls,
            "java:java.lang.System#currentTimeMillis"
        ),
        [Some(Lang::Clj)]
    );
    assert_eq!(
        edges("clj:.//app.shared/now", EdgeKind::Calls, "js:Date.now"),
        [Some(Lang::Cljs)]
    );
    assert_eq!(
        edges(
            "clj:.//app.shared/both",
            EdgeKind::Calls,
            "clj:.//app.shared/now"
        ),
        [None],
        "stored once for both languages"
    );
    let provides: Vec<_> = g
        .edges
        .iter()
        .filter(|e| {
            e.kind == EdgeKind::Provides
                && e.from == id("lib:npm:react")
                && e.to == id("ext:clj//react")
        })
        .collect();
    assert_eq!(provides.len(), 1);
    assert_eq!(provides[0].certainty, Some(Certainty::Certain));
    assert_eq!(
        edges("clj:.//app.main", EdgeKind::Requires, "ext:clj//react").len(),
        1
    );
    assert_eq!(
        edges(
            "clj:.//app.main",
            EdgeKind::Requires,
            "ext:clj//reagent.core"
        )
        .len(),
        1
    );
    assert!(
        !edges(
            "clj:.//app.main/init",
            EdgeKind::Calls,
            "clj:.//app.macros/when-dev"
        )
        .is_empty()
    );
    assert!(
        !edges(
            "clj:.//app.ui/render",
            EdgeKind::Calls,
            "clj:.//app.ui/widget"
        )
        .is_empty()
    );
    for entry in ["clj:.//app.main/init", "clj:.//app.ui/render"] {
        assert!(
            g.nodes[&id(entry)].flags.contains(&SymbolFlag::EntryPoint),
            "{entry}"
        );
    }
    assert!(
        !g.nodes[&id("clj:.//app.shared/both")]
            .flags
            .contains(&SymbolFlag::EntryPoint)
    );
}

#[test]
fn keyword_dispatch_golden() {
    assert_graph_snapshot("clj-rules/keyword-dispatch", AggregationLevel::Symbol);
}

/// R11 (WP2.8): every row of `fixtures/clj-rules/keyword-dispatch/EXPECTED.md`.
#[test]
fn r11_keyword_dispatch_matches_expected() {
    use Certainty::{Likely, Possible};
    use aldente_model::{BlindSpotCategory, Reason};
    let a = analyze_fixture(&fixture_dir("clj-rules/keyword-dispatch"));
    let g = &a.graph;
    let short = |id: &NodeId| id.to_string().trim_start_matches("clj:.//").to_owned();
    let actual: BTreeSet<(String, EdgeKind, String, Certainty)> = g
        .edges
        .iter()
        .filter(|e| e.reason == Some(Reason::KeywordDispatch))
        .map(|e| (short(&e.from), e.kind, short(&e.to), e.certainty.unwrap()))
        .collect();
    let c = |from: &str, to: &str| (from.to_owned(), EdgeKind::Calls, to.to_owned(), Likely);
    let mut expected: BTreeSet<_> = [
        c("app.views/order-list", "app.subs/reg-sub@:orders/all"),
        c(
            "app.views/submit-button",
            "app.events/reg-event-fx@:order/submit",
        ),
        c(
            "app.views/local-by-alias",
            "app.events/reg-event-db@:app.events/local",
        ),
        c(
            "app.views/local-by-name",
            "app.events/reg-event-db@:app.events/local",
        ),
        c(
            "app.events/reg-event-fx@:order/submit",
            "app.events/reg-cofx@:now",
        ),
        c(
            "app.events/reg-event-fx@:order/submit",
            "app.events/reg-event-db@:order/submitted",
        ),
        c(
            "app.events/reg-event-fx@:order/submit",
            "app.events/reg-event-db@:order/log",
        ),
        c(
            "app.events/reg-event-fx@:order/batch",
            "app.events/reg-event-db@:order/submitted",
        ),
        c(
            "app.events/reg-event-fx@:order/batch",
            "app.events/reg-event-db@:order/log",
        ),
        c(
            "app.subs/reg-sub@:orders/count",
            "app.subs/reg-sub@:orders/all",
        ),
        c("app.views/alpha-log", "app.events/reg-event-db@:order/log"),
        c(
            "app.events/reg-event-fx@:order/notify",
            "app.events/reg-fx@:app/notify",
        ),
    ]
    .into_iter()
    .collect();
    for m in ["init-key", "halt-key!"] {
        expected.insert((
            "app.system/config".to_owned(),
            EdgeKind::References,
            format!("integrant.core/{m}@:app/db"),
            Possible,
        ));
    }
    let missing: Vec<_> = expected.difference(&actual).collect();
    let extra: Vec<_> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing {missing:#?}\nextra {extra:#?}"
    );

    let registered = |id: &str, loc: &str| {
        let n = g
            .nodes
            .values()
            .find(|n| short(&n.id) == id)
            .unwrap_or_else(|| panic!("no node {id}"));
        let l = n.loc.as_ref().unwrap();
        assert_eq!(format!("{}:{}:{}", l.file, l.line, l.col), loc, "{id}");
    };
    registered("app.events/reg-cofx@:now", "src/app/events.cljs:4:1");
    registered(
        "app.events/reg-event-db@:app.events/local",
        "src/app/events.cljs:23:1",
    );
    registered("app.subs/reg-sub@:orders/count", "src/app/subs.cljs:7:1");
    assert!(
        !g.nodes
            .keys()
            .any(|n| short(n).starts_with("app.events/<top>")),
        "registrations replace <top> impls"
    );

    let notify = g
        .edges
        .iter()
        .find(|e| {
            short(&e.to) == "app.events/reg-fx@:app/notify"
                && e.reason == Some(Reason::KeywordDispatch)
        })
        .unwrap();
    let sites: Vec<(u32, u32)> = notify.sites.iter().map(|s| (s.line, s.col)).collect();
    assert_eq!(sites, [(40, 6), (41, 6)]);
    assert!(
        !g.blindspots.iter().any(|b| b.message.contains(":db")),
        "unregistered effect keys are data"
    );

    for (line, col) in [(27, 3), (30, 3), (45, 6)] {
        let hole = format!("hole:dynamic-resolve@src/app/events.cljs:{line}:{col}");
        assert!(
            g.edges.iter().any(|e| e.to.to_string() == hole
                && e.reason == Some(Reason::Dynamic)
                && e.certainty == Some(Certainty::Certain)),
            "{hole}"
        );
    }
    assert!(
        g.blindspots
            .iter()
            .any(|b| b.category == BlindSpotCategory::UnresolvedSymbol
                && b.message.contains(":no/handler"))
    );
}

/// R10 (WP2.9) on clj-mini: every external namespace has a `provides` edge,
/// and `lib:unknown` provides nothing.
#[test]
fn r10_every_external_namespace_is_provided_in_clj_mini() {
    let g = analyze_fixture(&fixture_dir("clj-mini")).graph;
    let provided: BTreeSet<String> = g
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Provides)
        .map(|e| e.to.to_string())
        .collect();
    let external_namespaces: BTreeSet<String> = g
        .nodes
        .keys()
        .filter_map(|id| match id {
            NodeId::External(e) => Some(format!("ext:clj//{}", e.ns())),
            _ => None,
        })
        .collect();
    let missing: Vec<_> = external_namespaces.difference(&provided).collect();
    assert!(missing.is_empty(), "no provides edge: {missing:?}");
    assert!(
        !g.edges.iter().any(|e| e.from.to_string() == "lib:unknown"),
        "lib:unknown provides nothing"
    );
    let row = |lib: &str, ns: &str| {
        g.edges
            .iter()
            .find(|e| {
                e.kind == EdgeKind::Provides
                    && e.from.to_string() == lib
                    && e.to.to_string() == format!("ext:clj//{ns}")
            })
            .and_then(|e| e.certainty)
    };
    assert_eq!(
        row("lib:maven:org.clojure/clojure", "clojure.string"),
        Some(Certainty::Certain)
    );
    assert_eq!(
        row("lib:maven:org.clojure/clojurescript", "cljs.core"),
        Some(Certainty::Certain)
    );
    assert_eq!(
        row("lib:maven:re-frame/re-frame", "re-frame.core"),
        Some(Certainty::Likely)
    );
    assert_eq!(row("lib:npm:react", "react"), Some(Certainty::Certain));
}

/// R10 across units: a namespace another unit defines is not a library's.
#[test]
fn r10_namespaces_of_other_units_get_no_library() {
    let g = analyze_fixture(&fixture_dir("mono-mini")).graph;
    for e in g.edges.iter().filter(|e| e.kind == EdgeKind::Provides) {
        assert!(
            matches!(e.to, NodeId::External(_)),
            "provides into a repository node: {} -> {}",
            e.from,
            e.to
        );
    }
    assert!(
        !g.nodes
            .keys()
            .any(|id| id.to_string() == "lib:maven:mono/common"),
        "a library left without edges is dropped"
    );
    assert!(
        !g.blindspots
            .iter()
            .any(|b| b.message.contains("common.util")),
        "common.util is defined by a unit: {:?}",
        g.blindspots
    );
}

#[test]
fn entry_points_golden() {
    assert_graph_snapshot("clj-rules/entry-points", AggregationLevel::Symbol);
}

/// R13 and R14 (WP2.9): `fixtures/clj-rules/entry-points/EXPECTED.md`.
#[test]
fn r13_r14_entry_points_and_tests() {
    use aldente_model::{SymbolFlag, SymbolSubkind};
    let g = analyze_fixture(&fixture_dir("clj-rules/entry-points")).graph;
    let node = |id: &str| {
        g.nodes
            .get(&format!("clj:.//{id}").parse::<NodeId>().unwrap())
            .unwrap_or_else(|| panic!("no node {id}"))
    };
    let flagged = |flag: SymbolFlag| -> BTreeSet<String> {
        g.nodes
            .values()
            .filter(|n| n.flags.contains(&flag))
            .map(|n| n.id.to_string().trim_start_matches("clj:.//").to_owned())
            .collect()
    };
    let set = |xs: &[&str]| xs.iter().map(|s| (*s).to_owned()).collect::<BTreeSet<_>>();
    assert_eq!(
        flagged(SymbolFlag::EntryPoint),
        set(&[
            "app.core/-main",
            "app.core/app",
            "app.core/exported",
            "app.core/other-handler",
            "app.core/start",
            "app.midje-facts/<top>@L5",
            "app.thing-test/helper-is-two",
            "app.thing-test/newest-is-stable",
        ])
    );
    assert_eq!(
        flagged(SymbolFlag::Test),
        set(&[
            "app.helpers",
            "app.midje-facts",
            "app.midje-facts/<top>@L5",
            "app.thing-test",
            "app.thing-test/helper-is-two",
            "app.thing-test/newest-is-stable",
        ])
    );
    assert_eq!(
        node("app.thing-test/newest-is-stable").subkind,
        Some(SymbolSubkind::Test)
    );
    assert_eq!(
        node("app.thing-test/fixture-helper").subkind,
        Some(SymbolSubkind::Fn)
    );
    let unit = g.nodes.get(&"unit:.".parse::<NodeId>().unwrap()).unwrap();
    assert!(
        unit.flags.contains(&SymbolFlag::Service),
        "entry points seed the service flag"
    );
}

/// A unit whose only entry points are tests is not a service.
#[test]
fn tests_alone_do_not_make_a_service() {
    use aldente_model::SymbolFlag;
    let g = analyze_fixture(&fixture_dir("clj-rules/multimethods")).graph;
    let unit = g.nodes.get(&"unit:.".parse::<NodeId>().unwrap()).unwrap();
    assert!(!unit.flags.contains(&SymbolFlag::Service));
}

/// M2 (WP2.10): `fixtures/clj-mini/EXPECTED.md` is fully satisfied. Every
/// row of its edge tables (transcribed below, `contains` aside) exists, and
/// every `certain` edge is a row or one of the unlisted forms its scope rules
/// 2, 3 and 4a name (`let`, `fn`, `fn*`, `deref`, `var`).
#[test]
fn m2_clj_mini_expected_is_fully_satisfied() {
    let g = analyze_fixture(&fixture_dir("clj-mini")).graph;
    let short = |id: &NodeId| {
        id.to_string()
            .replace("clj:.//", "")
            .replace("ext:clj//", "ext:")
    };
    let token = |k: EdgeKind| {
        serde_json::to_value(k)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    let actual: BTreeSet<(String, String, String)> = g
        .edges
        .iter()
        .filter(|e| e.kind != EdgeKind::Contains)
        .map(|e| (short(&e.from), token(e.kind), short(&e.to)))
        .collect();
    let expected: BTreeSet<(String, String, String)> = [
        (
            "lib:maven:org.clojure/clojure",
            "provides",
            "ext:clojure.core",
        ),
        (
            "lib:maven:org.clojure/clojure",
            "provides",
            "ext:clojure.string",
        ),
        (
            "lib:maven:org.clojure/clojure",
            "provides",
            "ext:clojure.test",
        ),
        (
            "lib:maven:org.clojure/clojurescript",
            "provides",
            "ext:cljs.core",
        ),
        (
            "lib:maven:re-frame/re-frame",
            "provides",
            "ext:re-frame.core",
        ),
        ("lib:npm:react", "provides", "ext:react"),
        ("shop.audit/record!", "calls", "ext:clojure.core/println"),
        ("shop.core", "requires", "shop.inventory"),
        ("shop.core", "requires", "shop.model"),
        ("shop.core", "requires", "shop.payment"),
        ("shop.core", "requires", "shop.plugins"),
        ("shop.core", "requires", "shop.pricing"),
        ("shop.core", "requires", "shop.util"),
        ("shop.core/-main", "calls", "ext:clojure.core/println"),
        ("shop.core/-main", "calls", "shop.core/checkout"),
        ("shop.core/-main", "calls", "shop.payment/->Card"),
        ("shop.core/-main", "calls", "shop.payment/charge"),
        ("shop.core/-main", "calls", "shop.plugins/audit-hook"),
        ("shop.core/-main", "dispatches", "shop.payment/charge@Card"),
        ("shop.core/<top>@L10", "calls", "ext:clojure.core/println"),
        ("shop.core/checkout", "calls", "shop.inventory/reserve!"),
        ("shop.core/checkout", "calls", "shop.model/order-summary"),
        ("shop.core/checkout", "calls", "shop.payment/charge"),
        ("shop.core/checkout", "calls", "shop.pricing/price"),
        ("shop.core/checkout", "calls", "shop.util/with-timing"),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.payment/charge@Card",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.payment/charge@String",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.payment/charge@Voucher",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.payment/charge@reify:L25",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.pricing/price@:book",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.pricing/price@:default",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.pricing/price@:food",
        ),
        (
            "shop.core/checkout",
            "dispatches",
            "shop.pricing/price@:gift",
        ),
        ("shop.interop", "references", "java:java.util.UUID"),
        ("shop.interop/new-id", "calls", "ext:clojure.core/str"),
        (
            "shop.interop/new-id",
            "calls",
            "java:java.util.UUID#randomUUID",
        ),
        (
            "shop.interop/now-ms",
            "calls",
            "java:java.lang.System#currentTimeMillis",
        ),
        (
            "shop.interop/shout",
            "calls",
            "java:java.lang.String#toUpperCase",
        ),
        ("shop.inventory/available", "calls", "ext:clojure.core/get"),
        (
            "shop.inventory/available",
            "references",
            "shop.inventory/stock",
        ),
        ("shop.inventory/reserve!", "calls", "ext:clojure.core/swap!"),
        (
            "shop.inventory/reserve!",
            "references",
            "ext:clojure.core/-",
        ),
        (
            "shop.inventory/reserve!",
            "references",
            "ext:clojure.core/update",
        ),
        (
            "shop.inventory/reserve!",
            "references",
            "shop.inventory/stock",
        ),
        ("shop.inventory/restock!", "calls", "ext:clojure.core/fnil"),
        ("shop.inventory/restock!", "calls", "ext:clojure.core/swap!"),
        (
            "shop.inventory/restock!",
            "references",
            "ext:clojure.core/+",
        ),
        (
            "shop.inventory/restock!",
            "references",
            "ext:clojure.core/update",
        ),
        (
            "shop.inventory/restock!",
            "references",
            "shop.inventory/stock",
        ),
        ("shop.inventory/stock", "calls", "ext:clojure.core/atom"),
        ("shop.model", "requires", "ext:clojure.string"),
        ("shop.model/format-money", "calls", "ext:cljs.core/str"),
        (
            "shop.model/format-money",
            "calls",
            "ext:clojure.core/double",
        ),
        (
            "shop.model/format-money",
            "calls",
            "ext:clojure.core/format",
        ),
        (
            "shop.model/order-summary",
            "calls",
            "ext:clojure.string/join",
        ),
        (
            "shop.model/order-summary",
            "calls",
            "shop.model/format-money",
        ),
        (
            "shop.payment/charge@Card",
            "implements",
            "shop.payment/Chargeable",
        ),
        (
            "shop.payment/charge@String",
            "implements",
            "shop.payment/Chargeable",
        ),
        ("shop.payment/charge@Voucher", "calls", "ext:clojure.core/-"),
        (
            "shop.payment/charge@Voucher",
            "calls",
            "ext:clojure.core/max",
        ),
        (
            "shop.payment/charge@Voucher",
            "implements",
            "shop.payment/Chargeable",
        ),
        (
            "shop.payment/charge@reify:L25",
            "implements",
            "shop.payment/Chargeable",
        ),
        (
            "shop.payment/free-charger",
            "calls",
            "ext:clojure.core/reify",
        ),
        (
            "shop.plugins/audit-hook",
            "calls",
            "ext:clojure.core/requiring-resolve",
        ),
        ("shop.plugins/audit-hook", "calls", "shop.audit/record!"),
        (
            "shop.plugins/load-plugin",
            "calls",
            "ext:clojure.core/requiring-resolve",
        ),
        (
            "shop.plugins/load-plugin",
            "calls",
            "ext:clojure.core/symbol",
        ),
        (
            "shop.plugins/load-plugin",
            "calls",
            "hole:dynamic-resolve@src/shop/plugins.clj:8:11",
        ),
        ("shop.plugins/run-script", "calls", "ext:clojure.core/eval"),
        (
            "shop.plugins/run-script",
            "calls",
            "ext:clojure.core/read-string",
        ),
        (
            "shop.plugins/run-script",
            "calls",
            "hole:dynamic-eval@src/shop/plugins.clj:12:3",
        ),
        ("shop.pricing", "requires", "shop.util"),
        ("shop.pricing-test", "requires", "ext:clojure.test"),
        ("shop.pricing-test", "requires", "shop.pricing"),
        (
            "shop.pricing-test/book-is-half-off",
            "calls",
            "ext:clojure.core/=",
        ),
        (
            "shop.pricing-test/book-is-half-off",
            "calls",
            "ext:clojure.test/is",
        ),
        (
            "shop.pricing-test/book-is-half-off",
            "calls",
            "shop.pricing/price",
        ),
        (
            "shop.pricing-test/book-is-half-off",
            "dispatches",
            "shop.pricing/price@:book",
        ),
        (
            "shop.pricing-test/book-is-half-off",
            "dispatches",
            "shop.pricing/price@:default",
        ),
        (
            "shop.pricing-test/food-is-taxed",
            "calls",
            "ext:clojure.core/<",
        ),
        (
            "shop.pricing-test/food-is-taxed",
            "calls",
            "ext:clojure.test/is",
        ),
        (
            "shop.pricing-test/food-is-taxed",
            "calls",
            "shop.pricing/price",
        ),
        (
            "shop.pricing-test/food-is-taxed",
            "dispatches",
            "shop.pricing/price@:default",
        ),
        (
            "shop.pricing-test/food-is-taxed",
            "dispatches",
            "shop.pricing/price@:food",
        ),
        ("shop.pricing/half-off", "calls", "ext:clojure.core//"),
        (
            "shop.pricing/price@:book",
            "calls",
            "shop.util/apply-discount",
        ),
        (
            "shop.pricing/price@:book",
            "implements",
            "shop.pricing/price",
        ),
        (
            "shop.pricing/price@:book",
            "references",
            "shop.pricing/half-off",
        ),
        (
            "shop.pricing/price@:default",
            "implements",
            "shop.pricing/price",
        ),
        ("shop.pricing/price@:food", "calls", "ext:clojure.core/*"),
        (
            "shop.pricing/price@:food",
            "implements",
            "shop.pricing/price",
        ),
        (
            "shop.pricing/price@:gift",
            "implements",
            "shop.pricing/price",
        ),
        ("shop.ui.events", "requires", "ext:re-frame.core"),
        (
            "shop.ui.events/reg-event-db@:cart/add",
            "calls",
            "ext:cljs.core/fnil",
        ),
        (
            "shop.ui.events/reg-event-db@:cart/add",
            "calls",
            "ext:cljs.core/update-in",
        ),
        (
            "shop.ui.events/reg-event-db@:cart/add",
            "calls",
            "ext:re-frame.core/reg-event-db",
        ),
        (
            "shop.ui.events/reg-event-db@:cart/add",
            "references",
            "ext:cljs.core/inc",
        ),
        (
            "shop.ui.events/reg-event-db@:order/submitted",
            "calls",
            "ext:cljs.core/assoc",
        ),
        (
            "shop.ui.events/reg-event-db@:order/submitted",
            "calls",
            "ext:re-frame.core/reg-event-db",
        ),
        (
            "shop.ui.events/reg-event-fx@:order/submit",
            "calls",
            "ext:cljs.core/assoc",
        ),
        (
            "shop.ui.events/reg-event-fx@:order/submit",
            "calls",
            "ext:re-frame.core/reg-event-fx",
        ),
        (
            "shop.ui.events/reg-event-fx@:order/submit",
            "calls",
            "shop.ui.events/reg-event-db@:order/submitted",
        ),
        ("shop.ui.subs", "requires", "ext:re-frame.core"),
        (
            "shop.ui.subs/reg-sub@:cart/items",
            "calls",
            "ext:re-frame.core/reg-sub",
        ),
        ("shop.ui.views", "requires", "ext:re-frame.core"),
        ("shop.ui.views", "requires", "ext:react"),
        ("shop.ui.views", "requires", "shop.model"),
        ("shop.ui.views", "requires", "shop.ui.events"),
        ("shop.ui.views", "requires", "shop.ui.subs"),
        ("shop.ui.views/cart-view", "calls", "ext:cljs.core/count"),
        (
            "shop.ui.views/cart-view",
            "calls",
            "ext:re-frame.core/subscribe",
        ),
        (
            "shop.ui.views/cart-view",
            "calls",
            "ext:react/createElement",
        ),
        (
            "shop.ui.views/cart-view",
            "calls",
            "shop.ui.subs/reg-sub@:cart/items",
        ),
        ("shop.ui.views/init", "calls", "ext:re-frame.core/dispatch"),
        ("shop.ui.views/init", "calls", "js:console.log"),
        ("shop.ui.views/init", "calls", "shop.model/format-money"),
        (
            "shop.ui.views/init",
            "calls",
            "shop.ui.events/reg-event-db@:cart/add",
        ),
        (
            "shop.ui.views/submit-button",
            "calls",
            "ext:re-frame.core/dispatch",
        ),
        (
            "shop.ui.views/submit-button",
            "calls",
            "ext:react/createElement",
        ),
        (
            "shop.ui.views/submit-button",
            "calls",
            "shop.ui.events/reg-event-fx@:order/submit",
        ),
        ("shop.util", "requires", "shop.interop"),
        ("shop.util/apply-discount", "calls", "shop.pricing/half-off"),
        ("shop.util/apply-discount", "calls", "shop.util/clamp"),
        ("shop.util/clamp", "calls", "ext:clojure.core/max"),
        ("shop.util/clamp", "calls", "ext:clojure.core/min"),
        ("shop.util/log-timing", "calls", "ext:clojure.core/println"),
        ("shop.util/with-timing", "references", "ext:clojure.core/-"),
        (
            "shop.util/with-timing",
            "references",
            "ext:clojure.core/let",
        ),
        ("shop.util/with-timing", "references", "shop.interop/now-ms"),
        (
            "shop.util/with-timing",
            "references",
            "shop.util/log-timing",
        ),
    ]
    .iter()
    .map(|(a, k, b)| ((*a).to_owned(), (*k).to_owned(), (*b).to_owned()))
    .collect();
    let missing: Vec<_> = expected.difference(&actual).collect();
    assert!(missing.is_empty(), "missing rows: {missing:#?}");
    let unlisted_forms = ["let", "fn", "fn*", "deref", "var"];
    let extra_certain: Vec<_> = g
        .edges
        .iter()
        .filter(|e| e.kind != EdgeKind::Contains && e.certainty == Some(Certainty::Certain))
        .map(|e| (short(&e.from), token(e.kind), short(&e.to)))
        .filter(|row| !expected.contains(row))
        .filter(|(_, _, to)| {
            let form = to
                .strip_prefix("ext:clojure.core/")
                .or_else(|| to.strip_prefix("ext:cljs.core/"));
            !form.is_some_and(|f| unlisted_forms.contains(&f))
        })
        .collect();
    assert!(
        extra_certain.is_empty(),
        "certain edges not in EXPECTED.md: {extra_certain:#?}"
    );
}

/// Blind spots stay visible when zoomed out: holes keep their own node at
/// namespace level, with the rolled-up `dynamic` call from the namespace.
#[test]
fn holes_are_visible_at_namespace_level() {
    let g = analyze_fixture(&fixture_dir("clj-mini")).graph;
    let p = g.payload(AggregationLevel::Namespace);
    for hole in [
        "hole:dynamic-eval@src/shop/plugins.clj:12:3",
        "hole:dynamic-resolve@src/shop/plugins.clj:8:11",
    ] {
        assert!(
            p.nodes.iter().any(|n| n.id.to_string() == hole),
            "no node {hole}"
        );
        assert!(
            p.edges
                .iter()
                .any(|e| e.from.to_string() == "clj:.//shop.plugins"
                    && e.to.to_string() == hole
                    && e.kind == EdgeKind::Calls),
            "no edge to {hole}"
        );
    }
}

/// The GUI's global namespace map: `repo_only` keeps repository namespaces
/// (from every unit) and holes, and drops externals, platforms and libraries
/// and the edges into them, so the map fits the payload cap.
#[test]
fn global_namespace_map_keeps_only_repository_code() {
    use aldente_graph::query::{Filter, fit};
    use aldente_model::NodeKind;
    let g = analyze_fixture(&fixture_dir("mono-mini")).graph;
    let filter = Filter {
        repo_only: true,
        ..Filter::default()
    };
    let (p, coarser) = fit(
        &g,
        &aldente_graph::cache::Cache::default(),
        AggregationLevel::Namespace,
        &filter,
    );
    assert!(!coarser);
    assert!(
        p.nodes
            .iter()
            .all(|n| matches!(n.kind, NodeKind::Namespace | NodeKind::Hole))
    );
    let units: BTreeSet<String> = p
        .nodes
        .iter()
        .filter_map(|n| n.unit.as_ref().map(|u| u.to_string()))
        .collect();
    assert!(units.len() > 1, "namespaces from several units: {units:?}");
    let ids: BTreeSet<&NodeId> = p.nodes.iter().map(|n| &n.id).collect();
    assert!(
        p.edges
            .iter()
            .all(|e| ids.contains(&e.from) && ids.contains(&e.to))
    );
    assert!(!p.edges.is_empty());
}

/// Node caps for the global map: the top N namespaces by calls in and out
/// (rolled-up counts), and only the edges among them.
#[test]
fn top_namespaces_by_call_counts() {
    use aldente_graph::query::{Filter, filter, top_by_calls};
    let g = analyze_fixture(&fixture_dir("clj-mini")).graph;
    let all = filter(
        &g,
        g.payload(AggregationLevel::Namespace),
        &Filter {
            repo_only: true,
            ..Filter::default()
        },
    );
    let weight = |id: &NodeId| -> u64 {
        all.edges
            .iter()
            .filter(|e| &e.from == id || &e.to == id)
            .filter(|e| e.kind != EdgeKind::Requires)
            .map(|e| e.count.unwrap_or(1))
            .sum()
    };
    let mut ranked: Vec<(u64, String)> = all
        .nodes
        .iter()
        .map(|n| (weight(&n.id), n.id.to_string()))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let expected: BTreeSet<String> = ranked.iter().take(3).map(|(_, id)| id.clone()).collect();

    let top = top_by_calls(all.clone(), 3);
    let ids: BTreeSet<String> = top.nodes.iter().map(|n| n.id.to_string()).collect();
    assert_eq!(ids, expected);
    assert!(top.truncated);
    assert!(
        expected.contains("clj:.//shop.core"),
        "the hub is kept: {expected:?}"
    );
    assert!(
        top.edges
            .iter()
            .all(|e| ids.contains(&e.from.to_string()) && ids.contains(&e.to.to_string()))
    );
    let same = top_by_calls(all.clone(), 10_000);
    assert_eq!(same.nodes.len(), all.nodes.len());
    assert!(!same.truncated);
}
