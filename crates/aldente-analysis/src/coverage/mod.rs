//! Static test coverage: from each test (R14), search forward over
//! invocation edges (not into other tests, but through test-support code
//! such as helpers in test namespaces) and give every non-test symbol a status;
//! roll it up per namespace and unit; merge an imported measured report.
//! See "Coverage" in `docs/plan/09-network-and-coverage.md`.

pub mod import;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use aldente_graph::Graph;
use aldente_model::coverage::{
    CoverageReport, CoverageStatus, Disagreement, GroupCoverage, StatusShares, SymbolCoverage,
};
use aldente_model::{Certainty, EdgeKind, Node, NodeId, NodeKind};

use crate::measures::projection::{is_test, is_test_itself};
pub use import::{LineHits, ParseError, parse_report};

/// Default depth limit for `indirect`.
pub const DEFAULT_DEPTH: u32 = 4;
/// Most covering tests listed per symbol.
const TEST_CAP: usize = 10;

/// Edge kinds a test's reach follows.
const FOLLOW: &[EdgeKind] = &[
    EdgeKind::Calls,
    EdgeKind::References,
    EdgeKind::Dispatches,
    EdgeKind::Registers,
];

/// Whether a node gets a coverage status: repository symbols and impls
/// that are not tests and not top-level forms.
///
/// Example: `shop.core/checkout` does; a `deftest` and `<top>@L10` do not.
fn subject(graph: &Graph, n: &Node) -> bool {
    matches!(n.kind, NodeKind::Symbol | NodeKind::Impl)
        && !is_test(graph, n)
        && !n.name.starts_with("<top>@")
}

/// Whether coverage reach starts at a node: tests (R14), and test-namespace
/// code that runs when the tests load or run (top-level forms, `def`
/// values, method bodies). Helper functions in a test namespace are not
/// starts; they run only when called, and reach passes through them.
///
/// Example: a `deftest`, `(def store (cookie-store))` and a top-level
/// `(defservice …)` in a test namespace are starts; a `defn` helper there
/// is not.
fn reach_starts_at(graph: &Graph, n: &Node) -> bool {
    use aldente_model::SymbolSubkind;
    let helper_fn = n.kind == NodeKind::Symbol
        && matches!(n.subkind, Some(SymbolSubkind::Fn | SymbolSubkind::Macro));
    matches!(n.kind, NodeKind::Symbol | NodeKind::Impl)
        && (is_test_itself(n) || (is_test(graph, n) && !helper_fn))
}

/// Forward adjacency over the followed kinds: `(target, certainty)`.
///
/// Example: `a -calls(likely)-> b` gives `a: [(b, likely)]`.
fn adjacency(graph: &Graph) -> HashMap<&NodeId, Vec<(&NodeId, Certainty)>> {
    let mut out: HashMap<&NodeId, Vec<(&NodeId, Certainty)>> = HashMap::new();
    for e in graph.edges.iter().filter(|e| FOLLOW.contains(&e.kind)) {
        out.entry(&e.from)
            .or_default()
            .push((&e.to, e.certainty.unwrap_or(Certainty::Certain)));
    }
    out
}

