//! The `data` shape of `aldente crater`: the impact subgraph of one or more
//! seeds. See "Impact crater" in `docs/plan/08-crater-and-traces.md`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::certainty::Certainty;
use crate::graph::GraphPayload;
use crate::graph::Location;
use crate::id::NodeId;
use crate::kinds::NodeKind;

/// Which way a crater grows from its seeds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// Everything the seeds can reach.
    Forward,
    /// Everything that can reach the seeds.
    Reverse,
    /// Both of the above.
    Both,
}

/// How one crater node was reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Reach {
    pub id: NodeId,
    /// Hops from the nearest seed on the path that gave `path_certainty`.
    pub distance: u32,
    /// The weakest certainty on the strongest path from a seed.
    pub path_certainty: Certainty,
    /// One predecessor on that path (in the direction of travel); null for seeds.
    pub via: Option<NodeId>,
    /// The side `via` belongs to: `forward` or `reverse`.
    pub direction: Direction,
}

/// Node counts of a crater.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CraterCounts {
    pub by_certainty: BTreeMap<Certainty, u64>,
    /// Keyed by unit ID; nodes outside any unit (externals, platforms, holes)
    /// are not counted here.
    pub by_unit: BTreeMap<String, u64>,
}

/// An impact crater.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CraterResult {
    /// The seeds as given, sorted.
    pub seeds: Vec<NodeId>,
    pub direction: Direction,
    /// 0 means unlimited.
    pub depth: u32,
    pub min_certainty: Certainty,
    /// The crater's nodes and the edges among them that the search follows.
    pub graph: GraphPayload,
    /// One entry per node of `graph`, sorted by ID.
    pub reach: Vec<Reach>,
    pub counts: CraterCounts,
    /// Externals, platform nodes and holes reached: the crater's blind spots.
    pub boundary: Vec<NodeId>,
    /// Hub nodes that were included but not expanded past.
    pub hubs: Vec<NodeId>,
    /// Nodes matching a stop pattern, included but not expanded past.
    pub stopped: Vec<NodeId>,
    /// How many more nodes a `possible` minimum certainty would add.
    pub hidden_below_min: u64,
    /// The depth limit or the node cap cut the crater short.
    pub truncated: bool,
    /// With `--diff`: how the seeds were found.
    pub diff: Option<DiffSeeds>,
}

/// Seeds taken from the changes between a revision and the working tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct DiffSeeds {
    /// The revision as given.
    pub base: String,
    /// The commit it names.
    pub base_commit: String,
    /// Symbols and impls whose source range touches a changed line, sorted.
    pub changed: Vec<NodeId>,
    /// Definitions present in the base revision and gone now, sorted.
    pub removed: Vec<RemovedSymbol>,
    /// Changed files (repository-relative) that hold no analyzed code.
    pub other_files: Vec<String>,
    /// Whether a snapshot of the base revision was in the cache (it gives
    /// removed definitions their former callers).
    pub base_snapshot: bool,
}

/// A definition that the working tree no longer has.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct RemovedSymbol {
    pub namespace: String,
    pub name: String,
    /// Where it was defined in the base revision.
    pub file: String,
    pub line: u32,
    /// Its callers in the base revision's snapshot (null without one).
    pub former_callers: Option<Vec<NodeId>>,
}

/// Whether tests exercise a symbol. Filled from Phase 4 on.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Coverage {
    Tested,
    Indirect,
    Untested,
}

/// One place to verify after a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ChecklistRow {
    pub id: NodeId,
    pub kind: NodeKind,
    /// `namespace/name`.
    pub name: String,
    pub loc: Option<Location>,
    pub distance: u32,
    pub path_certainty: Certainty,
    /// Null until coverage is computed (Phase 4).
    pub coverage: Option<Coverage>,
    pub entry_point: bool,
}

/// `aldente crater --checklist`: the crater's symbols and impls as a flat
/// list, strongest certainty first, then by distance and ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Checklist {
    pub seeds: Vec<NodeId>,
    pub direction: Direction,
    pub depth: u32,
    pub min_certainty: Certainty,
    pub rows: Vec<ChecklistRow>,
    pub hidden_below_min: u64,
    pub truncated: bool,
    pub diff: Option<DiffSeeds>,
}
