//! Graph measures on projections of the graph.
//! See "Network analysis and coverage" in `docs/plan/09-network-and-coverage.md`.

pub mod cache;
pub mod coupling;
pub mod cuts;
pub mod deadcode;
pub mod dominators;
pub mod eigen;
pub mod fan;
pub mod god;
pub mod hits;
pub mod kcore;
pub mod louvain;
pub mod pagerank;
pub mod paths;
pub mod projection;
pub mod scc;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests2;

use std::collections::{BTreeMap, BTreeSet};

use aldente_graph::Graph;
use aldente_model::measures::{
    Bridge, Community, Component, CutEdge, MeasureReport, NodeMeasures, NodeMeasures2,
    ProjectionSpec, SecondReport, SplitNamespace,
};
use aldente_model::{NodeId, SymbolFlag};

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

/// Settings of the second set that are not part of the projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SecondOptions {
    /// Sources for betweenness and closeness above `paths::EXACT_UP_TO` nodes.
    pub pivots: usize,
    pub pivot_seed: u64,
    pub community_seed: u64,
}

impl Default for SecondOptions {
    fn default() -> Self {
        SecondOptions {
            pivots: paths::DEFAULT_PIVOTS,
            pivot_seed: paths::DEFAULT_SEED,
            community_seed: louvain::DEFAULT_SEED,
        }
    }
}

/// The second set of measures (betweenness, harmonic closeness,
/// eigenvector, communities, dominators, articulation points and bridges,
/// god-object score) on the projection `spec` describes.
///
/// Example: for clj-mini, `communities` groups the pricing code apart from
/// the UI code.
pub fn second_set(
    graph: &Graph,
    spec: &ProjectionSpec,
    opts: SecondOptions,
) -> Result<SecondReport, ProjectionError> {
    let p = project(graph, spec)?;
    Ok(second_set_on(graph, &p, spec.clone(), opts))
}

/// [`second_set`] on a projection already built.
///
/// Example: see [`second_set`].
pub fn second_set_on(
    graph: &Graph,
    p: &Projection,
    spec: ProjectionSpec,
    opts: SecondOptions,
) -> SecondReport {
    let n = p.len();
    let sampling = paths::sampling_for(n, opts.pivots, opts.pivot_seed);
    let (between, close) = paths::betweenness_closeness(p, sampling);
    let comps = scc::components(p);
    let (eigen, eigen_conv) = eigen::eigenvector(p, &comps);
    let (community, modularity) = louvain::louvain(p, opts.community_seed);
    let entries: Vec<u32> = (0..n as u32)
        .filter(|&i| {
            graph
                .nodes
                .get(p.id(i))
                .is_some_and(|nd| nd.flags.contains(&SymbolFlag::EntryPoint))
        })
        .collect();
    let dominates = dominators::dominated_counts(p, &entries);
    let undirected = p.undirected();
    let (articulation, bridges) = cuts::articulation_and_bridges(&undirected);
    let (hub, _, _) = hits::hits(p);
    let core = kcore::core_numbers(&undirected);
    let god = god::god_scores(graph, p, &hub, &core);
    let (communities, split_namespaces) = describe_communities(graph, p, &community);
    let nodes = (0..n)
        .map(|i| NodeMeasures2 {
            id: p.id(i as u32).clone(),
            betweenness: between[i],
            closeness: close[i],
            eigenvector: eigen[i],
            community: community[i],
            dominates: dominates[i],
            articulation: articulation[i],
            god_score: god[i],
        })
        .collect();
    SecondReport {
        projection: spec,
        nodes,
        sampling,
        communities,
        modularity,
        community_seed: opts.community_seed,
        split_namespaces,
        bridges: bridges
            .into_iter()
            .map(|(a, b)| Bridge {
                a: p.id(a).clone(),
                b: p.id(b).clone(),
            })
            .collect(),
        entry_points: entries.len() as u32,
        eigenvector_convergence: eigen_conv,
    }
}

/// Each community's namespaces and units, and the namespaces split across
/// communities.
///
/// Example: a namespace whose four functions fall two and two into
/// communities 0 and 1 is split.
fn describe_communities(
    graph: &Graph,
    p: &Projection,
    community: &[u32],
) -> (Vec<Community>, Vec<SplitNamespace>) {
    let k = community.iter().map(|&c| c as usize + 1).max().unwrap_or(0);
    let mut namespaces: Vec<BTreeSet<String>> = vec![BTreeSet::new(); k];
    let mut units: Vec<BTreeSet<NodeId>> = vec![BTreeSet::new(); k];
    let mut size = vec![0u32; k];
    let mut by_ns: BTreeMap<String, BTreeMap<u32, u32>> = BTreeMap::new();
    for (i, &c) in community.iter().enumerate() {
        size[c as usize] += 1;
        let node = graph.nodes.get(p.id(i as u32));
        if let Some(ns) = node.and_then(|nd| nd.namespace.clone()) {
            namespaces[c as usize].insert(ns.clone());
            *by_ns.entry(ns).or_default().entry(c).or_insert(0) += 1;
        }
        if let Some(u) = graph.unit_of(p.id(i as u32)) {
            units[c as usize].insert(u);
        }
    }
    let communities = (0..k)
        .map(|c| Community {
            size: size[c],
            namespaces: namespaces[c].iter().cloned().collect(),
            units: units[c].iter().cloned().collect(),
        })
        .collect();
    let split = by_ns
        .into_iter()
        .filter(|(_, cs)| cs.len() > 1)
        .map(|(namespace, communities)| SplitNamespace {
            namespace,
            communities,
        })
        .collect();
    (communities, split)
}
