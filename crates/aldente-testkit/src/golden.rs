//! The golden graph harness.

use std::path::{Path, PathBuf};

use aldente_acquire::tools::locate;
use aldente_clj::adapter::CljAdapter;
use aldente_clj::kondo::Kondo;
use aldente_graph::pipeline::{Analyzed, analyze};
use aldente_model::{AggregationLevel, GraphPayload};

use crate::render::render_graph;

/// Absolute path of `fixtures/<name>` at the workspace root. Panics if it does
/// not exist, so a typo fails the test instead of snapshotting nothing.
///
/// Example: `fixture_dir("clj-mini")` ends in `fixtures/clj-mini`.
pub fn fixture_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    assert!(dir.is_dir(), "no fixture {name:?} at {}", dir.display());
    dir
}

/// Runs the analysis pipeline on a fixture (as a repository without git)
/// and returns the graph at `level`. Needs clj-kondo on `PATH`.
///
/// Example: `run_pipeline(&fixture_dir("clj-mini"), AggregationLevel::Symbol)`
/// contains `clj:.//shop.core/checkout`.
pub fn run_pipeline(fixture: &Path, level: AggregationLevel) -> GraphPayload {
    analyze_fixture(fixture).graph.payload(level)
}

/// Runs the full pipeline on a fixture directory.
///
/// Example: `analyze_fixture(&fixture_dir("mono-mini")).discovery.report.units.len()` is 3.
pub fn analyze_fixture(fixture: &Path) -> Analyzed {
    let root = fixture
        .canonicalize()
        .unwrap_or_else(|_| fixture.to_path_buf());
    let kondo = locate("clj-kondo", std::env::var_os("PATH").as_deref())
        .unwrap_or_else(|| PathBuf::from("clj-kondo"));
    let empty = std::env::temp_dir().join(format!("aldente-testkit-kondo-{}", std::process::id()));
    let adapter = CljAdapter::new(Kondo::new(kondo, empty));
    match analyze(&root, false, &[&adapter]) {
        Ok(analyzed) => analyzed,
        Err(e) => panic!("pipeline failed on {}: {e}", fixture.display()),
    }
}

/// Runs the pipeline on `fixtures/<fixture>` and compares the rendered graph
/// with the golden file `fixtures/<fixture>/golden/<fixture>__<level>.snap`.
///
/// Example: `assert_graph_snapshot("clj-mini", AggregationLevel::Namespace)`
/// checks `fixtures/clj-mini/golden/clj-mini__namespace.snap`.
#[track_caller]
pub fn assert_graph_snapshot(fixture: &str, level: AggregationLevel) {
    let dir = fixture_dir(fixture);
    let rendered = render_graph(&run_pipeline(&dir, level));
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(dir.join("golden"));
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.bind(|| {
        insta::assert_snapshot!(format!("{fixture}__{}", level_name(level)), rendered);
    });
}

/// The file-name form of an aggregation level.
///
/// Example: `level_name(AggregationLevel::Namespace)` is `"namespace"`.
fn level_name(level: AggregationLevel) -> &'static str {
    match level {
        AggregationLevel::Unit => "unit",
        AggregationLevel::Namespace => "namespace",
        AggregationLevel::Symbol => "symbol",
    }
}
