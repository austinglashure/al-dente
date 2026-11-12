//! The graph build: merges adapter facts across units, links across units,
//! rolls edges up to namespace and unit level, and runs the whole pipeline.

pub mod build;
pub mod pipeline;
pub mod rollup;

pub use build::{Graph, build};
