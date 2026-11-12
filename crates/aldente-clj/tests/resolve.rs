//! Rules R1–R4 on the fixtures, through the real clj-kondo.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use aldente_acquire::tools::locate;
use aldente_acquire::units::{Discovery, discover};
use aldente_acquire::walk::{Limits, walk};
use aldente_clj::forms::index_files;
use aldente_clj::kondo::Kondo;
use aldente_clj::resolve::{Resolved, resolve};
use aldente_model::{
    BlindSpotCategory, Certainty, Edge, EdgeKind, Lang, Node, NodeKind, Reason, RepoPath,
    SymbolFlag, SymbolSubkind,
};
use aldente_rules::keyword_dispatch::builtin;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

/// Resolves every unit of a fixture.
///
/// Example: `run("clj-mini")` gives one `Resolved`.
fn run(name: &str) -> Vec<Resolved> {
    let root = fixture(name);
    let d: Discovery = discover(&root, false, walk(&root, &Limits::default()));
    let kondo_path = locate("clj-kondo", std::env::var_os("PATH").as_deref()).unwrap();
    let empty = std::env::temp_dir().join(format!("aldente-resolve-{}-{name}", std::process::id()));
    let kondo = Kondo::new(kondo_path, empty);
    d.report
        .units
        .iter()
        .map(|unit| {
            let files: Vec<RepoPath> = d
                .analyzed
                .iter()
                .filter(|f| f.unit == unit.dir)
                .map(|f| f.path.clone())
                .collect();
            let run = kondo.analyze(&root, &files).unwrap();
            let indexes = index_files(&root, &files);
            resolve(unit, &run, &indexes, &builtin().unwrap())
        })
        .collect()
}

/// Resolves a single-unit tree built in a temp dir from `(path, source)` pairs.
///
/// Example: `run_files("cond", &[("deps.edn", "{}"), ("src/a.clj", "(ns a)")])`.
fn run_files(name: &str, files: &[(&str, &str)]) -> Resolved {
    let root = std::env::temp_dir().join(format!(
        "aldente-resolve-tree-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    for (path, src) in files {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let root = root.canonicalize().unwrap();
    let d = discover(&root, false, walk(&root, &Limits::default()));
    let kondo_path = locate("clj-kondo", std::env::var_os("PATH").as_deref()).unwrap();
    let kondo = Kondo::new(
        kondo_path,
        root.join("..").join(format!("{name}-kondo-empty")),
    );
    let unit = &d.report.units[0];
    let files: Vec<RepoPath> = d.analyzed.iter().map(|f| f.path.clone()).collect();
    let run = kondo.analyze(&root, &files).unwrap();
    resolve(unit, &run, &index_files(&root, &files), &builtin().unwrap())
}

fn node<'a>(r: &'a Resolved, id: &str) -> &'a Node {
    r.batch
        .nodes
        .iter()
        .find(|n| n.id.to_string() == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn edges<'a>(r: &'a Resolved, from: &str, kind: EdgeKind) -> Vec<&'a Edge> {
    r.batch
        .edges
        .iter()
        .filter(|e| e.from.to_string() == from && e.kind == kind)
        .collect()
}

fn targets(r: &Resolved, from: &str, kind: EdgeKind) -> Vec<String> {
    let mut t: Vec<String> = edges(r, from, kind)
        .iter()
        .map(|e| e.to.to_string())
        .collect();
    t.sort();
    t.dedup();
    t
}

#[test]
fn r1_definitions_subkinds_and_flags() {
    let r = &run("clj-mini")[0];
    let sub = |id: &str| node(r, id).subkind;
    assert_eq!(sub("clj:.//shop.core/checkout"), Some(SymbolSubkind::Fn));
    assert_eq!(
        sub("clj:.//shop.util/with-timing"),
        Some(SymbolSubkind::Macro)
    );
    assert_eq!(
        sub("clj:.//shop.pricing/price"),
        Some(SymbolSubkind::Multimethod)
    );
    assert_eq!(
        sub("clj:.//shop.payment/Chargeable"),
        Some(SymbolSubkind::Protocol)
    );
    assert_eq!(
        sub("clj:.//shop.payment/charge"),
        Some(SymbolSubkind::ProtocolMethod)
    );
    assert_eq!(sub("clj:.//shop.payment/Card"), Some(SymbolSubkind::Record));
    assert_eq!(sub("clj:.//shop.payment/->Card"), Some(SymbolSubkind::Fn));
    assert_eq!(sub("clj:.//shop.inventory/stock"), Some(SymbolSubkind::Var));
    assert_eq!(
        sub("clj:.//shop.pricing-test/food-is-taxed"),
        Some(SymbolSubkind::Test)
    );
    assert!(
        node(r, "clj:.//shop.util/clamp")
            .flags
            .contains(&SymbolFlag::Private)
    );
    assert!(
        node(r, "clj:.//shop.inventory/stock")
            .flags
            .contains(&SymbolFlag::Stateful)
    );
    assert!(node(r, "clj:.//shop.core/checkout").flags.is_empty());
    let money = node(r, "clj:.//shop.model/format-money");
    assert_eq!(
        money.langs,
        [Lang::Clj, Lang::Cljs].into(),
        "cljc definitions merge"
    );
    assert_eq!(
        money.loc.as_ref().unwrap().file.as_str(),
        "src/shop/model.cljc"
    );
    assert_eq!(
        r.batch
            .nodes
            .iter()
            .filter(|n| n.id.to_string() == "clj:.//shop.model/format-money")
            .count(),
        1
    );
}

