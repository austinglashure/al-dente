//! The contract between language adapters and the graph build
//! (`02-architecture.md`, "The adapter contract"). Adapters turn one unit's
//! files into facts; they never write to the store or touch the network.

use std::path::Path;

use aldente_model::{BlindSpot, Diagnostic, Edge, Node, RepoPath, UnitInfo};
use thiserror::Error;

/// Facts an adapter found in one unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactBatch {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub literals: Vec<LiteralFact>,
    pub diagnostics: Vec<Diagnostic>,
    pub blindspots: Vec<BlindSpot>,
}

impl FactBatch {
    /// Appends another batch.
    ///
    /// Example: a batch with 2 nodes extended by one with 3 has 5.
    pub fn extend(&mut self, other: FactBatch) {
        self.nodes.extend(other.nodes);
        self.edges.extend(other.edges);
        self.literals.extend(other.literals);
        self.diagnostics.extend(other.diagnostics);
        self.blindspots.extend(other.blindspots);
    }
}

/// A literal harvested for later analyses (routes, state machines).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LiteralFact {
    pub file: RepoPath,
    pub line: u32,
    pub col: u32,
    /// `string`, `keyword`, `number` or `regex`.
    pub kind: &'static str,
    /// As written.
    pub text: String,
}

/// What an adapter may read. Files are the unit's own, already walked with
/// the safety rules (inside the root, no symlinks, size-capped).
#[derive(Debug, Clone, Copy)]
pub struct AnalyzeCtx<'a> {
    /// Canonical repository root.
    pub root: &'a Path,
    /// The unit's analyzed source files, sorted.
    pub files: &'a [RepoPath],
}

/// Why an adapter could not analyze a unit.
#[derive(Debug, Error)]
pub enum AdapterError {
    /// A required external tool is missing (exit code 5 in the CLI).
    #[error("{tool} is missing: {hint}")]
    ToolMissing { tool: String, hint: String },
    #[error("{0}")]
    Failed(String),
}

/// A language adapter.
pub trait Adapter: Send + Sync {
    /// Short identifier, e.g. `clj`.
    ///
    /// Example: the Clojure adapter returns `"clj"`.
    fn id(&self) -> &'static str;

    /// Whether this unit holds the adapter's language.
    ///
    /// Example: the Clojure adapter claims a unit whose `langs` include `clj`.
    fn claims(&self, unit: &UnitInfo) -> bool;

    /// Analyzes one unit.
    ///
    /// Example: the Clojure adapter on clj-mini's unit returns symbols such
    /// as `clj:.//shop.core/checkout` and their edges.
    fn analyze(&self, unit: &UnitInfo, ctx: AnalyzeCtx<'_>) -> Result<FactBatch, AdapterError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_extend() {
        let mut a = FactBatch::default();
        a.diagnostics.push(Diagnostic::new(
            None,
            aldente_model::DiagnosticCode::LimitReached,
            "x",
        ));
        let mut b = FactBatch::default();
        b.diagnostics.push(Diagnostic::new(
            None,
            aldente_model::DiagnosticCode::LimitReached,
            "y",
        ));
        a.extend(b);
        assert_eq!(a.diagnostics.len(), 2);
    }
}
