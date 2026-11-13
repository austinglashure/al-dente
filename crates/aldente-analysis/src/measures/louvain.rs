//! Louvain community detection on the undirected weighted projection
//! (an edge's weight is the sum of both directions). Deterministic for a
//! fixed seed: nodes are visited in a seeded order, ties go to the smaller
//! community.

use std::collections::BTreeMap;

use super::paths::Rng;
use super::projection::Projection;

/// Default seed of the community search.
pub const DEFAULT_SEED: u64 = 42;
/// Smallest modularity gain that counts as an improvement.
const MIN_GAIN: f64 = 1e-12;
/// A level stops when a whole pass gains less modularity than this.
const MIN_PASS_GAIN: f64 = 1e-5;
/// Most passes per level.
const MAX_PASSES: usize = 25;

/// An undirected weighted graph: `adj[v]` lists `(neighbor, weight)`,
/// self-loops allowed (they appear once), plus each node's weighted degree.
#[derive(Debug, Clone)]
struct Level {
    adj: Vec<Vec<(usize, f64)>>,
    /// Self-loop weight per node.
    loops: Vec<f64>,
}

impl Level {
    /// Weighted degree of `v` (a loop counts twice).
    ///
    /// Example: a node with one edge of weight 2 has degree 2.
    fn degree(&self, v: usize) -> f64 {
        self.adj[v].iter().map(|(_, w)| w).sum::<f64>() + 2.0 * self.loops[v]
    }
}

/// `(community of each node, modularity)`, communities numbered by their
/// smallest member.
///
/// Example: two triangles joined by one edge give two communities.
pub fn louvain(p: &Projection, seed: u64) -> (Vec<u32>, f64) {
    let n = p.len();
    let mut level = undirected(p);
    let total: f64 = (0..n).map(|v| level.degree(v)).sum::<f64>() / 2.0;
    // Original node → current super-node.
    let mut member: Vec<usize> = (0..n).collect();
    if total <= 0.0 {
        return (renumber(&member), 0.0);
    }
    let mut rng = Rng::new(seed);
    loop {
        let (comm, moved) = one_level(&level, total, &mut rng);
        if !moved {
            break;
        }
        let (next, map) = aggregate(&level, &comm);
        for m in member.iter_mut() {
            *m = map[comm[*m]];
        }
        level = next;
    }
    let comm = renumber(&member);
    let q = modularity(&undirected(p), &comm, total);
    (comm, q)
}

/// The undirected weighted form of `p`.
///
/// Example: arcs `a → b` (1.0) and `b → a` (0.5) give one edge of 1.5.
fn undirected(p: &Projection) -> Level {
    let n = p.len();
    let mut acc: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); n];
    for v in 0..n {
        for a in p.outgoing(v as u32) {
            let w = a.other as usize;
            *acc[v].entry(w).or_insert(0.0) += a.weight;
            *acc[w].entry(v).or_insert(0.0) += a.weight;
        }
    }
    Level {
        adj: acc.into_iter().map(|m| m.into_iter().collect()).collect(),
        loops: vec![0.0; n],
    }
}

/// Local moves, pass after pass, until a pass improves modularity by less
/// than `MIN_PASS_GAIN` (or nothing moves). Returns each node's community
/// and whether anything moved at all.
///
/// Example: on two disconnected edges, the pairs join.
fn one_level(g: &Level, m: f64, rng: &mut Rng) -> (Vec<usize>, bool) {
    let n = g.adj.len();
    let mut comm: Vec<usize> = (0..n).collect();
    let deg: Vec<f64> = (0..n).map(|v| g.degree(v)).collect();
    let mut tot: Vec<f64> = deg.clone();
    let mut order: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        order.swap(i, rng.below(i + 1));
    }
    // Weight from the current node to each community, and which entries
    // are set (reset after each node).
    let mut links = vec![0.0f64; n];
    let mut touched: Vec<usize> = Vec::new();
    let mut moved_any = false;
    for _ in 0..MAX_PASSES {
        let mut moved = false;
        let mut pass_gain = 0.0;
        for &v in &order {
            let own = comm[v];
            for &(w, wt) in &g.adj[v] {
                if w != v {
                    let c = comm[w];
                    if links[c] == 0.0 {
                        touched.push(c);
                    }
                    links[c] += wt;
                }
            }
            tot[own] -= deg[v];
            let gain = |c: usize, k_in: f64| k_in - tot[c] * deg[v] / (2.0 * m);
            let stay = gain(own, links[own]);
            let (mut best, mut best_gain) = (own, stay);
            for &c in &touched {
                let g = gain(c, links[c]);
                let better = g > best_gain + MIN_GAIN;
                let tie = (g - best_gain).abs() <= MIN_GAIN && c < best;
                if better || tie {
                    best = c;
                    best_gain = g;
                }
            }
            for &c in &touched {
                links[c] = 0.0;
            }
            touched.clear();
            tot[best] += deg[v];
            if best != own {
                comm[v] = best;
                moved = true;
                moved_any = true;
                pass_gain += (best_gain - stay) / m;
            }
        }
        if !moved || pass_gain < MIN_PASS_GAIN {
            break;
        }
    }
    (comm, moved_any)
}

/// Collapses communities into super-nodes. Returns the new level and the
/// community → super-node map.
///
/// Example: two communities give a two-node level.
fn aggregate(g: &Level, comm: &[usize]) -> (Level, Vec<usize>) {
    let mut map = vec![usize::MAX; comm.len()];
    let mut next = 0;
    for &c in comm {
        if map[c] == usize::MAX {
            map[c] = next;
            next += 1;
        }
    }
    let mut acc: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); next];
    let mut loops = vec![0.0; next];
    for v in 0..g.adj.len() {
        let cv = map[comm[v]];
        loops[cv] += g.loops[v];
        for &(w, wt) in &g.adj[v] {
            let cw = map[comm[w]];
            if cv == cw {
                // Each internal edge is seen from both ends.
                loops[cv] += wt / 2.0;
            } else {
                *acc[cv].entry(cw).or_insert(0.0) += wt;
            }
        }
    }
    (
        Level {
            adj: acc.into_iter().map(|m| m.into_iter().collect()).collect(),
            loops,
        },
        map,
    )
}

/// Newman's modularity of a partition of the base graph.
///
/// Example: a single community has modularity 0.
fn modularity(g: &Level, comm: &[u32], m: f64) -> f64 {
    let k = comm.iter().map(|&c| c as usize + 1).max().unwrap_or(0);
    let mut inside = vec![0.0; k];
    let mut tot = vec![0.0; k];
    for v in 0..g.adj.len() {
        let c = comm[v] as usize;
        tot[c] += g.degree(v);
        for &(w, wt) in &g.adj[v] {
            if comm[w] as usize == c {
                inside[c] += wt;
            }
        }
    }
    (0..k)
        .map(|c| inside[c] / (2.0 * m) - (tot[c] / (2.0 * m)).powi(2))
        .sum()
}

/// Community labels numbered in order of their smallest member.
///
/// Example: `[5, 5, 2]` becomes `[0, 0, 1]`.
fn renumber(member: &[usize]) -> Vec<u32> {
    let mut map: BTreeMap<usize, u32> = BTreeMap::new();
    member
        .iter()
        .map(|&m| {
            let next = map.len() as u32;
            *map.entry(m).or_insert(next)
        })
        .collect()
}
