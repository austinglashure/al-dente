//! The `data` shape of `aldente analyze`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::kinds::{BlindSpotCategory, EdgeKind, NodeKind};

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
