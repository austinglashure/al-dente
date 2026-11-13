//! Views of one graph that are expensive to compute and never change while
//! the graph does not: the namespace and unit rollups and an edge index.
//! `serve` keeps one [`Cache`] per snapshot; one-shot CLI commands use a
//! fresh one.

use std::borrow::Cow;
use std::sync::OnceLock;

use aldente_model::{AggregationLevel, GraphPayload};

use crate::adjacency::Adjacency;
use crate::build::Graph;

/// Lazily computed views of a graph. Always use it with the same graph.
#[derive(Debug, Default)]
pub struct Cache {
    namespace: OnceLock<GraphPayload>,
    unit: OnceLock<GraphPayload>,
    adjacency: OnceLock<Adjacency>,
}

impl Cache {
    /// The graph at `level`: the namespace and unit rollups are computed on
    /// first use and borrowed after; the symbol level is the stored graph,
    /// copied each time.
    ///
    /// Example: two `payload(g, Namespace)` calls roll up once.
    pub fn payload<'a>(&'a self, graph: &Graph, level: AggregationLevel) -> Cow<'a, GraphPayload> {
        match level {
            AggregationLevel::Namespace => {
                Cow::Borrowed(self.namespace.get_or_init(|| graph.payload(level)))
            }
            AggregationLevel::Unit => Cow::Borrowed(self.unit.get_or_init(|| graph.payload(level))),
            AggregationLevel::Symbol => Cow::Owned(graph.payload(level)),
        }
    }

    /// The stored edges indexed by source and target, built on first use.
    ///
    /// Example: `cache.adjacency(g).outgoing(&checkout)` lists checkout's edges.
    pub fn adjacency(&self, graph: &Graph) -> &Adjacency {
        self.adjacency.get_or_init(|| Adjacency::new(&graph.edges))
    }
}