/// The estimate per subject symbol, sorted by ID.
///
/// Example: in clj-mini, `shop.pricing/price` is `direct` (both tests call
/// it) and `shop.inventory/restock!` is `untested`.
pub fn estimate(graph: &Graph, depth: u32) -> (Vec<SymbolCoverage>, u32) {
    let adj = adjacency(graph);
    let tests: Vec<&NodeId> = graph
        .nodes
        .values()
        .filter(|n| reach_starts_at(graph, n))
        .map(|n| &n.id)
        .collect();
    let test_count = graph
        .nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::Symbol | NodeKind::Impl) && is_test_itself(n))
        .count() as u32;
    let test_set: BTreeSet<&NodeId> = tests.iter().copied().collect();
    // Within the depth limit at likely or better: per test, BFS by hops.
    let mut near: HashMap<&NodeId, (u32, BTreeSet<&NodeId>)> = HashMap::new();
    for &t in &tests {
        let mut seen: HashMap<&NodeId, u32> = HashMap::from([(t, 0)]);
        let mut queue = VecDeque::from([t]);
        while let Some(v) = queue.pop_front() {
            let d = seen[v];
            if d == depth {
                continue;
            }
            for &(w, c) in adj.get(v).map(Vec::as_slice).unwrap_or(&[]) {
                if c < Certainty::Likely || test_set.contains(w) || seen.contains_key(w) {
                    continue;
                }
                seen.insert(w, d + 1);
                queue.push_back(w);
                let e = near.entry(w).or_insert((u32::MAX, BTreeSet::new()));
                e.0 = e.0.min(d + 1);
                e.1.insert(t);
            }
        }
    }
    // Any certainty, any depth: one multi-source BFS for distance, and the
    // tests reaching each node (for `weak` rows).
    let mut far: HashMap<&NodeId, (u32, BTreeSet<&NodeId>)> = HashMap::new();
    for &t in &tests {
        let mut seen: BTreeSet<&NodeId> = BTreeSet::from([t]);
        let mut queue: VecDeque<(&NodeId, u32)> = VecDeque::from([(t, 0)]);
        while let Some((v, d)) = queue.pop_front() {
            for &(w, _) in adj.get(v).map(Vec::as_slice).unwrap_or(&[]) {
                if test_set.contains(w) || !seen.insert(w) {
                    continue;
                }
                queue.push_back((w, d + 1));
                let e = far.entry(w).or_insert((u32::MAX, BTreeSet::new()));
                e.0 = e.0.min(d + 1);
                e.1.insert(t);
            }
        }
    }
    let rows = graph
        .nodes
        .values()
        .filter(|n| subject(graph, n))
        .map(|n| {
            let (status, path_certainty, covering) = match (near.get(&n.id), far.get(&n.id)) {
                (Some((d, ts)), _) => {
                    let s = if *d == 1 {
                        CoverageStatus::Direct
                    } else {
                        CoverageStatus::Indirect
                    };
                    (
                        s,
                        best_certainty(&adj, &test_set, ts, &n.id, depth),
                        ts.clone(),
                    )
                }
                (None, Some((_, ts))) => (CoverageStatus::Weak, None, ts.clone()),
                (None, None) => (CoverageStatus::Untested, None, BTreeSet::new()),
            };
            SymbolCoverage {
                id: n.id.clone(),
                status,
                distance: far.get(&n.id).map(|(d, _)| *d),
                path_certainty,
                test_count: covering.len() as u32,
                tests: covering.into_iter().take(TEST_CAP).cloned().collect(),
                measured: None,
            }
        })
        .collect();
    (rows, test_count)
}

/// The strongest path certainty from any of `tests` to `target` within
/// `depth` hops (a widest-path search over the edges at `likely` or better).
///
/// Example: a test calling `f` (certain) and `f` referencing `g` (likely)
/// gives `likely` for `g`.
fn best_certainty(
    adj: &HashMap<&NodeId, Vec<(&NodeId, Certainty)>>,
    test_set: &BTreeSet<&NodeId>,
    tests: &BTreeSet<&NodeId>,
    target: &NodeId,
    depth: u32,
) -> Option<Certainty> {
    for level in [Certainty::Certain, Certainty::Likely] {
        let mut seen: BTreeSet<&NodeId> = tests.iter().copied().collect();
        let mut frontier: Vec<&NodeId> = tests.iter().copied().collect();
        for _ in 0..depth {
            let mut next = Vec::new();
            for v in frontier {
                for &(w, c) in adj.get(v).map(Vec::as_slice).unwrap_or(&[]) {
                    if c >= level && !test_set.contains(w) && seen.insert(w) {
                        if w == target {
                            return Some(level);
                        }
                        next.push(w);
                    }
                }
            }
            frontier = next;
        }
    }
    None
}

/// Merges measured line hits into the rows: a symbol's measured coverage is
/// covered over instrumented lines inside its source range.
///
/// Example: a symbol on lines 3–5 with hits `{3: 1, 4: 0}` measures 0.5.
pub fn merge_measured(graph: &Graph, rows: &mut [SymbolCoverage], hits: &LineHits) {
    for row in rows.iter_mut() {
        let Some(loc) = graph.nodes.get(&row.id).and_then(|n| n.loc.as_ref()) else {
            continue;
        };
        let Some(lines) = hits.get(loc.file.as_str()) else {
            continue;
        };
        let (mut total, mut covered) = (0u32, 0u32);
        for (_, &h) in lines.range(loc.line..=loc.end_line.max(loc.line)) {
            total += 1;
            if h > 0 {
                covered += 1;
            }
        }
        if total > 0 {
            row.measured = Some(f64::from(covered) / f64::from(total));
        }
    }
}

