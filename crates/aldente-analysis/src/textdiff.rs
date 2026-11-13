//! Line diffs between two versions of a file (Myers' O(ND) algorithm), for
//! mapping changes to symbols. Al Dente computes these itself instead of
//! asking `git diff`, which would run the working tree's clean filters.

/// One changed region: lines `old` of the old text were replaced by lines
/// `new` of the new text. Ranges are 1-based and half-open; an empty range
/// marks the position of a pure insertion or deletion (before that line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hunk {
    pub old: (u32, u32),
    pub new: (u32, u32),
}

/// Above this many differences the exact diff is skipped and the whole
/// region between the common prefix and suffix becomes one hunk (an
/// over-approximation: more symbols may be seeded, never fewer).
const MAX_EDITS: usize = 4000;

/// The hunks that turn `old` into `new`, in order.
///
/// Example: `diff_lines("a\nb\nc\n", "a\nB\nc\n")` is
/// `[Hunk { old: (2, 3), new: (2, 3) }]`.
pub fn diff_lines(old: &str, new: &str) -> Vec<Hunk> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a_mid, b_mid) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    if a_mid.is_empty() && b_mid.is_empty() {
        return Vec::new();
    }
    let ops = myers(a_mid, b_mid).unwrap_or_else(|| {
        // One hunk covering the whole middle.
        let mut ops = vec![Op::Delete; a_mid.len()];
        ops.extend(vec![Op::Insert; b_mid.len()]);
        ops
    });
    hunks(&ops, prefix as u32)
}

/// One step of an edit script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Keep,
    Delete,
    Insert,
}

/// The shortest edit script from `a` to `b`, or `None` when it needs more
/// than [`MAX_EDITS`] edits.
///
/// Example: `myers(["x"], ["y"])` is `[Delete, Insert]`.
fn myers(a: &[&str], b: &[&str]) -> Option<Vec<Op>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = ((n + m) as usize).min(MAX_EDITS) as isize;
    let offset = max + 1;
    let mut v = vec![0isize; (2 * max + 3) as usize];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for d in 0..=max {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let ki = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[ki - 1] < v[ki + 1]) {
                v[ki + 1]
            } else {
                v[ki - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[ki] = x;
            if x >= n && y >= m {
                return Some(backtrack(&trace, a.len(), b.len(), offset));
            }
            k += 2;
        }
    }
    None
}

/// Walks the saved frontiers back from the end to recover the script.
///
/// Example: see [`myers`].
fn backtrack(trace: &[Vec<isize>], n: usize, m: usize, offset: isize) -> Vec<Op> {
    let (mut x, mut y) = (n as isize, m as isize);
    let mut ops = Vec::new();
    for (d, v) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let k = x - y;
        let ki = (k + offset) as usize;
        let prev_k = if k == -d || (k != d && v[ki - 1] < v[ki + 1]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(Op::Keep);
            x -= 1;
            y -= 1;
        }
        if d > 0 {
            ops.push(if x == prev_x { Op::Insert } else { Op::Delete });
        }
        x = prev_x;
        y = prev_y;
    }
    ops.reverse();
    ops
}

/// Groups consecutive edits into hunks; `skip` lines precede the script.
///
/// Example: `[Keep, Delete, Insert]` after 1 skipped line gives one hunk at
/// old `(3, 4)`, new `(3, 4)`.
fn hunks(ops: &[Op], skip: u32) -> Vec<Hunk> {
    let (mut i, mut j) = (skip + 1, skip + 1);
    let mut out = Vec::new();
    let mut open: Option<Hunk> = None;
    for op in ops {
        match op {
            Op::Keep => {
                out.extend(open.take());
                i += 1;
                j += 1;
            }
            Op::Delete => {
                let h = open.get_or_insert(Hunk {
                    old: (i, i),
                    new: (j, j),
                });
                i += 1;
                h.old.1 = i;
            }
            Op::Insert => {
                let h = open.get_or_insert(Hunk {
                    old: (i, i),
                    new: (j, j),
                });
                j += 1;
                h.new.1 = j;
            }
        }
    }
    out.extend(open);
    out
}

/// Whether a hunk's side `range` touches the inclusive line span
/// `first..=last`. A non-empty range touches it when they overlap; an empty
/// range (lines removed from, or inserted into, this side's neighbor) only
/// when it falls strictly inside the span, so a change right before or
/// after a definition does not count.
///
/// Example: `touches((5, 5), 3, 6)` is `true`; `touches((3, 3), 3, 6)` is `false`.
pub fn touches(range: (u32, u32), first: u32, last: u32) -> bool {
    let (start, end) = range;
    if start < end {
        start <= last && end > first
    } else {
        first < start && start <= last
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn h(old: (u32, u32), new: (u32, u32)) -> Hunk {
        Hunk { old, new }
    }

    #[test]
    fn identical_texts_have_no_hunks() {
        assert!(diff_lines("a\nb\n", "a\nb\n").is_empty());
        assert!(diff_lines("", "").is_empty());
    }

    #[test]
    fn replace_insert_and_delete() {
        assert_eq!(diff_lines("a\nb\nc\n", "a\nB\nc\n"), [h((2, 3), (2, 3))]);
        assert_eq!(diff_lines("a\nc\n", "a\nb\nc\n"), [h((2, 2), (2, 3))]);
        assert_eq!(diff_lines("a\nb\nc\n", "a\nc\n"), [h((2, 3), (2, 2))]);
        assert_eq!(diff_lines("", "x\n"), [h((1, 1), (1, 2))]);
    }

    #[test]
    fn separate_changes_stay_separate() {
        let old = "1\n2\n3\n4\n5\n6\n";
        let new = "1\nX\n3\n4\nY\n6\n";
        assert_eq!(diff_lines(old, new), [h((2, 3), (2, 3)), h((5, 6), (5, 6))]);
    }

    #[test]
    fn touching_a_span() {
        assert!(touches((4, 5), 3, 6));
        assert!(touches((1, 4), 3, 6));
        assert!(!touches((1, 3), 3, 6));
        assert!(!touches((7, 9), 3, 6));
        assert!(touches((5, 5), 3, 6));
        assert!(!touches((3, 3), 3, 6), "removal right before the span");
        assert!(!touches((7, 7), 3, 6), "removal right after the span");
    }

    /// Applies hunks to `old` with lines from `new`, which must give `new`.
    fn apply(old: &[&str], new: &[&str], hunks: &[Hunk]) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 1u32;
        for hk in hunks {
            while i < hk.old.0 {
                out.push(old[(i - 1) as usize].to_owned());
                i += 1;
            }
            for j in hk.new.0..hk.new.1 {
                out.push(new[(j - 1) as usize].to_owned());
            }
            i = hk.old.1;
        }
        while (i as usize) <= old.len() {
            out.push(old[(i - 1) as usize].to_owned());
            i += 1;
        }
        out
    }

    proptest! {
        #[test]
        fn hunks_rebuild_the_new_text(
            old in prop::collection::vec("[abc]", 0..30),
            new in prop::collection::vec("[abc]", 0..30),
        ) {
            let (o, n) = (old.join("\n"), new.join("\n"));
            let hunks = diff_lines(&o, &n);
            let ol: Vec<&str> = o.lines().collect();
            let nl: Vec<&str> = n.lines().collect();
            prop_assert_eq!(apply(&ol, &nl, &hunks), nl.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        }
    }
}
