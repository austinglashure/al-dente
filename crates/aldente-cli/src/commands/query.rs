//! Helpers shared by `find`, `show` and `graph`: resolving user-typed names.

use aldente_graph::Graph;
use aldente_graph::query::{Resolution, resolve};
use aldente_model::{ErrorCode, NodeId};

use crate::commands::target::Failure;

/// Resolves a full ID or unambiguous suffix; ambiguity and absence are
/// failures with exit code 3 (ambiguity lists the candidates).
///
/// Example: `node_id(g, "charge!")` in mono-mini fails with two candidates.
pub fn node_id(graph: &Graph, query: &str) -> Result<NodeId, Failure> {
    match resolve(graph, query) {
        Resolution::Found(id) => Ok(id),
        Resolution::NotFound => Err(Failure::new(
            ErrorCode::UnknownSymbol,
            format!("no node matches {query:?}"),
        )),
        Resolution::Ambiguous(ids) => {
            let mut f = Failure::new(
                ErrorCode::AmbiguousSymbol,
                format!(
                    "{query:?} matches {} nodes; give more of the name",
                    ids.len()
                ),
            );
            f.candidates = Some(ids.iter().map(ToString::to_string).collect());
            Err(f)
        }
    }
}
