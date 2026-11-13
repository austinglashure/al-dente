//! Hotspots: measures turned into ranked findings with evidence and a
//! reason, and the risk score (importance × exposure). Thresholds come from
//! `rules/hotspots.toml`. Pure: every input is computed elsewhere.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};

use aldente_graph::Graph;
use aldente_model::NodeId;
use aldente_model::churn::ChurnReport;
use aldente_model::coverage::{CoverageReport, CoverageStatus};
use aldente_model::hotspots::{Finding, FindingKind, HotspotsReport, RiskRow};
use aldente_model::measures::{Component, DeadCodeReport, MeasureReport, SecondReport};
use aldente_rules::hotspots::HotspotRules;

use crate::measures::god::percentile_ranks;

/// What the findings are computed from.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub graph: &'a Graph,
    /// Symbol-level invocation measures.
    pub first: &'a MeasureReport,
    pub second: &'a SecondReport,
    pub coverage: &'a CoverageReport,
    pub dead: &'a DeadCodeReport,
    /// Cycles at namespace and unit level (structure or invocation).
    pub namespace_cycles: &'a [Component],
    pub unit_cycles: &'a [Component],
    pub churn: Option<&'a ChurnReport>,
}

/// The hotspots report.
///
/// Example: on clj-mini, `shop.pricing/price` is a risk-ranked symbol and
/// the never-called `shop.plugins/run-script` is a dead-code finding.
pub fn hotspots(inp: &Inputs<'_>, rules: &HotspotRules, rules_source: &str) -> HotspotsReport {
    let risk = risk_rows(inp, rules);
    let mut findings = Vec::new();
    findings.extend(god_objects(inp, rules));
    findings.extend(bulletproof(inp, rules));
    findings.extend(bottlenecks(inp, rules));
    findings.extend(cycles(inp, rules));
    findings.extend(hidden_coupling(inp, rules));
    let dead: std::collections::HashSet<&NodeId> = inp.dead.dead.iter().map(|d| &d.id).collect();
    findings.extend(risky_untested(&risk, &dead, rules));
    findings.extend(misplaced(inp, rules));
    if rules.dead_code.enabled {
        findings.extend(dead_code(inp));
    }
    findings.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.kind.cmp(&b.kind))
            .then(a.subject.cmp(&b.subject))
    });
    HotspotsReport {
        rules: rules_source.to_owned(),
        churn: inp.churn.is_some(),
        findings,
        risk,
    }
}

/// `namespace/name` of a node, or its ID.
///
/// Example: `clj:.//shop.core/checkout` gives `shop.core/checkout`.
fn label(graph: &Graph, id: &NodeId) -> String {
    graph
        .nodes
        .get(id)
        .and_then(|n| n.namespace.as_ref().map(|ns| format!("{ns}/{}", n.name)))
        .unwrap_or_else(|| id.to_string())
}

/// Percentile ranks (0–100) with "above threshold and above zero".
///
/// Example: `top(&[0.0, 1.0, 2.0], 50.0)` is `[false, true, true]`.
fn top(values: &[f64], percentile: f64) -> Vec<bool> {
    percentile_ranks(values)
        .iter()
        .zip(values)
        .map(|(r, v)| r * 100.0 >= percentile && *v > 0.0)
        .collect()
}

/// A finding about one node.
///
/// Example: `single(FindingKind::DeadCode, id, 0.5, [], "never called")`.
fn single<const N: usize>(
    kind: FindingKind,
    id: &NodeId,
    score: f64,
    evidence: [(&str, f64); N],
    reason: String,
) -> Finding {
    Finding {
        kind,
        subject: id.to_string(),
        nodes: vec![id.clone()],
        score,
        evidence: evidence
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect(),
        reason,
    }
}

/// Values scaled to 0..1 by their maximum.
///
/// Example: `scaled(&[1.0, 4.0])` is `[0.25, 1.0]`.
fn scaled(values: &[f64]) -> Vec<f64> {
    let max = values.iter().copied().fold(0.0, f64::max);
    values
        .iter()
        .map(|v| if max > 0.0 { v / max } else { 0.0 })
        .collect()
}

