//! Hardened git acquisition, the file walker, unit discovery and snapshot keys.
//!
//! So far: running external tools ([`process`]), finding and version-checking
//! them ([`tools`]) and the cache directory ([`cache`]). See
//! `docs/plan/02-architecture.md`.

pub mod cache;
pub mod edn;
pub mod key;
pub mod manifest;
pub mod process;
pub mod target;
pub mod tools;
pub mod units;
pub mod walk;
