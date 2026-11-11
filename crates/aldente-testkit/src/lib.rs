//! Test support shared by the workspace's tests: fixture lookup and the
//! golden graph harness (`assert_graph_snapshot`).

pub mod golden;
pub mod render;

pub use golden::{assert_graph_snapshot, fixture_dir, run_pipeline};
pub use render::render_graph;
