//! Coverage on clj-mini: every row of "Static coverage" in its EXPECTED.md.

use std::collections::{BTreeMap, BTreeSet};

use aldente_model::coverage::CoverageStatus::{self, Direct, Indirect, Untested, Weak};

use super::*;

fn mini() -> Graph {
    aldente_testkit::analyze_fixture(&aldente_testkit::fixture_dir("clj-mini")).graph
}

fn statuses(r: &CoverageReport) -> BTreeMap<String, CoverageStatus> {
    r.symbols
        .iter()
        .map(|s| {
            (
                s.id.to_string().trim_start_matches("clj:.//").to_owned(),
                s.status,
            )
        })
        .collect()
}

#[test]
fn clj_mini_statuses_match_the_hand_list() {
    let g = mini();
    let r = coverage(&g, DEFAULT_DEPTH, None);
    assert_eq!(r.tests, 2);
    let s = statuses(&r);
    let want = [
        ("shop.pricing/price", Direct),
        ("shop.pricing/price@:food", Direct),
        ("shop.pricing/price@:book", Direct),
        ("shop.pricing/price@:default", Weak),
        ("shop.util/apply-discount", Indirect),
        ("shop.pricing/half-off", Indirect),
        ("shop.util/clamp", Indirect),
        ("shop.pricing/price@:gift", Untested),
    ];
    for (id, st) in want {
        assert_eq!(s.get(id), Some(&st), "{id}");
    }
    let others: BTreeSet<&String> = s
        .iter()
        .filter(|(id, st)| **st != Untested && !want.iter().any(|(w, _)| w == id))
        .map(|(id, _)| id)
        .collect();
    assert!(
        others.is_empty(),
        "covered beyond the hand list: {others:?}"
    );
    assert!(
        !s.keys().any(|k| k.starts_with("shop.pricing-test/")),
        "tests get no status"
    );
    let price = r
        .symbols
        .iter()
        .find(|x| x.id.to_string().ends_with("shop.pricing/price"))
        .unwrap();
    assert_eq!(price.test_count, 2);
    assert_eq!(price.path_certainty, Some(Certainty::Certain));
    let clamp = r
        .symbols
        .iter()
        .find(|x| x.id.to_string().ends_with("shop.util/clamp"))
        .unwrap();
    assert_eq!(clamp.distance, Some(3));
    // Roll-ups: shop.pricing has direct and untested shares; shop.core none covered.
    let ns = |n: &str| {
        r.namespaces
            .iter()
            .find(|g| g.id.to_string().ends_with(n))
            .unwrap()
    };
    assert!(ns("//shop.pricing").weighted.direct > 0.0);
    assert_eq!(ns("//shop.core").weighted.untested, 1.0);
    assert_eq!(r.units.len(), 1);
    assert!(r.disagreements.is_empty() && r.imported.is_none());
}

#[test]
fn imported_lcov_overrides_and_lists_disagreements() {
    let g = mini();
    let text = std::fs::read_to_string(
        aldente_testkit::fixture_dir("clj-mini").join("coverage/lcov.info"),
    )
    .unwrap();
    let files: BTreeSet<String> = g
        .nodes
        .values()
        .filter_map(|n| n.loc.as_ref().map(|l| l.file.as_str().to_owned()))
        .collect();
    let (hits, info) = parse_report(&text, &files).unwrap();
    assert_eq!((info.files, info.matched), (2, 2));
    let r = coverage(&g, DEFAULT_DEPTH, Some((&hits, info)));
    let m = |id: &str| {
        r.symbols
            .iter()
            .find(|x| x.id.to_string().ends_with(id))
            .unwrap()
            .measured
    };
    assert_eq!(m("shop.pricing/price@:food"), Some(1.0));
    assert_eq!(m("shop.pricing/half-off"), Some(1.0));
    assert_eq!(m("shop.pricing/price@:book"), Some(0.0));
    assert_eq!(m("shop.inventory/restock!"), Some(1.0));
    assert_eq!(m("shop.core/checkout"), None, "no instrumented lines");
    let d: Vec<(String, &str)> = r
        .disagreements
        .iter()
        .map(|d| {
            (
                d.id.to_string().trim_start_matches("clj:.//").to_owned(),
                d.kind.as_str(),
            )
        })
        .collect();
    assert_eq!(
        d,
        vec![
            (
                "shop.inventory/restock!".to_owned(),
                "measured-covered-estimated-untested"
            ),
            (
                "shop.pricing/price@:book".to_owned(),
                "estimated-covered-measured-zero"
            ),
        ]
    );
    let ns = r
        .namespaces
        .iter()
        .find(|g| g.id.to_string().ends_with("//shop.pricing"))
        .unwrap();
    assert!(ns.measured.is_some());
}

#[test]
fn crater_checklists_show_coverage() {
    use crate::crater::{CraterIndex, CraterOptions, checklist, crater};
    use aldente_model::crater::Direction;
    let g = mini();
    let seed: NodeId = "clj:.//shop.pricing/half-off".parse().unwrap();
    let opts = CraterOptions {
        direction: Direction::Reverse,
        ..CraterOptions::default()
    };
    let result = crater(&g, &CraterIndex::new(&g), &[seed], &opts).unwrap();
    let mut rows = checklist(&g, &result);
    annotate_checklist(&mut rows, &coverage(&g, DEFAULT_DEPTH, None));
    let cov = |id: &str| {
        rows.iter()
            .find(|r| r.id.to_string().ends_with(id))
            .unwrap()
            .coverage
    };
    assert_eq!(cov("shop.pricing/half-off"), Some(Indirect));
    assert_eq!(cov("shop.pricing/price@:book"), Some(Direct));
    assert_eq!(
        cov("pricing-test/book-is-half-off"),
        None,
        "tests have no status"
    );
}

/// Helper functions in test namespaces are support that reach passes
/// through; tests, load-time code and method bodies there are where reach
/// starts: `fixtures/clj-rules/test-helpers/EXPECTED.md`.
#[test]
fn test_helpers_are_support_not_tests() {
    let g =
        aldente_testkit::analyze_fixture(&aldente_testkit::fixture_dir("clj-rules/test-helpers"))
            .graph;
    let r = coverage(&g, DEFAULT_DEPTH, None);
    assert_eq!(r.tests, 2, "only the deftests are tests");
    let s = statuses(&r);
    assert_eq!(s.get("app.core/run"), Some(&Direct));
    assert_eq!(s.get("app.core/build"), Some(&Indirect));
    assert_eq!(s.get("app.core/orphan"), Some(&Untested));
    assert_eq!(s.get("app.core/load-config"), Some(&Direct), "def value");
    assert_eq!(s.get("app.core/register!"), Some(&Direct), "top-level form");
    assert_eq!(
        s.get("app.core/send!"),
        Some(&Direct),
        "reify method in a test"
    );
    assert!(
        !s.keys().any(|k| k.starts_with("app.core-test/")),
        "test support gets no status: {s:?}"
    );
    let build = r
        .symbols
        .iter()
        .find(|x| x.id.to_string().ends_with("app.core/build"))
        .unwrap();
    assert_eq!(build.distance, Some(2));
    let names: Vec<String> = build.tests.iter().map(ToString::to_string).collect();
    assert_eq!(names, ["clj:.//app.core-test/run-works"]);
}
