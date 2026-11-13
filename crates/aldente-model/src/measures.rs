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

/// How betweenness and closeness were computed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Sampling {
    /// False when every node was a source (exact).
    pub approximate: bool,
    /// Sources used.
    pub pivots: u32,
    /// Seed of the pivot choice.
    pub seed: u64,
}

/// The second set of measures for one node of a projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct NodeMeasures2 {
    pub id: NodeId,
    /// Brandes betweenness over hop distances, normalized by (n−1)(n−2).
    pub betweenness: f64,
    /// Sum of 1 / distance from every other node that reaches this one.
    pub closeness: f64,
    /// Eigenvector centrality within the node's strongly connected
    /// component (0 outside cycles), L2-normalized per component.
    pub eigenvector: f64,
    /// Index into `SecondReport::communities`.
    pub community: u32,
    /// Nodes this one dominates from the entry points, itself excluded.
    pub dominates: u32,
    pub articulation: bool,
    /// 0..1, the mean of five percentile ranks (see `god_object`).
    pub god_score: f64,
}

/// One community found by Louvain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Community {
    pub size: u32,
    /// Namespaces of its members (symbol level), sorted.
    pub namespaces: Vec<String>,
    /// Units of its members, sorted.
    pub units: Vec<NodeId>,
}

/// A namespace whose symbols fall into several communities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SplitNamespace {
    pub namespace: String,
    /// Community index → number of the namespace's symbols in it.
    pub communities: BTreeMap<u32, u32>,
}

/// An undirected edge whose removal disconnects part of the graph.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Bridge {
    pub a: NodeId,
    pub b: NodeId,
}

/// The second set of measures on one projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SecondReport {
    pub projection: ProjectionSpec,
    /// Sorted by ID.
    pub nodes: Vec<NodeMeasures2>,
    pub sampling: Sampling,
    pub communities: Vec<Community>,
    pub modularity: f64,
    /// Seed of the community search.
    pub community_seed: u64,
    pub split_namespaces: Vec<SplitNamespace>,
    pub bridges: Vec<Bridge>,
    /// Entry points the dominators were computed from.
    pub entry_points: u32,
    /// Per strongly connected component, as in `MeasureReport::components`.
    pub eigenvector_convergence: Vec<Convergence>,
}

/// Coupling of one namespace or unit (Robert Martin's package metrics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Coupling {
    pub id: NodeId,
    /// Afferent coupling: distinct namespaces (units) depending on this one.
    pub afferent: u32,
    /// Efferent coupling: distinct namespaces (units) this one depends on.
    pub efferent: u32,
    /// efferent / (afferent + efferent); 0 when both are 0.
    pub instability: f64,
    /// Protocols, multimethods and interfaces over all definitions.
    pub abstractness: f64,
    /// |abstractness + instability − 1|.
    pub distance: f64,
    /// Layer in the condensation: 0 depends on nothing in the projection.
    pub layer: u32,
}

/// A dependency that breaks a design rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Violation {
    pub from: NodeId,
    pub to: NodeId,
    /// `upward` (against the layering: an edge inside a cycle that the cut
    /// suggestion removes) or `unstable-dependency` (a stabler module
    /// depending on a less stable one).
    pub kind: String,
}

/// Coupling, layers and violations at namespace or unit level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CouplingReport {
    pub projection: ProjectionSpec,
    /// Sorted by ID.
    pub modules: Vec<Coupling>,
    pub violations: Vec<Violation>,
}

/// A symbol nothing reaches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct DeadSymbol {
    pub id: NodeId,
    pub name: String,
    pub namespace: Option<String>,
    pub loc: Option<crate::graph::Location>,
}

/// Symbols unreachable from any entry point, test or load-time code over
/// every invocation edge at any certainty.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct DeadCodeReport {
    /// Roots the search started from.
    pub roots: u32,
    /// Sorted by ID.
    pub dead: Vec<DeadSymbol>,
}
