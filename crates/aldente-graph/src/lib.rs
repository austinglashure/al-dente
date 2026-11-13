//! The graph build: merges adapter facts across units, links across units,
//! rolls edges up to namespace and unit level, and runs the whole pipeline.

pub mod adjacency;
pub mod build;
pub mod cache;
pub mod pipeline;
pub mod query;
pub mod rollup;
pub mod snapshot;
pub mod summary;
pub mod tree;

pub use build::{Graph, build};
