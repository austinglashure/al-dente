//! The golden graph harness.

use std::path::{Path, PathBuf};

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

/// Runs the analysis pipeline on a fixture and returns the graph at `level`.
///
/// Placeholder until WP1.5: there is no pipeline yet, so this returns an empty
/// graph. WP1.5 replaces the body; callers do not change.
///
/// Example: `run_pipeline(&fixture_dir("clj-mini"), AggregationLevel::Symbol)`
/// returns `GraphPayload::default()` for now.
pub fn run_pipeline(fixture: &Path, level: AggregationLevel) -> GraphPayload {
    let _ = (fixture, level);
    GraphPayload::default()
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
