//! Seeds from source changes: which symbols a set of changed lines touches.

use std::collections::BTreeMap;

use aldente_graph::Graph;
use aldente_model::{NodeId, NodeKind, RepoPath};

use crate::textdiff::{Hunk, touches};

/// Symbols and impls whose source range (in the current graph) touches the
/// new side of a hunk in their file, sorted. One pass over the nodes.
///
/// A symbol's range runs from its first line to its last; a comment or
/// blank line just above it is outside, so changing only that seeds nothing.
///
/// Example: with `f` on lines 3–6 of `src/a.clj`, a hunk replacing line 5
/// seeds `f`; one replacing line 2 seeds nothing.
pub fn changed_symbols(graph: &Graph, hunks: &BTreeMap<RepoPath, Vec<Hunk>>) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = graph
        .nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::Symbol | NodeKind::Impl))
        .filter_map(|n| {
            let loc = n.loc.as_ref()?;
            let file_hunks = hunks.get(&loc.file)?;
            file_hunks
                .iter()
                .any(|h| touches(h.new, loc.line, loc.end_line.max(loc.line)))
                .then(|| n.id.clone())
        })
        .collect();
    out.sort();
    out
}
