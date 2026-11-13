//! Fan-in and fan-out, raw (distinct neighbors) and weighted.

use super::projection::{Ix, Projection};

/// Fan values of one node.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fan {
    pub fan_in: u32,
    pub fan_out: u32,
    pub fan_in_weighted: f64,
    pub fan_out_weighted: f64,
}

/// Fan-in and fan-out of every node.
///
/// Example: for `a → b` (weight 0.6), `b` has `fan_in == 1` and
/// `fan_in_weighted == 0.6`; `a` has `fan_out == 1`.
pub fn fan(p: &Projection) -> Vec<Fan> {
    (0..p.len() as Ix)
        .map(|i| Fan {
            fan_in: p.incoming(i).len() as u32,
            fan_out: p.outgoing(i).len() as u32,
            fan_in_weighted: p.incoming(i).iter().map(|a| a.weight).sum(),
            fan_out_weighted: p.outgoing(i).iter().map(|a| a.weight).sum(),
        })
        .collect()
}
