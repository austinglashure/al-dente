//! M2 statistics for an analyzed repository (WP2.10): edges by kind,
//! certainty and reason, and orphaned symbols.

use std::collections::{BTreeMap, BTreeSet};

use aldente_graph::Graph;
use aldente_model::{EdgeKind, NodeId, NodeKind, SymbolFlag, SymbolSubkind};
use serde::Serialize;

/// The JSON token of a serializable enum value.
///
/// Example: `token(EdgeKind::Calls)` is `"calls"`.
fn token<T: Serialize>(v: T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

/// Edge counts keyed `kind/certainty/reason`, `-` where absent.
///
/// Example: `{"calls/certain/direct": 120, "contains/-/-": 40, …}`.
pub fn edge_breakdown(g: &Graph) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for e in &g.edges {
        let key = format!(
            "{}/{}/{}",
            token(e.kind),
            e.certainty.map_or("-".to_owned(), token),
            e.reason.map_or("-".to_owned(), token)
        );
        *out.entry(key).or_default() += 1;
    }
    out
}

/// Orphaned repository symbols.
#[derive(Debug, Clone, Serialize)]
pub struct Orphans {
    /// Repository symbols that are not entry points, tests, or in a test
    /// namespace.
    pub eligible: usize,
    /// Of those, how many have no incoming edge (other than `contains`).
    pub orphaned: usize,
    pub share: f64,
    /// The largest orphaned functions and macros, by lines, then ID.
    pub largest: Vec<Orphan>,
}

/// One orphaned function.
#[derive(Debug, Clone, Serialize)]
pub struct Orphan {
    pub id: String,
    pub lines: u32,
    pub location: String,
}

/// Orphan statistics with the `top` largest orphaned functions.
///
/// Example: on `clj-rules/entry-points` 2 of 3 eligible symbols are orphaned.
pub fn orphans(g: &Graph, top: usize) -> Orphans {
    let incoming: BTreeSet<&NodeId> = g
        .edges
        .iter()
        .filter(|e| e.kind != EdgeKind::Contains)
        .map(|e| &e.to)
        .collect();
    let test_namespaces: BTreeSet<String> = g
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Namespace && n.flags.contains(&SymbolFlag::Test))
        .map(|n| n.name.clone())
        .collect();
    let eligible: Vec<_> = g
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Symbol && n.loc.is_some())
        .filter(|n| {
            !n.flags.contains(&SymbolFlag::EntryPoint) && !n.flags.contains(&SymbolFlag::Test)
        })
        .filter(|n| {
            !n.namespace
                .as_ref()
                .is_some_and(|ns| test_namespaces.contains(ns))
        })
        .collect();
    let orphaned: Vec<_> = eligible
        .iter()
        .filter(|n| !incoming.contains(&n.id))
        .collect();
    let mut largest: Vec<Orphan> = orphaned
        .iter()
        .filter(|n| matches!(n.subkind, Some(SymbolSubkind::Fn | SymbolSubkind::Macro)))
        .filter_map(|n| {
            let l = n.loc.as_ref()?;
            Some(Orphan {
                id: n.id.to_string(),
                lines: l.end_line.saturating_sub(l.line) + 1,
                location: format!("{}:{}", l.file, l.line),
            })
        })
        .collect();
    largest.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.id.cmp(&b.id)));
    largest.truncate(top);
    Orphans {
        eligible: eligible.len(),
        orphaned: orphaned.len(),
        share: if eligible.is_empty() {
            0.0
        } else {
            orphaned.len() as f64 / eligible.len() as f64
        },
        largest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_testkit::{analyze_fixture, fixture_dir};

    #[test]
    fn orphans_exclude_entry_points_tests_and_test_namespaces() {
        let g = analyze_fixture(&fixture_dir("clj-rules/entry-points")).graph;
        let o = orphans(&g, 20);
        assert_eq!(o.eligible, 3, "helper, serve-other, newest");
        let ids: Vec<&str> = o.largest.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            ["clj:.//app.core/serve-other", "clj:.//app.latest/newest"]
        );
        assert_eq!(o.orphaned, 2);
        assert_eq!(o.largest[0].lines, 2);
    }

    #[test]
    fn edge_breakdown_keys_are_kind_certainty_reason() {
        let g = analyze_fixture(&fixture_dir("clj-rules/entry-points")).graph;
        let b = edge_breakdown(&g);
        assert!(
            b.get("calls/certain/direct").is_some_and(|n| *n > 0),
            "{b:?}"
        );
        assert!(b.contains_key("contains/-/-"));
    }
}
