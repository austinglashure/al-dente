//! The Clojure and ClojureScript adapter: the clj-kondo runner ([`kondo`])
//! and the tree-sitter form index ([`forms`]).
//! See `docs/plan/04-clojure-analysis.md`.

pub mod adapter;
pub mod forms;
pub mod kondo;
pub mod resolve;