#[test]
fn r2_namespaces_and_requires() {
    let r = &run("clj-mini")[0];
    assert_eq!(
        node(r, "clj:.//shop.model").langs,
        [Lang::Clj, Lang::Cljs].into()
    );
    assert_eq!(
        targets(r, "clj:.//shop.ui.views", EdgeKind::Requires),
        [
            "clj:.//shop.model",
            "clj:.//shop.ui.events",
            "clj:.//shop.ui.subs",
            "ext:clj//re-frame.core",
            "ext:clj//react",
        ]
    );
    assert!(targets(r, "clj:.//shop.interop", EdgeKind::Requires).is_empty());
}

#[test]
fn r3_defmethod_bodies_belong_to_their_impls() {
    let r = &run("clj-mini")[0];
    let book = "clj:.//shop.pricing/price@:book";
    assert_eq!(node(r, book).kind, NodeKind::Impl);
    assert_eq!(
        targets(r, book, EdgeKind::Calls),
        ["clj:.//shop.util/apply-discount"]
    );
    let refs = edges(r, book, EdgeKind::References);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].to.to_string(), "clj:.//shop.pricing/half-off");
    assert_eq!(refs[0].sites[0].line, 13);
    assert_eq!(refs[0].sites[0].col, 24);
    // Every method exists, even one with no usages in its body.
    node(r, "clj:.//shop.pricing/price@:gift");
    // The multimethod itself gets nothing from the bodies.
    assert!(edges(r, "clj:.//shop.pricing/price", EdgeKind::Calls).is_empty());
}

#[test]
fn r3_protocol_impl_bodies_and_top_level_forms() {
    let r = &run("clj-mini")[0];
    assert_eq!(
        targets(r, "clj:.//shop.payment/charge@Voucher", EdgeKind::Calls),
        ["ext:clj//clojure.core/-", "ext:clj//clojure.core/max"]
    );
    node(r, "clj:.//shop.payment/charge@String");
    assert_eq!(
        targets(r, "clj:.//shop.core/<top>@L10", EdgeKind::Calls),
        ["ext:clj//clojure.core/println"]
    );
    // Phase 1: reify bodies belong to the enclosing function.
    assert!(
        targets(r, "clj:.//shop.payment/free-charger", EdgeKind::Calls)
            .contains(&"ext:clj//clojure.core/reify".to_owned())
    );
}

#[test]
fn r4_calls_references_and_macro_bodies() {
    let r = &run("clj-mini")[0];
    let call = edges(r, "clj:.//shop.core/checkout", EdgeKind::Calls)
        .into_iter()
        .find(|e| e.to.to_string() == "clj:.//shop.pricing/price")
        .unwrap();
    assert_eq!(
        (call.certainty, call.reason),
        (Some(Certainty::Certain), Some(Reason::Direct))
    );
    assert_eq!((call.sites[0].line, call.sites[0].col), (16, 17));
    let stock = edges(r, "clj:.//shop.inventory/available", EdgeKind::References)
        .into_iter()
        .find(|e| e.to.to_string() == "clj:.//shop.inventory/stock")
        .unwrap();
    assert_eq!(
        (stock.certainty, stock.reason),
        (Some(Certainty::Likely), Some(Reason::ValueRef))
    );

    let macro_refs = edges(r, "clj:.//shop.util/with-timing", EdgeKind::References);
    let mut names: Vec<String> = macro_refs.iter().map(|e| e.to.to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "clj:.//shop.interop/now-ms",
            "clj:.//shop.util/log-timing",
            "ext:clj//clojure.core/-",
            "ext:clj//clojure.core/let",
        ]
    );
    assert!(
        macro_refs.iter().all(
            |e| e.certainty == Some(Certainty::Possible) && e.reason == Some(Reason::MacroBody)
        )
    );
    let generated: Vec<_> = r
        .batch
        .blindspots
        .iter()
        .filter(|b| b.category == BlindSpotCategory::MacroGenerated)
        .collect();
    assert_eq!(generated.len(), 1);
    assert_eq!(
        generated[0].node.as_ref().unwrap().to_string(),
        "clj:.//shop.util/with-timing"
    );
    // Gensyms (`start#`) and special forms (`do`) in the template are not
    // vars and not blind spots.
    assert!(
        !r.batch
            .blindspots
            .iter()
            .any(|b| b.category == BlindSpotCategory::UnresolvedSymbol),
        "{:?}",
        r.batch.blindspots
    );
    // Macro expansion is invisible: no edge from checkout into the macro body.
    assert!(
        !targets(r, "clj:.//shop.core/checkout", EdgeKind::Calls)
            .contains(&"clj:.//shop.interop/now-ms".to_owned())
    );
}

