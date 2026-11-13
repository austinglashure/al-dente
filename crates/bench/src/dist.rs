//! Small sampling helpers on top of the seeded generator: heavy-tailed
//! weights, splitting a total into buckets, and weighted picks inside a
//! contiguous range.

use aldente_analysis::measures::paths::Rng;

/// A float in `[0, 1)`.
///
/// Example: `unit(&mut Rng::new(1))` is something like `0.4172…`, the same on every run.
pub fn unit(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

/// A Pareto-distributed weight with shape `alpha` (smaller is heavier
/// tailed), at least 1 and at most `cap`.
///
/// Example: with `alpha = 1.5` most weights are near 1, a few are in the hundreds.
pub fn pareto(rng: &mut Rng, alpha: f64, cap: f64) -> f64 {
    // 1 - u is in (0, 1], so the power is finite.
    (1.0 - unit(rng)).powf(-1.0 / alpha).min(cap)
}

/// Splits `total` into `weights.len()` integer parts proportional to the
/// weights, each at least `min` (largest remainder method). Fails when
/// `total` cannot give every part its minimum.
///
/// Example: `split(10, &[1.0, 1.0, 2.0], 1)` is `Ok(vec![2, 3, 5])` or similar; the sum is always 10.
pub fn split(total: usize, weights: &[f64], min: usize) -> anyhow::Result<Vec<usize>> {
    let floor = weights.len() * min;
    anyhow::ensure!(
        total >= floor,
        "cannot split {total} into {} parts of at least {min}",
        weights.len()
    );
    let spare = (total - floor) as f64;
    let sum: f64 = weights.iter().sum();
    let shares: Vec<f64> = weights.iter().map(|w| spare * w / sum).collect();
    let mut parts: Vec<usize> = shares.iter().map(|s| min + *s as usize).collect();
    let mut left = total - parts.iter().sum::<usize>();
    let mut order: Vec<usize> = (0..weights.len()).collect();
    // Biggest fractional part first; index breaks ties so the result is stable.
    order.sort_by(|&a, &b| {
        let (fa, fb) = (shares[a].fract(), shares[b].fract());
        fb.total_cmp(&fa).then(a.cmp(&b))
    });
    for i in order.into_iter().cycle() {
        if left == 0 {
            break;
        }
        parts[i] += 1;
        left -= 1;
    }
    Ok(parts)
}

/// Prefix sums over weights, for weighted picks of an index inside any
/// contiguous range in `O(log n)`.
#[derive(Debug, Clone)]
pub struct Cumulative {
    /// `sums[i]` = weight of items `0..i`; one longer than the items.
    sums: Vec<f64>,
}

impl Cumulative {
    /// Prefix sums of `weights`.
    ///
    /// Example: `Cumulative::new(&[1.0, 3.0]).pick(&mut rng, 0..2)` is 1 three times in four.
    pub fn new(weights: &[f64]) -> Cumulative {
        let mut sums = Vec::with_capacity(weights.len() + 1);
        let mut acc = 0.0;
        sums.push(acc);
        for w in weights {
            acc += w;
            sums.push(acc);
        }
        Cumulative { sums }
    }

    /// An index in `range`, chosen with probability proportional to its weight.
    ///
    /// Example: `c.pick(&mut rng, 5..9)` is one of 5, 6, 7, 8.
    pub fn pick(&self, rng: &mut Rng, range: std::ops::Range<usize>) -> usize {
        let (lo, hi) = (self.sums[range.start], self.sums[range.end]);
        let target = lo + unit(rng) * (hi - lo);
        // First prefix strictly above the target, minus one, is the item.
        let i = self.sums[range.start..=range.end].partition_point(|s| *s <= target);
        (range.start + i.saturating_sub(1)).min(range.end - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sums_to_total_and_respects_minimum() {
        let parts = split(100, &[1.0, 10.0, 0.001, 5.0], 2).unwrap();
        assert_eq!(parts.iter().sum::<usize>(), 100);
        assert!(parts.iter().all(|p| *p >= 2));
        assert!(parts[1] > parts[3] && parts[3] > parts[0]);
        assert!(split(3, &[1.0, 1.0], 2).is_err());
    }

    #[test]
    fn pick_stays_in_range_and_follows_weights() {
        let c = Cumulative::new(&[1.0, 1.0, 1.0, 9.0, 0.0, 5.0]);
        let mut rng = Rng::new(7);
        let mut seen = [0usize; 6];
        for _ in 0..10_000 {
            seen[c.pick(&mut rng, 2..6)] += 1;
        }
        assert_eq!(seen[0] + seen[1], 0);
        assert_eq!(seen[4], 0, "zero weight is never picked");
        assert!(seen[3] > seen[5] && seen[5] > seen[2]);
    }

    #[test]
    fn pareto_is_heavy_tailed_and_capped() {
        let mut rng = Rng::new(3);
        let ws: Vec<f64> = (0..10_000).map(|_| pareto(&mut rng, 1.2, 500.0)).collect();
        assert!(ws.iter().all(|w| (1.0..=500.0).contains(w)));
        let big = ws.iter().filter(|w| **w > 50.0).count();
        assert!(big > 10 && big < 500, "{big}");
    }
}
