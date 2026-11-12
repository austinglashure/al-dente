//! The clj-mini graph survives a write and load of its snapshot exactly.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use aldente_graph::snapshot::{Snapshot, load, path_for, write};
use aldente_testkit::{analyze_fixture, fixture_dir};

#[test]
fn clj_mini_round_trips_through_sqlite() {
    let analyzed = analyze_fixture(&fixture_dir("clj-mini"));
    let snap = Snapshot {
        key: "b3:0123".into(),
        repo: "fixtures/clj-mini".into(),
        rev: None,
        git: false,
        analyzer: "test".into(),
        units: analyzed.discovery.report.units.clone(),
        files: analyzed
            .discovery
            .analyzed
            .iter()
            .map(|f| (f.path.clone(), f.hash.clone()))
            .collect(),
        graph: analyzed.graph,
    };
    let dir = std::env::temp_dir().join(format!("aldente-testkit-snap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = path_for(&dir, &snap.key);
    write(&path, &snap).unwrap();
    let back = load(&path).unwrap().expect("snapshot loads");
    assert_eq!(back.graph.nodes, snap.graph.nodes);
    assert_eq!(back.graph.edges, snap.graph.edges);
    assert_eq!(back.graph.blindspots, snap.graph.blindspots);
    assert_eq!(back.graph.diagnostics, snap.graph.diagnostics);
    assert_eq!(back.graph.literals, snap.graph.literals);
    assert_eq!(back, snap);
    assert!(!snap.graph.literals.is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}
