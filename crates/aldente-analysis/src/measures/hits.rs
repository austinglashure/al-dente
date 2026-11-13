//! HITS hubs and authorities on the weighted projection, power iteration,
//! each vector normalized to sum 1 (as networkx reports them).

use aldente_model::measures::Convergence;

use super::projection::{Ix, Projection};

const TOLERANCE: f64 = 1e-12;
const MAX_ITERATIONS: u32 = 10_000;

/// `(hubs, authorities, convergence)`.
///
/// Example: on `a → b`, `a` is the hub (1.0) and `b` the authority (1.0).
pub fn hits(p: &Projection) -> (Vec<f64>, Vec<f64>, Convergence) {
    let n = p.len();
    if n == 0 {
        return (
            vec![],
            vec![],
            Convergence {
                iterations: 0,
                converged: true,
                delta: 0.0,
            },
        );
    }
    let mut hub = vec![1.0 / n as f64; n];
    let mut auth = vec![0.0; n];
    let mut delta = f64::INFINITY;
    let mut iterations = 0;
    while iterations < MAX_ITERATIONS {
        iterations += 1;
        for (v, slot) in auth.iter_mut().enumerate() {
            *slot = p
                .incoming(v as Ix)
                .iter()
                .map(|a| a.weight * hub[a.other as usize])
                .sum();
        }
        normalize(&mut auth);
        let mut next: Vec<f64> = (0..n)
            .map(|u| {
                p.outgoing(u as Ix)
                    .iter()
                    .map(|a| a.weight * auth[a.other as usize])
                    .sum()
            })
            .collect();
        normalize(&mut next);
        delta = next.iter().zip(&hub).map(|(a, b)| (a - b).abs()).sum();
        hub = next;
        if delta < TOLERANCE {
            break;
        }
    }
    (
        hub,
        auth,
        Convergence {
            iterations,
            converged: delta < TOLERANCE,
            delta,
        },
    )
}

/// Scales `v` to sum 1; an all-zero vector stays zero.
///
/// Example: `[1, 3]` becomes `[0.25, 0.75]`.
fn normalize(v: &mut [f64]) {
    let s: f64 = v.iter().sum();
    if s > 0.0 {
        v.iter_mut().for_each(|x| *x /= s);
    }
}
