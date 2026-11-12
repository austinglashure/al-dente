//! Forward and reverse adjacency over a graph's edges, for traversals.

use std::collections::HashMap;

use aldente_model::{Edge, NodeId};

/// Edge indexes by source and by target.
#[derive(Debug, Clone, Default)]
pub struct Adjacency {
    forward: HashMap<NodeId, Vec<usize>>,
    reverse: HashMap<NodeId, Vec<usize>>,
}

impl Adjacency {
    /// Indexes `edges` by position.
    ///
    /// Example: for edges `[a→b, a→c]`, `outgoing(a)` is `[0, 1]` and
    /// `incoming(c)` is `[1]`.
    pub fn new(edges: &[Edge]) -> Adjacency {
        let mut adj = Adjacency::default();
        for (i, e) in edges.iter().enumerate() {
            adj.forward.entry(e.from.clone()).or_default().push(i);
            adj.reverse.entry(e.to.clone()).or_default().push(i);
        }
        adj
    }

    /// Positions of edges leaving `id`.
    ///
    /// Example: see [`Adjacency::new`].
    pub fn outgoing(&self, id: &NodeId) -> &[usize] {
        self.forward.get(id).map_or(&[], Vec::as_slice)
    }

    /// Positions of edges arriving at `id`.
    ///
    /// Example: see [`Adjacency::new`].
    pub fn incoming(&self, id: &NodeId) -> &[usize] {
        self.reverse.get(id).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::EdgeKind;
    use std::str::FromStr;

    fn e(from: &str, to: &str) -> Edge {
        Edge {
            from: NodeId::from_str(from).unwrap(),
            to: NodeId::from_str(to).unwrap(),
            kind: EdgeKind::Contains,
            certainty: None,
            reason: None,
            lang: None,
            sites: vec![],
            count: None,
            by_certainty: None,
        }
    }

    #[test]
    fn indexes_both_directions() {
        let edges = [e("clj:.//a", "clj:.//b"), e("clj:.//a", "clj:.//c")];
        let adj = Adjacency::new(&edges);
        let a = NodeId::from_str("clj:.//a").unwrap();
        let c = NodeId::from_str("clj:.//c").unwrap();
        assert_eq!(adj.outgoing(&a), [0, 1]);
        assert_eq!(adj.incoming(&c), [1]);
        assert!(adj.incoming(&a).is_empty());
    }
}
