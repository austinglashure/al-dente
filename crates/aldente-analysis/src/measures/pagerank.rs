//! Weighted PageRank with certainty leak.
//!
//! A walker at `u` follows arc `u → v` with probability `w / D(u)`, where
//! `D(u)` is the sum of `u`'s arc counts (at symbol level, its number of
//! callees). A certain arc (weight 1 per underlying edge) passes on its
//! full share; a speculative one passes on only its weight's share and the
//! rest goes back to the uniform jump. So lowering an edge from `certain`
//! to `possible` lowers its target's rank instead of being normalized away.
//! With every arc certain this is standard PageRank (networkx defaults:
//! uniform jump, dangling mass spread uniformly).

use aldente_model::measures::Convergence;

use super::projection::{Ix, Projection};

/// Damping factor.
pub const DAMPING: f64 = 0.85;
const TOLERANCE: f64 = 1e-12;
const MAX_ITERATIONS: u32 = 1000;

/// PageRank of every node (sums to 1) and how the iteration ended.
///
/// Example: on `a → b`, `b` ranks above `a`; on an empty projection the
/// result is empty and converged.
pub fn pagerank(p: &Projection) -> (Vec<f64>, Convergence) {
    let n = p.len();
    if n == 0 {
        return (
            vec![],
            Convergence {
                iterations: 0,
                converged: true,
                delta: 0.0,
            },
        );
    }
    let nf = n as f64;
    // Per-node denominators; zero means dangling.
    let denom: Vec<f64> = (0..n as Ix)
        .map(|i| p.outgoing(i).iter().map(|a| f64::from(a.count)).sum())
        .collect();
    let mut x = vec![1.0 / nf; n];
    let mut next = vec![0.0; n];
    let mut delta = f64::INFINITY;
    let mut iterations = 0;
    while iterations < MAX_ITERATIONS {
        iterations += 1;
        next.iter_mut().for_each(|v| *v = 0.0);
        let mut leaked = 0.0;
        for u in 0..n {
            if denom[u] == 0.0 {
                leaked += x[u];
                continue;
            }
            let mut passed = 0.0;
            for a in p.outgoing(u as Ix) {
                let share = x[u] * a.weight / denom[u];
                next[a.other as usize] += share;
                passed += share;
            }
            leaked += (x[u] - passed).max(0.0);
        }
        let base = (1.0 - DAMPING) / nf + DAMPING * leaked / nf;
        delta = 0.0;
        for v in 0..n {
            let val = base + DAMPING * next[v];
            delta += (val - x[v]).abs();
            x[v] = val;
        }
        if delta < TOLERANCE {
            break;
        }
    }
    (
        x,
        Convergence {
            iterations,
            converged: delta < TOLERANCE,
            delta,
        },
    )
}