#[test]
fn cljc_edges_keep_their_language() {
    let r = &run("clj-mini")[0];
    let money = edges(r, "clj:.//shop.model/format-money", EdgeKind::Calls);
    let format = money
        .iter()
        .find(|e| e.to.to_string() == "ext:clj//clojure.core/format")
        .unwrap();
    assert_eq!(format.lang, Some(Lang::Clj));
    let s = money
        .iter()
        .find(|e| e.to.to_string() == "ext:clj//cljs.core/str")
        .unwrap();
    assert_eq!(s.lang, Some(Lang::Cljs));
}

/// No clj-kondo var usage is dropped: each becomes an edge site, a blind
/// spot, or one of the documented suppressions.
#[test]
fn no_usage_is_dropped() {
    for fixture in ["clj-mini", "mono-mini"] {
        for r in run(fixture) {
            let s = &r.stats;
            let suppressed: usize = s.suppressed.values().sum();
            assert_eq!(
                s.usages,
                s.sites + s.positionless + suppressed + s.unresolved,
                "{fixture}: {s:?}"
            );
            let sites: usize = r
                .batch
                .edges
                .iter()
                .filter(|e| matches!(e.kind, EdgeKind::Calls | EdgeKind::References))
                // Only reasons that come straight from a var usage; derived
                // edges (param-flow, quoted-resolve, dynamic) are not usages.
                .filter(|e| {
                    matches!(
                        e.reason,
                        Some(
                            Reason::Direct | Reason::ValueRef | Reason::Interop | Reason::MacroBody
                        )
                    )
                })
                // Java and instance-method interop come from clj-kondo's class
                // and instance-invocation records, not var usages.
                .filter(|e| {
                    let to = e.to.to_string();
                    !(to.starts_with("java:") || to.starts_with("js:?."))
                })
                .map(|e| e.sites.len())
                .sum();
            assert_eq!(sites + s.merged_duplicates, s.sites, "{fixture}: {s:?}");
        }
    }
}

#[test]
fn every_edge_meets_the_model_invariants() {
    for r in run("clj-mini") {
        let ids: std::collections::BTreeSet<String> =
            r.batch.nodes.iter().map(|n| n.id.to_string()).collect();
        for e in &r.batch.edges {
            e.validate().unwrap();
            assert!(ids.contains(&e.from.to_string()), "dangling from: {e:?}");
            assert!(ids.contains(&e.to.to_string()), "dangling to: {e:?}");
        }
    }
}

#[test]
fn mono_mini_same_named_namespaces_are_distinct() {
    let all = run("mono-mini");
    let ids: Vec<String> = all
        .iter()
        .flat_map(|r| r.batch.nodes.iter())
        .filter(|n| n.kind == NodeKind::Namespace && n.name == "config")
        .map(|n| n.id.to_string())
        .collect();
    assert_eq!(
        ids,
        [
            "clj:services/billing//config",
            "clj:services/orders//config"
        ]
    );
}

/// Found on metosin/reitit: usages without a position (from `cond->>`) still
/// become edges, scoped by `from-var`, and are accounted for.
#[test]
fn positionless_usages_become_edges_without_sites() {
    let r = run_files(
        "cond",
        &[
            ("deps.edn", "{}"),
            (
                "src/a.clj",
                "(ns a) (defn f [xs] (cond->> xs true (map inc)))",
            ),
        ],
    );
    let threading = edges(&r, "clj:.//a/f", EdgeKind::Calls)
        .into_iter()
        .find(|e| e.to.to_string() == "ext:clj//clojure.core/->>")
        .expect("edge to ->>");
    assert!(threading.sites.is_empty());
    let s = &r.stats;
    assert!(s.positionless > 0);
    let suppressed: usize = s.suppressed.values().sum();
    assert_eq!(
        s.usages,
        s.sites + s.positionless + suppressed + s.unresolved,
        "{s:?}"
    );
}

/// A Java interface in `reify`/`deftype` is not a protocol: the impl
/// implements the platform class, and no edge points at a missing node.
#[test]
fn reify_of_a_java_interface_implements_the_platform_class() {
    let r = run_files(
        "reify-java",
        &[
            ("deps.edn", "{}"),
            (
                "src/a.clj",
                "(ns a)\n(defn mk [f]\n  (reify clojure.lang.IDeref\n    (deref [_] (f))))\n(deftype T [] java.lang.Runnable\n  (run [_] nil))\n",
            ),
        ],
    );
    let ids: std::collections::BTreeSet<String> =
        r.batch.nodes.iter().map(|n| n.id.to_string()).collect();
    for e in &r.batch.edges {
        assert!(
            ids.contains(&e.to.to_string()) || e.to.to_string().starts_with("ext:"),
            "edge to a missing node: {} -> {}",
            e.from,
            e.to
        );
    }
    let implements: Vec<String> = r
        .batch
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Implements)
        .map(|e| e.to.to_string())
        .collect();
    assert!(
        implements.contains(&"java:clojure.lang.IDeref".to_owned()),
        "{implements:?}"
    );
    assert!(
        implements.contains(&"java:java.lang.Runnable".to_owned()),
        "{implements:?}"
    );
}
