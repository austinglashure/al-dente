//! Strongly connected components and cut suggestions.
//!
//! The cut is a greedy feedback arc set: a weighted Eades–Lin–Smyth order
//! (nodes whose outgoing weight most exceeds their incoming weight go
//! first), whose backward arcs break every cycle. Then, on components small
//! enough, each cut arc is put back, strongest first, if it does not close
//! a cycle again, so every arc left in the cut is needed. Low-weight
//! (low-certainty) and low-count arcs are cut first.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use super::projection::{Arc, Ix, Projection};

/// Most arcs in a component for the put-back pass.
const MINIMIZE_LIMIT: usize = 20_000;

/// Strongly connected components with more than one node, each sorted,
/// largest first (ties by first node).
///
/// Example: `a ⇄ b`, `c` gives `[[a, b]]`.
pub fn components(p: &Projection) -> Vec<Vec<Ix>> {
    let n = p.len();
    let mut index = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<Ix> = Vec::new();
    let mut next = 0u32;
    let mut out: Vec<Vec<Ix>> = Vec::new();
    // Iterative Tarjan: (node, position in its arc list).
    let mut call: Vec<(Ix, usize)> = Vec::new();
    for root in 0..n as Ix {
        if index[root as usize] != u32::MAX {
            continue;
        }
        call.push((root, 0));
        index[root as usize] = next;
        low[root as usize] = next;
        next += 1;
        stack.push(root);
        on_stack[root as usize] = true;
        while let Some(&mut (v, ref mut pos)) = call.last_mut() {
            let arcs = p.outgoing(v);
            if *pos < arcs.len() {
                let w = arcs[*pos].other;
                *pos += 1;
                if index[w as usize] == u32::MAX {
                    index[w as usize] = next;
                    low[w as usize] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w as usize] = true;
                    call.push((w, 0));
                } else if on_stack[w as usize] {
                    low[v as usize] = low[v as usize].min(index[w as usize]);
                }
                continue;
            }
            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent as usize] = low[parent as usize].min(low[v as usize]);
            }
            if low[v as usize] == index[v as usize] {
                let mut comp = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w as usize] = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                if comp.len() > 1 {
                    comp.sort_unstable();
                    out.push(comp);
                }
            }
        }
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
    out
}

/// A suggested cut for one component: `(from, to, arc)` triples, and
/// whether the put-back pass ran.
///
/// Example: for `a → b` (certain), `b → a` (possible) the cut is `b → a`.
pub fn cut(p: &Projection, comp: &[Ix]) -> (Vec<(Ix, Ix, Arc)>, bool) {
    let local: HashMap<Ix, usize> = comp.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    let m = comp.len();
    // Arcs inside the component, by local index.
    let mut arcs: Vec<(usize, usize, Arc)> = Vec::new();
    for (i, &v) in comp.iter().enumerate() {
        for a in p.outgoing(v) {
            if let Some(&j) = local.get(&a.other) {
                arcs.push((i, j, *a));
            }
        }
    }
    let order = els_order(m, &arcs);
    let mut rank = vec![0usize; m];
    for (r, &v) in order.iter().enumerate() {
        rank[v] = r;
    }
    let (mut keep, mut back): (Vec<_>, Vec<_>) = arcs
        .iter()
        .copied()
        .partition(|(f, t, _)| rank[*f] < rank[*t]);
    let minimal = arcs.len() <= MINIMIZE_LIMIT;
    if minimal {
        // Strongest first; ties by count, then position for determinism.
        back.sort_by(|a, b| {
            priority(b)
                .partial_cmp(&priority(a))
                .unwrap_or(Ordering::Equal)
                .then((a.0, a.1).cmp(&(b.0, b.1)))
        });
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); m];
        for (f, t, _) in &keep {
            adj[*f].push(*t);
        }
        let mut still = Vec::new();
        for arc in back {
            if reaches(&adj, arc.1, arc.0) {
                still.push(arc);
            } else {
                adj[arc.0].push(arc.1);
                keep.push(arc);
            }
        }
        back = still;
    }
    let mut out: Vec<(Ix, Ix, Arc)> = back
        .into_iter()
        .map(|(f, t, a)| (comp[f], comp[t], a))
        .collect();
    out.sort_by_key(|(f, t, _)| (*f, *t));
    (out, minimal)
}

/// How much an arc is worth keeping: weight, then count.
///
/// Example: a certain arc outranks a possible one.
fn priority(a: &(usize, usize, Arc)) -> (f64, u32) {
    (a.2.weight, a.2.count)
}

