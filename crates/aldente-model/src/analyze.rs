//! The `data` shape of `aldente analyze`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::kinds::{BlindSpotCategory, EdgeKind, NodeKind};
use crate::units::UnitInfo;

/// Counts for a snapshot. Whether it came from the cache is reported on
/// stderr, not here, so repeated runs print identical data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct AnalyzeSummary {
    pub units: usize,
    /// Source files analyzed.
    pub files: usize,
    pub nodes: BTreeMap<NodeKind, usize>,
    pub edges: BTreeMap<EdgeKind, usize>,
    pub blindspots: BTreeMap<BlindSpotCategory, usize>,
    pub diagnostics: Vec<Diagnostic>,
}

/// The server's `/snapshot`: the summary plus the units, for the GUI's first screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SnapshotOverview {
    pub summary: AnalyzeSummary,
    pub units: Vec<UnitInfo>,
}

/// What `aldente serve --json` prints once the server listens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ServeInfo {
    /// The UI address, token included: `http://127.0.0.1:<port>/?token=…`.
    pub url: String,
    pub port: u16,
}
