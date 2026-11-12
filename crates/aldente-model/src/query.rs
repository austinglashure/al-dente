//! The `data` shapes of `aldente find`, `show` and `blindspots`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::blindspot::BlindSpot;
use crate::certainty::{Certainty, Reason};
use crate::diagnostic::Diagnostic;
use crate::graph::{Node, Site};
use crate::id::NodeId;
use crate::kinds::EdgeKind;

/// Nodes matching a search, sorted by ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct FindResult {
    pub matches: Vec<Node>,
    /// More matches exist than `--limit` allowed.
    pub truncated: bool,
}

/// One node with its direct callers and callees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ShowResult {
    pub node: Node,
    /// Strongest certainty first, then by ID.
    pub callers: Vec<Neighbor>,
    /// Strongest certainty first, then by ID.
    pub callees: Vec<Neighbor>,
}

/// The other end of an invocation edge (`calls`, `references`, `dispatches`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Neighbor {
    pub id: NodeId,
    pub name: String,
    pub kind: EdgeKind,
    pub certainty: Option<Certainty>,
    pub reason: Option<Reason>,
    pub sites: Vec<Site>,
}

/// Everything the analyzer could not see through, plus skipped files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct BlindspotReport {
    pub blindspots: Vec<BlindSpot>,
    pub diagnostics: Vec<Diagnostic>,
}
