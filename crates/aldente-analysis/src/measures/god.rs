//! The god-object score: the mean of five percentile ranks within the
//! projection (fan-out, hub score, body size in lines, core number,
//! distinct namespaces called). 1.0 is the top on every count.

use std::collections::BTreeSet;

use aldente_graph::Graph;

use super::projection::{Ix, Projection};

/// Percentile rank of each value: the share of values strictly below it,
/// ties sharing the same rank.
///
/// Example: `percentile_ranks(&[1.0, 2.0, 2.0, 3.0])` is `[0.0, 0.25, 0.25, 0.75]`.
pub fn percentile_ranks(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    if n == 0 {
        return vec![];
    }
    let mut sorted: Vec<f64> = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    values
        .iter()
        .map(|v| sorted.partition_point(|x| x < v) as f64 / n as f64)
        .collect()
}

/// God-object scores, given fan-out, hub and core values per node.
///
/// Example: a node with the largest value on all five counts scores
/// `(n−1)/n`.
pub fn god_scores(graph: &Graph, p: &Projection, hub: &[f64], core: &[u32]) -> Vec<f64> {
    let n = p.len();
    let fan_out: Vec<f64> = (0..n as Ix).map(|i| p.outgoing(i).len() as f64).collect();
    let body: Vec<f64> = (0..n as Ix)
        .map(|i| {
            graph
                .nodes
                .get(p.id(i))
                .and_then(|nd| nd.loc.as_ref())
                .map_or(0.0, |l| f64::from(l.end_line.saturating_sub(l.line) + 1))
        })
        .collect();
    let touched: Vec<f64> = (0..n as Ix)
        .map(|i| {
            p.outgoing(i)
                .iter()
                .filter_map(|a| graph.namespace_of(p.id(a.other)))
                .collect::<BTreeSet<_>>()
                .len() as f64
        })
        .collect();
    let core: Vec<f64> = core.iter().map(|&c| f64::from(c)).collect();
    let ranks = [
        percentile_ranks(&fan_out),
        percentile_ranks(hub),
        percentile_ranks(&body),
        percentile_ranks(&core),
        percentile_ranks(&touched),
    ];
    (0..n)
        .map(|i| ranks.iter().map(|r| r[i]).sum::<f64>() / ranks.len() as f64)
        .collect()
}
