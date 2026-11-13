//! The refactoring checklist: a crater flattened to the places to verify.

use std::cmp::Reverse;

use aldente_graph::Graph;
use aldente_model::crater::{ChecklistRow, CraterResult};
use aldente_model::{NodeKind, SymbolFlag};

/// One row per symbol and impl in the crater, strongest path certainty
/// first, then nearest, then by ID. Coverage stays null until Phase 4.
///
/// Example: a reverse crater of `b` over `a -calls-> b` gives rows for `b`
/// (distance 0) and `a` (distance 1), both `certain`.
pub fn checklist(graph: &Graph, crater: &CraterResult) -> Vec<ChecklistRow> {
    let mut rows: Vec<ChecklistRow> = crater
        .reach
        .iter()
        .filter_map(|r| {
            let node = graph.nodes.get(&r.id)?;
            if !matches!(node.kind, NodeKind::Symbol | NodeKind::Impl) {
                return None;
            }
            let name = match &node.namespace {
                Some(ns) => format!("{ns}/{}", node.name),
                None => node.name.clone(),
            };
            Some(ChecklistRow {
                id: r.id.clone(),
                kind: node.kind,
                name,
                loc: node.loc.clone(),
                distance: r.distance,
                path_certainty: r.path_certainty,
                coverage: None,
                entry_point: node.flags.contains(&SymbolFlag::EntryPoint),
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        (Reverse(a.path_certainty), a.distance, &a.id).cmp(&(
            Reverse(b.path_certainty),
            b.distance,
            &b.id,
        ))
    });
    rows
}