/// Estimates the measured report contradicts.
///
/// Example: a `direct` symbol measured at 0 is listed as
/// `estimated-covered-measured-zero`.
pub fn disagreements(rows: &[SymbolCoverage]) -> Vec<Disagreement> {
    rows.iter()
        .filter_map(|r| {
            let m = r.measured?;
            let kind = match r.status {
                CoverageStatus::Direct | CoverageStatus::Indirect if m == 0.0 => {
                    "estimated-covered-measured-zero"
                }
                CoverageStatus::Untested if m > 0.0 => "measured-covered-estimated-untested",
                _ => return None,
            };
            Some(Disagreement {
                id: r.id.clone(),
                estimated: r.status,
                measured: m,
                kind: kind.into(),
            })
        })
        .collect()
}

/// Namespace or unit roll-ups, shares weighted by symbol size in lines.
///
/// Example: a namespace with a 10-line direct symbol and a 30-line
/// untested one is 0.25 direct and 0.75 untested.
pub fn roll_up(
    graph: &Graph,
    rows: &[SymbolCoverage],
    group: impl Fn(&NodeId) -> Option<NodeId>,
) -> Vec<GroupCoverage> {
    #[derive(Default)]
    struct Acc {
        n: u32,
        w: [f64; 4],
        covered: f64,
        instrumented: f64,
    }
    let mut acc: BTreeMap<NodeId, Acc> = BTreeMap::new();
    for r in rows {
        let Some(g) = group(&r.id) else { continue };
        let lines = graph
            .nodes
            .get(&r.id)
            .and_then(|n| n.loc.as_ref())
            .map_or(1.0, |l| f64::from(l.end_line.max(l.line) - l.line + 1));
        let a = acc.entry(g).or_default();
        a.n += 1;
        a.w[r.status as usize] += lines;
        if let Some(m) = r.measured {
            a.covered += m * lines;
            a.instrumented += lines;
        }
    }
    acc.into_iter()
        .map(|(id, a)| {
            let total: f64 = a.w.iter().sum::<f64>().max(f64::MIN_POSITIVE);
            GroupCoverage {
                id,
                symbols: a.n,
                weighted: StatusShares {
                    direct: a.w[0] / total,
                    indirect: a.w[1] / total,
                    weak: a.w[2] / total,
                    untested: a.w[3] / total,
                },
                measured: (a.instrumented > 0.0).then(|| a.covered / a.instrumented),
            }
        })
        .collect()
}

/// The whole report: estimate, optional measured merge, roll-ups.
///
/// Example: `coverage(&graph, 4, None)` for the estimate alone.
pub fn coverage(
    graph: &Graph,
    depth: u32,
    measured: Option<(&LineHits, aldente_model::coverage::ImportInfo)>,
) -> CoverageReport {
    let (mut symbols, tests) = estimate(graph, depth);
    let mut imported = None;
    if let Some((hits, info)) = measured {
        merge_measured(graph, &mut symbols, hits);
        imported = Some(info);
    }
    let namespaces = roll_up(graph, &symbols, |id| graph.namespace_of(id));
    let units = roll_up(graph, &symbols, |id| graph.unit_of(id));
    CoverageReport {
        estimated: true,
        depth,
        tests,
        disagreements: disagreements(&symbols),
        symbols,
        namespaces,
        units,
        imported,
    }
}

/// Fills checklist rows' coverage from a report.
///
/// Example: a row for `shop.pricing/price` gets `direct`.
pub fn annotate_checklist(
    rows: &mut [aldente_model::crater::ChecklistRow],
    report: &CoverageReport,
) {
    let by_id: HashMap<&NodeId, CoverageStatus> =
        report.symbols.iter().map(|s| (&s.id, s.status)).collect();
    for row in rows {
        row.coverage = by_id.get(&row.id).copied();
    }
}
