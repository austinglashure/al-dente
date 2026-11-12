//! Counts for a snapshot, shared by `aldente analyze` and the server.

use std::collections::BTreeMap;

use aldente_model::Diagnostic;
use aldente_model::analyze::AnalyzeSummary;

use crate::snapshot::Snapshot;

/// Counts nodes, edges and blind spots by kind; `extra` diagnostics (e.g.
/// submodules found while cloning) are merged in.
///
/// Example: clj-mini gives `units: 1, files: 13, nodes: {namespace: 13, …}`.
pub fn summarize(snap: &Snapshot, extra: &[Diagnostic]) -> AnalyzeSummary {
    let mut nodes = BTreeMap::new();
    for n in snap.graph.nodes.values() {
        *nodes.entry(n.kind).or_default() += 1;
    }
    let mut edges = BTreeMap::new();
    let mut edge_certainty: BTreeMap<_, BTreeMap<_, usize>> = BTreeMap::new();
    for e in &snap.graph.edges {
        *edges.entry(e.kind).or_default() += 1;
        if let Some(c) = e.certainty {
            *edge_certainty
                .entry(e.kind)
                .or_default()
                .entry(c)
                .or_default() += 1;
        }
    }
    let mut blindspots = BTreeMap::new();
    for b in &snap.graph.blindspots {
        *blindspots.entry(b.category).or_default() += 1;
    }
    let mut diagnostics = snap.graph.diagnostics.clone();
    diagnostics.extend(extra.iter().cloned());
    diagnostics.sort();
    diagnostics.dedup();
    AnalyzeSummary {
        units: snap.units.len(),
        files: snap.units.iter().map(|u| u.files).sum(),
        nodes,
        edges,
        edge_certainty,
        blindspots,
        diagnostics,
    }
}
