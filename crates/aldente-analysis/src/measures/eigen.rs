//! Eigenvector centrality per strongly connected component.
//!
//! Outside a cycle the directed eigenvector is zero (or undefined), so the
//! power iteration runs inside each component of more than one node, on
//! `x ← x + A·x` (the shift networkx uses, which avoids oscillation on
//! periodic components). Non-convergence is reported, not hidden.

use std::collections::HashMap;

use aldente_model::measures::Convergence;

use super::projection::{Ix, Projection};

const TOLERANCE: f64 = 1e-10;
const MAX_ITERATIONS: u32 = 1000;

/// Centrality of every node (0 outside the given components) and each
/// component's convergence.
///
/// Example: in a 3-cycle each node gets `1/√3`.
pub fn eigenvector(p: &Projection, comps: &[Vec<Ix>]) -> (Vec<f64>, Vec<Convergence>) {
    let mut out = vec![0.0; p.len()];
    let mut conv = Vec::with_capacity(comps.len());
    for comp in comps {
        let local: HashMap<Ix, usize> = comp.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        let m = comp.len();
        let mut x = vec![1.0 / m as f64; m];
        let mut iterations = 0;
        let mut delta = f64::INFINITY;
        while iterations < MAX_ITERATIONS {
            iterations += 1;
            let mut next = x.clone();
            for (i, &v) in comp.iter().enumerate() {
                for a in p.incoming(v) {
                    if let Some(&j) = local.get(&a.other) {
                        next[i] += a.weight * x[j];
                    }
                }
            }
            let norm = next.iter().map(|v| v * v).sum::<f64>().sqrt();
            if norm > 0.0 {
                next.iter_mut().for_each(|v| *v /= norm);
            }
            delta = next.iter().zip(&x).map(|(a, b)| (a - b).abs()).sum();
            x = next;
            if delta < TOLERANCE * m as f64 {
                break;
            }
        }
        for (i, &v) in comp.iter().enumerate() {
            out[v as usize] = x[i];
        }
        conv.push(Convergence {
            iterations,
            converged: delta < TOLERANCE * m as f64,
            delta,
        });
    }
    (out, conv)
}
