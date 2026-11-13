//! Shortest-path measures over hop distances: Brandes betweenness and
//! harmonic closeness, exact or estimated from sampled sources.

use std::collections::VecDeque;

use aldente_model::measures::Sampling;

use super::projection::{Ix, Projection};

/// Above this many nodes, sources are sampled.
pub const EXACT_UP_TO: usize = 20_000;
/// Sources used when sampling.
pub const DEFAULT_PIVOTS: usize = 256;
/// Default seed of the pivot choice.
pub const DEFAULT_SEED: u64 = 0x5eed_0a1d_e17e;

/// A small deterministic generator (xorshift64*), so results do not depend
/// on a library's random number stream.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// A generator from `seed` (0 is replaced by a fixed constant).
    ///
    /// Example: `Rng::new(1).below(10)` is the same on every run.
    pub fn new(seed: u64) -> Rng {
        Rng(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    /// The next value.
    ///
    /// Example: two generators with one seed give the same sequence.
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// A value in `0..n` (`n > 0`).
    ///
    /// Example: `rng.below(3)` is 0, 1 or 2.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// `k` distinct sources out of `n`, chosen with `seed` (all of them, in
/// order, when `k >= n`).
///
/// Example: `pivots(5, 10, 1)` is `[0, 1, 2, 3, 4]`.
pub fn pivots(n: usize, k: usize, seed: u64) -> Vec<Ix> {
    let mut all: Vec<Ix> = (0..n as Ix).collect();
    if k >= n {
        return all;
    }
    // Partial Fisher–Yates.
    let mut rng = Rng::new(seed);
    for i in 0..k {
        let j = i + rng.below(n - i);
        all.swap(i, j);
    }
    all.truncate(k);
    all.sort_unstable();
    all
}

/// Betweenness (normalized by `(n−1)(n−2)`, as networkx does for directed
/// graphs) and harmonic closeness (sum over sources of 1/distance), from
/// one BFS per source. With sampled sources both are scaled by `n / k`.
///
/// Example: on `a → b → c`, `b` has betweenness `1 / 2` and `c` closeness
/// `1 + 1/2`.
pub fn betweenness_closeness(p: &Projection, sampling: Sampling) -> (Vec<f64>, Vec<f64>) {
    let n = p.len();
    let mut between = vec![0.0; n];
    let mut close = vec![0.0; n];
    if n == 0 {
        return (between, close);
    }
    let sources = if sampling.approximate {
        pivots(n, sampling.pivots as usize, sampling.seed)
    } else {
        (0..n as Ix).collect()
    };
    let mut dist = vec![u32::MAX; n];
    let mut sigma = vec![0.0f64; n];
    let mut delta = vec![0.0f64; n];
    let mut order: Vec<Ix> = Vec::with_capacity(n);
    let mut queue: VecDeque<Ix> = VecDeque::new();
    for &s in &sources {
        order.clear();
        dist[s as usize] = 0;
        sigma[s as usize] = 1.0;
        queue.push_back(s);
        while let Some(v) = queue.pop_front() {
            order.push(v);
            let dv = dist[v as usize];
            for a in p.outgoing(v) {
                let w = a.other as usize;
                if dist[w] == u32::MAX {
                    dist[w] = dv + 1;
                    queue.push_back(a.other);
                }
                if dist[w] == dv + 1 {
                    sigma[w] += sigma[v as usize];
                }
            }
        }
        // Accumulate dependencies in reverse BFS order; predecessors are the
        // in-neighbors one hop closer.
        for &w in order.iter().rev() {
            let wi = w as usize;
            let dw = dist[wi];
            if dw > 0 {
                close[wi] += 1.0 / f64::from(dw);
            }
            for a in p.incoming(w) {
                let v = a.other as usize;
                if dist[v] != u32::MAX && dist[v] + 1 == dw {
                    delta[v] += sigma[v] / sigma[wi] * (1.0 + delta[wi]);
                }
            }
            if w != s {
                between[wi] += delta[wi];
            }
        }
        for &v in &order {
            dist[v as usize] = u32::MAX;
            sigma[v as usize] = 0.0;
            delta[v as usize] = 0.0;
        }
    }
    let scale = n as f64 / sources.len() as f64;
    let norm = if n > 2 {
        1.0 / ((n - 1) as f64 * (n - 2) as f64)
    } else {
        0.0
    };
    between.iter_mut().for_each(|b| *b *= scale * norm);
    close.iter_mut().for_each(|c| *c *= scale);
    (between, close)
}

/// The sampling to use for a projection of `n` nodes.
///
/// Example: `sampling_for(100, DEFAULT_PIVOTS, DEFAULT_SEED).approximate` is false.
pub fn sampling_for(n: usize, pivots: usize, seed: u64) -> Sampling {
    let approximate = n > EXACT_UP_TO && pivots < n;
    Sampling {
        approximate,
        pivots: if approximate { pivots as u32 } else { n as u32 },
        seed,
    }
}