/// Risk per symbol with a coverage status, highest first.
///
/// Example: an untested symbol with the top PageRank, fan-in and
/// dominator reach has risk 1.0.
pub fn risk_rows(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<RiskRow> {
    let r = &rules.risk;
    let n = inp.first.nodes.len();
    let pr = scaled(
        &inp.first
            .nodes
            .iter()
            .map(|x| x.pagerank)
            .collect::<Vec<_>>(),
    );
    let fan = scaled(
        &inp.first
            .nodes
            .iter()
            .map(|x| x.fan_in_weighted)
            .collect::<Vec<_>>(),
    );
    let dom_by: HashMap<&NodeId, u32> = inp
        .second
        .nodes
        .iter()
        .map(|x| (&x.id, x.dominates))
        .collect();
    let dom = scaled(
        &inp.first
            .nodes
            .iter()
            .map(|x| f64::from(dom_by.get(&x.id).copied().unwrap_or(0)))
            .collect::<Vec<_>>(),
    );
    let total_w = (r.pagerank + r.fan_in + r.dominator).max(f64::MIN_POSITIVE);
    let cov: HashMap<&NodeId, (CoverageStatus, Option<f64>)> = inp
        .coverage
        .symbols
        .iter()
        .map(|s| (&s.id, (s.status, s.measured)))
        .collect();
    let churn: HashMap<&NodeId, u32> = inp
        .churn
        .map(|c| c.symbols.iter().map(|s| (&s.id, s.commits)).collect())
        .unwrap_or_default();
    let max_churn = churn.values().copied().max().unwrap_or(0);
    let mut rows: Vec<RiskRow> = (0..n)
        .filter_map(|i| {
            let id = &inp.first.nodes[i].id;
            let &(status, measured) = cov.get(id)?;
            let importance =
                (r.pagerank * pr[i] + r.fan_in * fan[i] + r.dominator * dom[i]) / total_w;
            let exposure = match measured {
                Some(m) => 1.0 - m,
                None => match status {
                    CoverageStatus::Untested => r.untested,
                    CoverageStatus::Weak => r.weak,
                    CoverageStatus::Indirect => r.indirect,
                    CoverageStatus::Direct => r.direct,
                },
            };
            let commits = inp.churn.map(|_| churn.get(id).copied().unwrap_or(0));
            let churn_factor = match (commits, max_churn) {
                (Some(c), m) if m > 0 => 1.0 + f64::from(c) / f64::from(m),
                _ => 1.0,
            };
            Some(RiskRow {
                id: id.clone(),
                risk: importance * exposure * churn_factor,
                importance,
                exposure,
                status,
                measured,
                churn: commits,
            })
        })
        .collect();
    rows.sort_by(|a, b| b.risk.total_cmp(&a.risk).then(a.id.cmp(&b.id)));
    rows
}

/// God objects: god-object score at or above the percentile.
///
/// Example: a function calling into every namespace from a long body.
fn god_objects(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<Finding> {
    let scores: Vec<f64> = inp.second.nodes.iter().map(|n| n.god_score).collect();
    let first: HashMap<&NodeId, &aldente_model::measures::NodeMeasures> =
        inp.first.nodes.iter().map(|n| (&n.id, n)).collect();
    top(&scores, rules.god_object.percentile)
        .iter()
        .zip(&inp.second.nodes)
        .filter(|(hit, _)| **hit)
        .filter_map(|(_, n)| {
            let f = first.get(&n.id)?;
            Some(single(
                FindingKind::GodObject,
                &n.id,
                n.god_score,
                [("god_score", n.god_score), ("fan_out", f64::from(f.fan_out)), ("hub", f.hub), ("core", f64::from(f.core))],
                format!(
                    "{} does too much: it calls {} functions and sits in the densest core (k = {}); split it by responsibility.",
                    label(inp.graph, &n.id),
                    f.fan_out,
                    f.core
                ),
            ))
        })
        .collect()
}

/// Must be bulletproof: top PageRank, or dominating a large share of the code.
///
/// Example: a validation function every request path calls.
fn bulletproof(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<Finding> {
    let b = &rules.must_be_bulletproof;
    let pr: Vec<f64> = inp.first.nodes.iter().map(|n| n.pagerank).collect();
    let ranks = percentile_ranks(&pr);
    let hits = top(&pr, b.pagerank_percentile);
    let dom: HashMap<&NodeId, u32> = inp
        .second
        .nodes
        .iter()
        .map(|x| (&x.id, x.dominates))
        .collect();
    let total = inp.first.nodes.len().max(1) as f64;
    inp.first
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            // An entry point trivially dominates what only it reaches.
            let entry = inp
                .graph
                .nodes
                .get(&n.id)
                .is_some_and(|x| x.flags.contains(&aldente_model::SymbolFlag::EntryPoint));
            let d = f64::from(dom.get(&n.id).copied().unwrap_or(0));
            let share = if entry { 0.0 } else { d / total };
            if !hits[i] && share <= b.dominator_share {
                return None;
            }
            let why = if hits[i] {
                format!(
                    "much important code ends up calling it (PageRank in the top {:.0}%)",
                    100.0 - ranks[i] * 100.0
                )
            } else {
                format!(
                    "every path from the entry points to {d:.0} function{} passes through it",
                    if d == 1.0 { "" } else { "s" }
                )
            };
            Some(single(
                FindingKind::MustBeBulletproof,
                &n.id,
                ranks[i].max(share),
                [
                    ("pagerank", n.pagerank),
                    ("pagerank_percentile", ranks[i] * 100.0),
                    ("dominates", d),
                    ("fan_in", f64::from(n.fan_in)),
                ],
                format!(
                    "{}: {why}; test it directly and harden its error handling.",
                    label(inp.graph, &n.id)
                ),
            ))
        })
        .collect()
}

/// Bottlenecks: top betweenness (and an articulation point, if required).
///
/// Example: the one function joining the UI code to the persistence code.
fn bottlenecks(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<Finding> {
    let b = &rules.bottleneck;
    let bt: Vec<f64> = inp.second.nodes.iter().map(|n| n.betweenness).collect();
    let ranks = percentile_ranks(&bt);
    top(&bt, b.betweenness_percentile)
        .iter()
        .zip(&inp.second.nodes)
        .enumerate()
        .filter(|(_, (hit, n))| **hit && (!b.articulation || n.articulation))
        .map(|(i, (_, n))| {
            single(
                FindingKind::Bottleneck,
                &n.id,
                ranks[i],
                [("betweenness", n.betweenness), ("articulation", f64::from(u8::from(n.articulation)))],
                format!(
                    "{}: many call paths run through it{}; a change here ripples widely, so keep its interface stable.",
                    label(inp.graph, &n.id),
                    if n.articulation { " and removing it disconnects part of the graph" } else { "" }
                ),
            )
        })
        .collect()
}

/// Cycles: every namespace or unit cycle, and large symbol cycles.
///
/// Example: `a.core` and `a.util` calling each other.
fn cycles(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut add = |c: &Component, what: &str, score: f64| {
        let cut: Vec<String> = c
            .cut
            .iter()
            .map(|e| format!("{} -> {}", short(&e.from), short(&e.to)))
            .collect();
        out.push(Finding {
            kind: FindingKind::Cycle,
            subject: c.nodes.first().map(ToString::to_string).unwrap_or_default(),
            nodes: c.nodes.clone(),
            score,
            evidence: BTreeMap::from([
                ("size".to_owned(), f64::from(c.size)),
                ("cut".to_owned(), c.cut.len() as f64),
            ]),
            reason: format!(
                "{} {what} depend on each other in a cycle; removing {} would break it.",
                c.size,
                if cut.is_empty() {
                    "an edge".to_owned()
                } else {
                    cut.join(", ")
                }
            ),
        });
    };
    for c in inp.unit_cycles {
        add(c, "units", 1.0);
    }
    for c in inp.namespace_cycles {
        add(c, "namespaces", 0.95);
    }
    for c in inp
        .first
        .components
        .iter()
        .filter(|c| c.size >= rules.cycle.symbol_min_size)
    {
        add(c, "functions", (0.5 + f64::from(c.size) / 40.0).min(0.9));
    }
    out
}

/// A short name for an ID in a one-line reason: the part after `//`.
///
/// Example: `clj:.//shop.core/checkout` gives `shop.core/checkout`.
fn short(id: &NodeId) -> String {
    let s = id.to_string();
    s.split_once("//")
        .map_or(s.clone(), |(_, rest)| rest.to_owned())
}

/// Hidden coupling: co-change with no static path, above the thresholds.
///
/// Example: a handler and a config file edited together in most commits.
fn hidden_coupling(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<Finding> {
    let h = &rules.hidden_coupling;
    let Some(churn) = inp.churn else {
        return vec![];
    };
    churn
        .co_changes
        .iter()
        .filter(|c| c.hidden && c.support >= h.min_support && c.confidence_a_b.max(c.confidence_b_a) >= h.min_confidence)
        .map(|c| {
            let conf = c.confidence_a_b.max(c.confidence_b_a);
            Finding {
                kind: FindingKind::HiddenCoupling,
                subject: format!("{} <-> {}", c.a, c.b),
                nodes: vec![],
                score: conf * 0.9,
                evidence: BTreeMap::from([
                    ("support".to_owned(), f64::from(c.support)),
                    ("confidence_a_b".to_owned(), c.confidence_a_b),
                    ("confidence_b_a".to_owned(), c.confidence_b_a),
                ]),
                reason: format!(
                    "{} and {} changed together in {} commits but no call or require joins them within two hops; make the dependency explicit or decouple them.",
                    c.a, c.b, c.support
                ),
            }
        })
        .collect()
}

/// Risky and untested: high risk with an untested or weak estimate. Dead
/// code is left out (it is its own finding).
///
/// Example: a much-called function no test reaches.
fn risky_untested(
    risk: &[RiskRow],
    dead: &std::collections::HashSet<&NodeId>,
    rules: &HotspotRules,
) -> Vec<Finding> {
    let r = &rules.risky_untested;
    let values: Vec<f64> = risk.iter().map(|x| x.risk).collect();
    let ranks = percentile_ranks(&values);
    top(&values, r.risk_percentile)
        .iter()
        .zip(risk)
        .enumerate()
        .filter(|(_, (hit, row))| {
            let status = serde_json::to_value(row.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned));
            **hit && !dead.contains(&row.id) && status.is_some_and(|s| r.statuses.contains(&s))
        })
        .map(|(i, (_, row))| {
            let status = serde_json::to_value(row.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            single(
                FindingKind::RiskyUntested,
                &row.id,
                ranks[i],
                [
                    ("risk", row.risk),
                    ("importance", row.importance),
                    ("exposure", row.exposure),
                ],
                format!(
                    "{} is heavily used (importance {:.2}) but {}; add a test that calls it.",
                    short(&row.id),
                    row.importance,
                    if status == "weak" {
                        "tests reach it only through uncertain calls"
                    } else {
                        "no test reaches it"
                    }
                ),
            )
        })
        .collect()
}

/// Misplaced code: a symbol whose community mostly lives in one other
/// namespace.
///
/// Example: a formatting helper in `app.db` used only by `app.ui` code.
fn misplaced(inp: &Inputs<'_>, rules: &HotspotRules) -> Vec<Finding> {
    let m = &rules.misplaced_code;
    let ns_of = |id: &NodeId| inp.graph.nodes.get(id).and_then(|n| n.namespace.clone());
    let mut by_comm: HashMap<u32, BTreeMap<String, u32>> = HashMap::new();
    for n in &inp.second.nodes {
        if let Some(ns) = ns_of(&n.id) {
            *by_comm
                .entry(n.community)
                .or_default()
                .entry(ns)
                .or_insert(0) += 1;
        }
    }
    inp.second
        .nodes
        .iter()
        .filter_map(|n| {
            let own = ns_of(&n.id)?;
            let counts = by_comm.get(&n.community)?;
            let size: u32 = counts.values().sum();
            if size < m.min_community {
                return None;
            }
            let (other, count) = counts
                .iter()
                .filter(|(ns, _)| **ns != own)
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))?;
            let share = f64::from(*count) / f64::from(size);
            (share >= m.min_share).then(|| {
                single(
                    FindingKind::MisplacedCode,
                    &n.id,
                    share * 0.7,
                    [("share", share), ("community_size", f64::from(size))],
                    format!(
                        "{}: {:.0}% of the code it works with is in {other}; consider moving it there.",
                        label(inp.graph, &n.id),
                        share * 100.0
                    ),
                )
            })
        })
        .collect()
}

/// Dead code: one finding per unreachable symbol.
///
/// Example: `shop.plugins/run-script`, which nothing calls.
fn dead_code(inp: &Inputs<'_>) -> Vec<Finding> {
    inp.dead
        .dead
        .iter()
        .map(|d| {
            single(
                FindingKind::DeadCode,
                &d.id,
                0.5,
                [],
                format!(
                    "{}: nothing reaches it from an entry point, a test or load-time code, even counting uncertain calls; delete it or find its caller.",
                    label(inp.graph, &d.id)
                ),
            )
        })
        .collect()
}

/// Everything `hotspots` needs, computed (or read from `store`) for a
/// graph: symbol-level invocation measures, coverage, dead code, and
/// namespace and unit cycles over `requires` and `depends`. Returns the
/// report and any warnings from keeping results.
///
/// Example: `from_graph(&graph, &HotspotRules::builtin()?, "builtin", None, &MemoryStore::default())`.
pub fn from_graph(
    graph: &Graph,
    rules: &HotspotRules,
    rules_source: &str,
    churn: Option<&ChurnReport>,
    store: &dyn crate::measures::cache::ResultStore,
) -> (HotspotsReport, Vec<String>) {
    use crate::measures::cache;
    use aldente_model::AggregationLevel;
    use aldente_model::measures::{ProjectionKind, ProjectionSpec};
    let mut warnings = Vec::new();
    let mut keep = |w: Option<String>| warnings.extend(w);
    let symbol = ProjectionSpec::default();
    let empty_first = |spec: &ProjectionSpec| MeasureReport {
        projection: spec.clone(),
        nodes: vec![],
        components: vec![],
        convergence: BTreeMap::new(),
    };
    let (first, w) =
        cache::measures(graph, &symbol, store).unwrap_or_else(|_| (empty_first(&symbol), None));
    keep(w);
    let second = match cache::second(
        graph,
        &symbol,
        crate::measures::SecondOptions::default(),
        store,
    ) {
        Ok((s, w)) => {
            keep(w);
            s
        }
        Err(_) => crate::measures::second_set_on(
            graph,
            &crate::measures::Projection::from_arcs(vec![], &[]),
            symbol.clone(),
            crate::measures::SecondOptions::default(),
        ),
    };
    let structure = |level| ProjectionSpec {
        kind: ProjectionKind::Structure,
        level,
        ..ProjectionSpec::default()
    };
    let mut cycles_at = |level| {
        let spec = structure(level);
        match cache::measures(graph, &spec, store) {
            Ok((r, w)) => {
                keep(w);
                r.components
            }
            Err(_) => vec![],
        }
    };
    let namespace_cycles = cycles_at(AggregationLevel::Namespace);
    let unit_cycles = cycles_at(AggregationLevel::Unit);
    let coverage = crate::coverage::coverage(graph, crate::coverage::DEFAULT_DEPTH, None);
    let (dead, w) = cache::dead_code(graph, store);
    warnings.extend(w);
    let inputs = Inputs {
        graph,
        first: &first,
        second: &second,
        coverage: &coverage,
        dead: &dead,
        namespace_cycles: &namespace_cycles,
        unit_cycles: &unit_cycles,
        churn,
    };
    (hotspots(&inputs, rules, rules_source), warnings)
}
