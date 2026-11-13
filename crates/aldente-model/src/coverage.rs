//! The `data` shape of `aldente coverage`: estimated (static) test
//! coverage per symbol, rolled up per namespace and unit, optionally merged
//! with an imported measured report. See "Coverage" in
//! `docs/plan/09-network-and-coverage.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::certainty::Certainty;
use crate::id::NodeId;

/// Estimated coverage of a symbol.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageStatus {
    /// Invoked by a test (distance 1) at `likely` or better.
    Direct,
    /// Reachable from a test within the depth limit at `likely` or better.
    Indirect,
    /// Reachable only through `possible` edges or beyond the depth limit.
    Weak,
    /// Not reachable from any test.
    Untested,
}

/// One symbol's coverage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SymbolCoverage {
    pub id: NodeId,
    pub status: CoverageStatus,
    /// Fewest hops from a test (any certainty), when reachable.
    pub distance: Option<u32>,
    /// Certainty of the best path within the depth limit, when there is one.
    pub path_certainty: Option<Certainty>,
    /// Covering tests, at most ten, sorted.
    pub tests: Vec<NodeId>,
    /// How many tests cover it in all.
    pub test_count: u32,
    /// Measured line coverage (0..1) from an imported report, when the
    /// report has instrumented lines inside the symbol.
    pub measured: Option<f64>,
}

/// Shares of a namespace's or unit's symbols per status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct StatusShares {
    pub direct: f64,
    pub indirect: f64,
    pub weak: f64,
    pub untested: f64,
}

/// Coverage of a namespace or unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct GroupCoverage {
    pub id: NodeId,
    pub symbols: u32,
    /// Shares weighted by each symbol's size in lines.
    pub weighted: StatusShares,
    /// Covered over instrumented lines of its symbols, when imported.
    pub measured: Option<f64>,
}

/// An estimate the measured report contradicts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Disagreement {
    pub id: NodeId,
    pub estimated: CoverageStatus,
    pub measured: f64,
    /// `estimated-covered-measured-zero` (an analysis mistake or a dead
    /// test) or `measured-covered-estimated-untested` (a missing edge).
    pub kind: String,
}

/// What an imported report contained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ImportInfo {
    /// `lcov` or `codecov-json`.
    pub format: String,
    pub files: u32,
    /// Report files matched to repository files.
    pub matched: u32,
    /// Report paths that matched no repository file, sorted.
    pub unmatched: Vec<String>,
}

/// `aldente coverage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CoverageReport {
    /// Always true: statuses are a static estimate ("some test exercises a
    /// path that could reach this").
    pub estimated: bool,
    pub depth: u32,
    pub tests: u32,
    /// Sorted by ID.
    pub symbols: Vec<SymbolCoverage>,
    pub namespaces: Vec<GroupCoverage>,
    pub units: Vec<GroupCoverage>,
    pub imported: Option<ImportInfo>,
    pub disagreements: Vec<Disagreement>,
}