/// Whether `from` reaches `to` over `adj`.
///
/// Example: with `0 → 1 → 2`, `reaches(adj, 0, 2)` is `true`.
fn reaches(adj: &[Vec<usize>], from: usize, to: usize) -> bool {
    let mut seen = vec![false; adj.len()];
    let mut stack = vec![from];
    while let Some(v) = stack.pop() {
        if v == to {
            return true;
        }
        if std::mem::replace(&mut seen[v], true) {
            continue;
        }
        stack.extend(adj[v].iter().copied().filter(|&w| !seen[w]));
    }
    false
}

/// A heap entry: larger `delta` first, then smaller node.
#[derive(PartialEq)]
struct Entry(f64, usize);
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0).then(other.1.cmp(&self.1))
    }
}

/// State of the Eades–Lin–Smyth pass over one component.
struct Els {
    out: Vec<Vec<(usize, f64)>>,
    inc: Vec<Vec<(usize, f64)>>,
    wout: Vec<f64>,
    win: Vec<f64>,
    dout: Vec<usize>,
    din: Vec<usize>,
    removed: Vec<bool>,
    heap: BinaryHeap<Entry>,
    sinks: Vec<usize>,
    sources: Vec<usize>,
}

impl Els {
    /// Indexes the arcs of `m` nodes.
    ///
    /// Example: `Els::new(2, &[(0, 1, arc)])` has node 1 as its only sink.
    fn new(m: usize, arcs: &[(usize, usize, Arc)]) -> Els {
        let mut s = Els {
            out: vec![Vec::new(); m],
            inc: vec![Vec::new(); m],
            wout: vec![0.0; m],
            win: vec![0.0; m],
            dout: vec![0; m],
            din: vec![0; m],
            removed: vec![false; m],
            heap: BinaryHeap::new(),
            sinks: Vec::new(),
            sources: Vec::new(),
        };
        for &(f, t, a) in arcs {
            s.out[f].push((t, a.weight));
            s.inc[t].push((f, a.weight));
            s.wout[f] += a.weight;
            s.win[t] += a.weight;
            s.dout[f] += 1;
            s.din[t] += 1;
        }
        for v in 0..m {
            s.heap.push(Entry(s.delta(v), v));
            if s.dout[v] == 0 {
                s.sinks.push(v);
            } else if s.din[v] == 0 {
                s.sources.push(v);
            }
        }
        s
    }

    /// Outgoing minus incoming weight among nodes still present.
    ///
    /// Example: a node with one outgoing arc of weight 1 has delta 1.
    fn delta(&self, v: usize) -> f64 {
        self.wout[v] - self.win[v]
    }

    /// Removes `v`, updating its neighbors and queueing new sinks and sources.
    ///
    /// Example: removing the only source of `0 → 1` makes 1 a sink.
    fn remove(&mut self, v: usize) {
        self.removed[v] = true;
        for k in 0..self.out[v].len() {
            let (t, w) = self.out[v][k];
            if !self.removed[t] {
                self.din[t] -= 1;
                self.win[t] -= w;
                self.heap.push(Entry(self.delta(t), t));
                if self.din[t] == 0 && self.dout[t] > 0 {
                    self.sources.push(t);
                }
            }
        }
        for k in 0..self.inc[v].len() {
            let (f, w) = self.inc[v][k];
            if !self.removed[f] {
                self.dout[f] -= 1;
                self.wout[f] -= w;
                self.heap.push(Entry(self.delta(f), f));
                if self.dout[f] == 0 {
                    self.sinks.push(f);
                }
            }
        }
    }
}

/// The weighted Eades–Lin–Smyth order of `m` nodes: sinks go last,
/// sources first, otherwise the node with the largest outgoing minus
/// incoming weight goes next.
///
/// Example: for `0 → 1` the order is `[0, 1]`.
fn els_order(m: usize, arcs: &[(usize, usize, Arc)]) -> Vec<usize> {
    let mut s = Els::new(m, arcs);
    let (mut head, mut tail) = (Vec::with_capacity(m), Vec::with_capacity(m));
    let mut left = m;
    while left > 0 {
        let (v, to_tail) = if let Some(v) = s.sinks.pop() {
            (v, true)
        } else if let Some(v) = s.sources.pop() {
            (v, false)
        } else {
            let Some(Entry(d, v)) = s.heap.pop() else {
                break;
            };
            // Stale entries carry an old delta.
            if s.removed[v] || d != s.delta(v) {
                continue;
            }
            (v, false)
        };
        if s.removed[v] {
            continue;
        }
        s.remove(v);
        if to_tail {
            tail.push(v)
        } else {
            head.push(v)
        }
        left -= 1;
    }
    tail.reverse();
    head.extend(tail);
    head
}
