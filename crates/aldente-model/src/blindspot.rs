//! Things the analyzer cannot see through, listed in the blind-spot report
//! (`03-graph-model.md`, "Blind spots").

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::graph::Site;
use crate::id::NodeId;
use crate::kinds::BlindSpotCategory;

/// One blind spot. Sorted by `(category, node, site, message)` in output.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct BlindSpot {
    pub category: BlindSpotCategory,
    /// The node it concerns (e.g. the macro, or the scope with the unresolved usage).
    pub node: Option<NodeId>,
    pub site: Option<Site>,
    pub message: String,
}
