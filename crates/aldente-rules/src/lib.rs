//! Loader for the detection catalogs in `rules/`.
//!
//! The catalogs are compiled into the binary. Nothing is ever read from an
//! analyzed repository: its configuration is data, not rules.

pub mod clj;
pub mod keyword_dispatch;
pub mod trace;

/// A catalog that does not parse. Built-in catalogs are checked by tests, so
/// this only fires for a broken build.
#[derive(Debug, thiserror::Error)]
#[error("rule catalog {name}: {message}")]
pub struct RulesError {
    pub name: &'static str,
    pub message: String,
}
