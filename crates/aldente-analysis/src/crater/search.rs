//! One side (forward or reverse) of a crater: the three-pass search.

use std::collections::VecDeque;

use aldente_model::crater::Direction;
use aldente_model::{Certainty, EdgeKind, NodeKind};

use super::index::{CraterIndex, Ix};

/// How a node was first reached on one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label {
    pub distance: u32,
    pub certainty: Certainty,
    pub via: Option<Ix>,
}

/// The settings a side needs, already resolved to indexes and masks.
pub struct Rules<'a> {
    pub index: &'a CraterIndex,
    pub direction: Direction,
    pub depth: u32,
    pub min_certainty: Certainty,
    /// Bit `k` set when edge kind `k` (its position in `EdgeKind::ALL`) is followed.
    pub kind_mask: u32,
    pub seeds: &'a [Ix],
    pub is_seed: &'a [bool],
    /// Whether a non-seed node may be expanded past (hubs, stop patterns,
    /// externals and unit boundaries say no).
    pub expandable: &'a dyn Fn(Ix) -> bool,
    /// Whether a node is left out entirely (tests when excluded).
    pub excluded: &'a dyn Fn(Ix) -> bool,
    pub node_cap: usize,
}

/// The result of one side.
pub struct Side {
    pub labels: Vec<Option<Label>>,
    pub truncated: bool,
}

/// Visit state within one pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Seen {
    No,
    /// Reached only through `implements` in reverse: included, not expanded.
    Terminal,
    Expanded,
}

/// Bit for `kind` in a kind mask.
///
/// Example: `kind_bit(EdgeKind::Contains)` is `1`.
pub fn kind_bit(kind: EdgeKind) -> u32 {
    let pos = EdgeKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    1 << pos
}

/// Runs the certain, likely and possible passes in order. A node keeps the
/// label of the first pass that reached it, so a node with any certain path
/// is certain however short its weaker paths are. All three passes run even
/// when `min_certainty` is higher, so the caller can say how much is hidden;
/// the node cap applies only to passes at or above `min_certainty`.
///
/// Example: for `a -certain-> b -certain-> c` and `a -possible-> c`, `c` is
/// labeled `certain` at distance 2 via `b`.
pub fn search(r: &Rules<'_>) -> Side {
    let n = r.index.len();
    let mut side = Side {
        labels: vec![None; n],
        truncated: false,
    };
    let mut labeled_at_min = 0usize;
    for &level in [Certainty::Certain, Certainty::Likely, Certainty::Possible].iter() {
        let capped = level >= r.min_certainty;
        if pass(r, level, capped, &mut side, &mut labeled_at_min) {
            side.truncated = true;
            break;
        }
    }
    side
}

/// One breadth-first pass over edges at `level` or stronger. Returns `true`
/// when it stopped at the node cap; sets `side.truncated` when the depth limit
/// cut off nodes it could have reached.
///
/// Example: see [`search`].
fn pass(r: &Rules<'_>, level: Certainty, capped: bool, side: &mut Side, count: &mut usize) -> bool {
    let n = r.index.len();
    let mut seen = vec![Seen::No; n];
    let mut dist = vec![0u32; n];
    let mut queue = VecDeque::new();
    let mut frontier = Vec::new();
    for &s in r.seeds {
        if seen[s as usize] == Seen::Expanded {
            continue;
        }
        seen[s as usize] = Seen::Expanded;
        if side.labels[s as usize].is_none() {
            side.labels[s as usize] = Some(Label {
                distance: 0,
                certainty: Certainty::Certain,
                via: None,
            });
            *count += 1;
        }
        queue.push_back(s);
    }
    while let Some(u) = queue.pop_front() {
        if !r.is_seed[u as usize] && !(r.expandable)(u) {
            continue;
        }
        if r.depth > 0 && dist[u as usize] >= r.depth {
            frontier.push(u);
            continue;
        }
        let mut stop = false;
        for_each_arc(r, u, level, |v, terminal| {
            if stop || (r.excluded)(v) {
                return;
            }
            let want = if terminal {
                Seen::Terminal
            } else {
                Seen::Expanded
            };
            if seen[v as usize] >= want {
                return;
            }
            seen[v as usize] = want;
            dist[v as usize] = dist[u as usize] + 1;
            if side.labels[v as usize].is_none() {
                side.labels[v as usize] = Some(Label {
                    distance: dist[v as usize],
                    certainty: level,
                    via: Some(u),
                });
                if capped {
                    *count += 1;
                    if *count >= r.node_cap {
                        stop = true;
                    }
                }
            }
            if want == Seen::Expanded {
                queue.push_back(v);
            }
        });
        if stop {
            return true;
        }
    }
    if level >= r.min_certainty {
        for u in frontier {
            let mut cut = false;
            for_each_arc(r, u, level, |v, _| {
                cut |= seen[v as usize] == Seen::No && !(r.excluded)(v);
            });
            side.truncated |= cut;
        }
    }
    false
}

/// Calls `f(other, terminal)` for each arc the search may take from `u` at
/// `level`.
///
/// Forward: outgoing edges of followed kinds, and, from a seed only, its
/// impls (incoming `implements`). Reverse: incoming edges of followed kinds,
/// and outgoing `implements` as a terminal step, which is the mirror image
/// of the forward rule: an impl reached in reverse names the method a seed
/// there would have fanned out from, without continuing to that method's
/// callers (they reach the impl only through their own `dispatches`).
///
/// Example: forward from seed multimethod `m` with impl `m@a`, `f(m@a, false)`.
fn for_each_arc(r: &Rules<'_>, u: Ix, level: Certainty, mut f: impl FnMut(Ix, bool)) {
    let implements = kind_bit(EdgeKind::Implements);
    let ok = |edge: u32, mask: u32| {
        let info = r.index.edge(edge);
        kind_bit(info.kind) & mask != 0 && info.certainty >= level
    };
    match r.direction {
        Direction::Forward | Direction::Both => {
            for a in r.index.outgoing(u) {
                if ok(a.edge, r.kind_mask) {
                    f(a.other, false);
                }
            }
            if r.is_seed[u as usize] {
                for a in r.index.incoming(u) {
                    if ok(a.edge, implements) {
                        f(a.other, false);
                    }
                }
            }
        }
        Direction::Reverse => {
            for a in r.index.incoming(u) {
                if ok(a.edge, r.kind_mask) {
                    f(a.other, false);
                }
            }
            for a in r.index.outgoing(u) {
                if ok(a.edge, implements) {
                    f(a.other, true);
                }
            }
        }
    }
}

/// Whether a node kind marks the edge of what the analyzer can see.
///
/// Example: `is_boundary(NodeKind::Hole)` is `true`.
pub fn is_boundary(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::External | NodeKind::Platform | NodeKind::Hole
    )
}
