//! Graph measures on projections of the graph.
//! See "Network analysis and coverage" in `docs/plan/09-network-and-coverage.md`.

pub mod cache;
pub mod fan;
pub mod hits;
pub mod kcore;
pub mod pagerank;
pub mod projection;
pub mod scc;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use aldente_graph::Graph;
use aldente_model::measures::{Component, CutEdge, MeasureReport, NodeMeasures, ProjectionSpec};

pub use projection::{Projection, ProjectionError, project};

/// The first set of measures (fan, PageRank both ways, HITS, SCC with cut
/// suggestions, k-core) on the projection `spec` describes.
///
/// Example: for clj-mini with the default spec, `nodes` has one row per
/// non-test function and impl, `pagerank` summing to 1.
pub fn first_set(graph: &Graph, spec: &ProjectionSpec) -> Result<MeasureReport, ProjectionError> {
    let p = project(graph, spec)?;
    Ok(first_set_on(&p, spec.clone()))
}

/// [`first_set`] on a projection already built.
///
/// Example: see [`first_set`].
pub fn first_set_on(p: &Projection, spec: ProjectionSpec) -> MeasureReport {
    let fans = fan::fan(p);
    let (pr, pr_conv) = pagerank::pagerank(p);
    let (prr, prr_conv) = pagerank::pagerank(&p.reversed());
    let (hub, auth, hits_conv) = hits::hits(p);
    let core = kcore::core_numbers(&p.undirected());
    let comps = scc::components(p);
    let mut component_of: Vec<Option<u32>> = vec![None; p.len()];
    let components = comps
        .iter()
        .enumerate()
        .map(|(ci, comp)| {
            for &v in comp {
                component_of[v as usize] = Some(ci as u32);
            }
            let (cut, minimal) = scc::cut(p, comp);
            Component {
                size: comp.len() as u32,
                nodes: comp.iter().map(|&v| p.id(v).clone()).collect(),
                cut: cut
                    .into_iter()
                    .map(|(f, t, a)| CutEdge {
                        from: p.id(f).clone(),
                        to: p.id(t).clone(),
                        weight: a.weight,
                        count: a.count,
                    })
                    .collect(),
                cut_minimal: minimal,
            }
        })
        .collect();
    let nodes = (0..p.len())
        .map(|i| NodeMeasures {
            id: p.id(i as u32).clone(),
            fan_in: fans[i].fan_in,
            fan_out: fans[i].fan_out,
            fan_in_weighted: fans[i].fan_in_weighted,
            fan_out_weighted: fans[i].fan_out_weighted,
            pagerank: pr[i],
            pagerank_reverse: prr[i],
            hub: hub[i],
            authority: auth[i],
            core: core[i],
            component: component_of[i],
        })
        .collect();
    let convergence = BTreeMap::from([
        ("pagerank".to_owned(), pr_conv),
        ("pagerank_reverse".to_owned(), prr_conv),
        ("hits".to_owned(), hits_conv),
    ]);
    MeasureReport {
        projection: spec,
        nodes,
        components,
        convergence,
    }
}
