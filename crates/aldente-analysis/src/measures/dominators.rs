//! Dominators from the entry points (Cooper, Harvey and Kennedy's
//! iterative algorithm) over a virtual root that calls every entry point.

use super::projection::{Ix, Projection};

/// For each node, how many other nodes it dominates (nodes every path from
/// an entry point to them passes through it). Unreachable nodes count 0.
///
/// Example: with entry `a` and `a → b → c`, `a` dominates 2 and `b` 1.
pub fn dominated_counts(p: &Projection, entries: &[Ix]) -> Vec<u32> {
    let n = p.len();
    let root = n; // virtual
    let mut is_entry = vec![false; n];
    for &e in entries {
        is_entry[e as usize] = true;
    }
    // Reverse postorder from the root.
    let succ = |v: usize| -> Vec<usize> {
        if v == root {
            entries.iter().map(|&e| e as usize).collect()
        } else {
            p.outgoing(v as Ix)
                .iter()
                .map(|a| a.other as usize)
                .collect()
        }
    };
    let mut post: Vec<usize> = Vec::with_capacity(n + 1);
    let mut seen = vec![false; n + 1];
    let mut stack: Vec<(usize, Vec<usize>, usize)> = vec![(root, succ(root), 0)];
    seen[root] = true;
    while let Some(top) = stack.last_mut() {
        if top.2 < top.1.len() {
            let w = top.1[top.2];
            top.2 += 1;
            if !seen[w] {
                seen[w] = true;
                let s = succ(w);
                stack.push((w, s, 0));
            }
            continue;
        }
        post.push(top.0);
        stack.pop();
    }
    let mut po_num = vec![usize::MAX; n + 1];
    for (i, &v) in post.iter().enumerate() {
        po_num[v] = i;
    }
    let rpo: Vec<usize> = post.iter().rev().copied().collect();
    let mut idom = vec![usize::MAX; n + 1];
    idom[root] = root;
    let preds = |v: usize| -> Vec<usize> {
        let mut ps: Vec<usize> = p
            .incoming(v as Ix)
            .iter()
            .map(|a| a.other as usize)
            .collect();
        if is_entry[v] {
            ps.push(root);
        }
        ps
    };
    let intersect = |idom: &[usize], mut a: usize, mut b: usize| -> usize {
        while a != b {
            while po_num[a] < po_num[b] {
                a = idom[a];
            }
            while po_num[b] < po_num[a] {
                b = idom[b];
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &v in rpo.iter().skip(1) {
            let mut new = usize::MAX;
            for q in preds(v) {
                if po_num[q] == usize::MAX || idom[q] == usize::MAX {
                    continue;
                }
                new = if new == usize::MAX {
                    q
                } else {
                    intersect(&idom, q, new)
                };
            }
            if new != usize::MAX && idom[v] != new {
                idom[v] = new;
                changed = true;
            }
        }
    }
    // Subtree sizes in the dominator tree, children before parents.
    let mut size = vec![1u32; n + 1];
    for &v in &post {
        if v != root && idom[v] != usize::MAX && idom[v] != root {
            size[idom[v]] += size[v];
        }
    }
    (0..n)
        .map(|v| {
            if po_num[v] == usize::MAX {
                0
            } else {
                size[v] - 1
            }
        })
        .collect()
}
