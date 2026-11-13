//! Graph measures: the projection they run on and their results.
//! See "Network analysis and coverage" in `docs/plan/09-network-and-coverage.md`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::certainty::Certainty;
use crate::graph::AggregationLevel;
use crate::id::NodeId;

/// Which edges a projection keeps.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectionKind {
    /// `calls`, `references`, `dispatches`, `registers`.
    Invocation,
    /// `requires` between namespaces, `depends` between units.
    Structure,
}

/// Edge weight per certainty level.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Weights {
    pub certain: f64,
    pub likely: f64,
    pub possible: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            certain: Certainty::Certain.default_weight(),
            likely: Certainty::Likely.default_weight(),
            possible: Certainty::Possible.default_weight(),
        }
    }
}

impl Weights {
    /// The weight of an edge at `c`; edges without a certainty count as certain.
    ///
    /// Example: `Weights::default().of(Some(Certainty::Likely))` is `0.6`.
    pub fn of(&self, c: Option<Certainty>) -> f64 {
        match c {
            None | Some(Certainty::Certain) => self.certain,
            Some(Certainty::Likely) => self.likely,
            Some(Certainty::Possible) => self.possible,
        }
    }
}

/// The graph a measure runs on. Its JSON form is the cache key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ProjectionSpec {
    pub kind: ProjectionKind,
    pub level: AggregationLevel,
    pub weights: Weights,
    pub min_certainty: Certainty,
    pub include_tests: bool,
    pub include_externals: bool,
}

impl Default for ProjectionSpec {
    fn default() -> Self {
        ProjectionSpec {
            kind: ProjectionKind::Invocation,
            level: AggregationLevel::Symbol,
            weights: Weights::default(),
            min_certainty: Certainty::Possible,
            include_tests: false,
            include_externals: false,
        }
    }
}

/// How an iterative measure ended.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Convergence {
    pub iterations: u32,
    pub converged: bool,
    /// The L1 change of the last iteration.
    pub delta: f64,
}

/// The first set of measures for one node of a projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct NodeMeasures {
    pub id: NodeId,
    /// Distinct callers (or dependents) in the projection.
    pub fan_in: u32,
    /// Distinct callees (or dependencies).
    pub fan_out: u32,
    pub fan_in_weighted: f64,
    pub fan_out_weighted: f64,
    /// Importance that collects where calls lead: code that much important
    /// code ends up calling ("must be bulletproof"). Sums to 1.
    pub pagerank: f64,
    /// Importance that collects where calls come from (orchestrators).
    pub pagerank_reverse: f64,
    pub hub: f64,
    pub authority: f64,
    /// k-core number on the undirected projection.
    pub core: u32,
    /// Index into `MeasureReport::components` when the node is in a cycle.
    pub component: Option<u32>,
}

/// An edge suggested for removal to break a cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CutEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub weight: f64,
    /// Underlying edges (symbol level) or symbol pairs (higher levels).
    pub count: u32,
}

/// A strongly connected component with more than one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Component {
    pub size: u32,
    /// Sorted.
    pub nodes: Vec<NodeId>,
    /// Edges whose removal leaves the component acyclic, low-weight edges
    /// preferred.
    pub cut: Vec<CutEdge>,
    /// Whether every edge in `cut` was checked to be needed (skipped on
    /// very large components).
    pub cut_minimal: bool,
}

/// The first set of measures on one projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct MeasureReport {
    pub projection: ProjectionSpec,
    /// Sorted by ID.
    pub nodes: Vec<NodeMeasures>,
    /// Largest first, then by first node.
    pub components: Vec<Component>,
    /// `pagerank`, `pagerank_reverse`, `hits`.
    pub convergence: BTreeMap<String, Convergence>,
}
