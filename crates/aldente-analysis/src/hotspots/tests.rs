//! Hotspot goldens on clj-mini and mono-mini, and thresholds read from a
//! rules text rather than compiled in.

use aldente_rules::hotspots::HotspotRules;

use super::*;
use crate::measures::cache::MemoryStore;

/// One line per finding (evidence rounded) and the top ten risk rows.
fn render(r: &HotspotsReport) -> String {
    let kind = |k: FindingKind| {
        serde_json::to_value(k)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    let mut out = String::from("findings:\n");
    for f in &r.findings {
        let ev: Vec<String> = f
            .evidence
            .iter()
            .map(|(k, v)| format!("{k}={v:.3}"))
            .collect();
        out.push_str(&format!(
            "{:.3} {} {} [{}]\n    {}\n",
            f.score,
            kind(f.kind),
            f.subject,
            ev.join(" "),
            f.reason
        ));
    }
    out.push_str("risk (top 10):\n");
    for row in r.risk.iter().take(10) {
        out.push_str(&format!(
            "{:.4} importance={:.3} exposure={:.2} {}\n",
            row.risk, row.importance, row.exposure, row.id
        ));
    }
    out
}

fn report(fixture: &str, rules: &HotspotRules) -> HotspotsReport {
    let g = aldente_testkit::analyze_fixture(&aldente_testkit::fixture_dir(fixture)).graph;
    from_graph(&g, rules, "builtin", None, &MemoryStore::default()).0
}

#[test]
fn clj_mini_golden() {
    let r = report("clj-mini", &HotspotRules::builtin().unwrap());
    for f in &r.findings {
        assert!(
            !f.evidence.is_empty() || f.kind == FindingKind::DeadCode,
            "{f:?}"
        );
        assert!(
            f.reason.ends_with('.') && !f.reason.contains('\n'),
            "{}",
            f.reason
        );
    }
    insta::assert_snapshot!("clj_mini_hotspots", render(&r));
}

#[test]
fn mono_mini_golden() {
    let r = report("mono-mini", &HotspotRules::builtin().unwrap());
    insta::assert_snapshot!("mono_mini_hotspots", render(&r));
}

#[test]
fn thresholds_come_from_the_rules_text() {
    let builtin = include_str!("../../../../rules/hotspots.toml");
    let base = report("clj-mini", &HotspotRules::parse(builtin).unwrap());
    let count =
        |r: &HotspotsReport, k: FindingKind| r.findings.iter().filter(|f| f.kind == k).count();
    // Lower the god-object percentile: more god objects.
    let loose =
        HotspotRules::parse(&builtin.replace("percentile = 99.0", "percentile = 50.0")).unwrap();
    assert!(
        count(&report("clj-mini", &loose), FindingKind::GodObject)
            > count(&base, FindingKind::GodObject)
    );
    // Turn dead code off: no dead-code findings.
    let off = HotspotRules::parse(&builtin.replace("enabled = true", "enabled = false")).unwrap();
    assert!(count(&base, FindingKind::DeadCode) > 0);
    assert_eq!(count(&report("clj-mini", &off), FindingKind::DeadCode), 0);
}

#[test]
fn patterns_match_the_unit_dir_and_the_source_root_only() {
    let pats: Vec<String> = ["examples", "*bench*", "dev"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    // Any segment of the unit's directory.
    assert!(code_excluded("examples/server", None, &pats));
    assert!(code_excluded(
        "ring-bench",
        Some("ring-bench/src/b.clj"),
        &pats
    ));
    // The first directory inside the unit (its source root).
    assert!(code_excluded(
        "modules/x",
        Some("modules/x/dev/user.clj"),
        &pats
    ));
    assert!(code_excluded(".", Some("dev/user.clj"), &pats));
    // Not namespace directories below the source root.
    assert!(!code_excluded(
        "modules/reitit-dev",
        Some("modules/reitit-dev/src/reitit/dev/pretty.clj"),
        &pats
    ));
    assert!(!code_excluded(
        ".",
        Some("src/app/examples_test.clj"),
        &pats
    ));
    assert!(!code_excluded(".", None, &pats));
}

#[test]
fn excluded_code_leaves_findings_and_is_counted() {
    let builtin = include_str!("../../../../rules/hotspots.toml");
    let base = report("mono-mini", &HotspotRules::parse(builtin).unwrap());
    assert_eq!(base.excluded, 0);
    let billing = |r: &HotspotsReport| {
        r.findings
            .iter()
            .filter(|f| f.subject.contains("services/billing//"))
            .count()
    };
    assert!(billing(&base) > 0);
    let rules =
        HotspotRules::parse(&builtin.replace("segments = [", "segments = [\"billing\", ")).unwrap();
    let r = report("mono-mini", &rules);
    assert_eq!(billing(&r), 0);
    assert_eq!(r.excluded as usize, billing(&base));
    // Risk rows are a table of every symbol, not findings: kept.
    assert_eq!(r.risk.len(), base.risk.len());
}

#[test]
fn entry_points_are_not_risky_untested_by_default() {
    let builtin = include_str!("../../../../rules/hotspots.toml");
    let everyone = builtin.replace("risk-percentile = 95.0", "risk-percentile = 0.0");
    let main_flagged = |text: &str| {
        report("mono-mini", &HotspotRules::parse(text).unwrap())
            .findings
            .iter()
            .any(|f| f.kind == FindingKind::RiskyUntested && f.subject.ends_with("/-main"))
    };
    assert!(!main_flagged(&everyone));
    assert!(main_flagged(&everyone.replace(
        "skip-entry-points = true",
        "skip-entry-points = false"
    )));
}
