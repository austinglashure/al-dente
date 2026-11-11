//! Exercises the golden harness end to end with the placeholder (empty)
//! pipeline. WP1.5 turns these into real golden graphs.

use aldente_model::AggregationLevel;
use aldente_testkit::{assert_graph_snapshot, fixture_dir};

#[test]
fn clj_mini_symbol_level() {
    assert_graph_snapshot("clj-mini", AggregationLevel::Symbol);
}

#[test]
fn clj_mini_namespace_level() {
    assert_graph_snapshot("clj-mini", AggregationLevel::Namespace);
}

#[test]
fn fixture_dir_finds_the_fixture() {
    assert!(fixture_dir("clj-mini").join("deps.edn").is_file());
}

#[test]
#[should_panic(expected = "no fixture")]
fn fixture_dir_rejects_unknown_names() {
    fixture_dir("no-such-fixture");
}
