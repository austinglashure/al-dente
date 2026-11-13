//! Churn and co-change from commit history: per-file and per-symbol change
//! counts, files that change together, and co-change the static graph does
//! not explain (hidden coupling). Pure: the history is read elsewhere.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use aldente_acquire::history::{CommitHunks, CommitStat};
use aldente_graph::Graph;
use aldente_model::churn::{ChurnReport, CoChange, FileChurn, HistoryDepth, SymbolChurn};
use aldente_model::{EdgeKind, NodeId, NodeKind};

/// Settings of the churn analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct ChurnOptions {
    /// Least commits a pair must share to be reported.
    pub min_support: u32,
    /// Least confidence (either direction) a pair must reach.
    pub min_confidence: f64,
    /// Commits touching more files are left out of co-change (bulk edits,
    /// reformatting, version bumps).
    pub max_files_per_commit: usize,
    /// Static distance within which co-change counts as explained.
    pub hidden_hops: u32,
}

impl Default for ChurnOptions {
    fn default() -> Self {
        ChurnOptions {
            min_support: 3,
            min_confidence: 0.5,
            max_files_per_commit: 50,
            hidden_hops: 2,
        }
    }
}

/// What was read from history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    pub depth: HistoryDepth,
    /// Newest first.
    pub commits: Vec<CommitStat>,
    /// Newest first; a prefix of `commits` (the symbol window).
    pub hunks: Vec<CommitHunks>,
    pub since: Option<String>,
}

/// The churn report. `exists` says whether a current path is still in the
/// working tree; files that are gone are left out.
///
/// Example: in a repository where `a.clj` and `b.clj` changed together in
/// three commits, `co_changes` holds that pair with support 3.
pub fn churn(
    graph: &Graph,
    history: &History,
    exists: &dyn Fn(&str) -> bool,
    opts: &ChurnOptions,
) -> ChurnReport {
    let aliases = rename_aliases(&history.commits);
    let current = |commit: usize, path: &str| -> String {
        aliases
            .get(&(commit, path.to_owned()))
            .cloned()
            .unwrap_or_else(|| path.to_owned())
    };
    let mut files: BTreeMap<String, FileChurn> = BTreeMap::new();
    let mut touched: Vec<BTreeSet<String>> = Vec::new();
    for (i, c) in history.commits.iter().enumerate() {
        let mut set = BTreeSet::new();
        for f in &c.files {
            let path = current(i, &f.path);
            if !exists(&path) {
                continue;
            }
            let entry = files.entry(path.clone()).or_insert_with(|| FileChurn {
                path: path.clone(),
                commits: 0,
                added: 0,
                deleted: 0,
                last_changed: date(c.time),
            });
            if set.insert(path) {
                entry.commits += 1;
            }
            entry.added += f.added.unwrap_or(0);
            entry.deleted += f.deleted.unwrap_or(0);
        }
        touched.push(set);
    }
    let index: HashMap<&str, usize> = history
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.as_str(), i))
        .collect();
    let symbols = symbol_churn(graph, &history.hunks, &|id: &str, path: &str| {
        index
            .get(id)
            .map_or_else(|| path.to_owned(), |&i| current(i, path))
    });
    let co_changes = co_changes(graph, &touched, &files, opts);
    let mut files: Vec<FileChurn> = files.into_values().collect();
    files.sort_by(|a, b| b.commits.cmp(&a.commits).then(a.path.cmp(&b.path)));
    ChurnReport {
        history: history.depth,
        commits: history.commits.len() as u32,
        symbol_commits: history.hunks.len() as u32,
        since: history.since.clone(),
        files,
        symbols,
        co_changes,
    }
}

/// For each `(commit position, path in that commit)`, the file's current
/// path, following renames from newest to oldest.
///
/// Example: commit 0 renames `a.clj` to `b.clj`; then `(1, "a.clj")` maps
/// to `b.clj`.
fn rename_aliases(commits: &[CommitStat]) -> HashMap<(usize, String), String> {
    // Historical path → current path, valid for older commits.
    let mut later: HashMap<String, String> = HashMap::new();
    let mut out = HashMap::new();
    for (i, c) in commits.iter().enumerate() {
        let mut renames = Vec::new();
        for f in &c.files {
            let now = later
                .get(&f.path)
                .cloned()
                .unwrap_or_else(|| f.path.clone());
            if now != f.path {
                out.insert((i, f.path.clone()), now.clone());
            }
            if let Some(old) = &f.old_path {
                renames.push((old.clone(), now));
            }
        }
        for (old, now) in renames {
            later.insert(old, now);
        }
    }
    out
}

/// Distinct commits per symbol whose changed lines overlap its current
/// source range.
///
/// Example: a hunk adding line 5 of `src/a.clj`, where `f` spans lines
/// 3–7, counts one commit for `f`.
fn symbol_churn(
    graph: &Graph,
    hunks: &[CommitHunks],
    current: &dyn Fn(&str, &str) -> String,
) -> Vec<SymbolChurn> {
    let mut by_file: HashMap<&str, Vec<(u32, u32, &NodeId)>> = HashMap::new();
    for n in graph.nodes.values() {
        if let (NodeKind::Symbol | NodeKind::Impl, Some(loc)) = (n.kind, &n.loc) {
            by_file.entry(loc.file.as_str()).or_default().push((
                loc.line,
                loc.end_line.max(loc.line),
                &n.id,
            ));
        }
    }
    let mut counts: BTreeMap<&NodeId, u32> = BTreeMap::new();
    for c in hunks {
        let mut hit: BTreeSet<&NodeId> = BTreeSet::new();
        for f in &c.files {
            let path = current(&c.id, &f.path);
            let Some(defs) = by_file.get(path.as_str()) else {
                continue;
            };
            for &(start, count) in &f.ranges {
                // A pure deletion sits between `start` and `start + 1`.
                let (lo, hi) = if count == 0 {
                    (start, start + 1)
                } else {
                    (start, start + count - 1)
                };
                for &(s, e, id) in defs {
                    if s <= hi && lo <= e {
                        hit.insert(id);
                    }
                }
            }
        }
        for id in hit {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    let mut out: Vec<SymbolChurn> = counts
        .into_iter()
        .map(|(id, commits)| SymbolChurn {
            id: id.clone(),
            commits,
        })
        .collect();
    out.sort_by(|a, b| b.commits.cmp(&a.commits).then(a.id.cmp(&b.id)));
    out
}

/// Pairs of files changed together often enough, with their static distance.
///
/// Example: see [`churn`].
fn co_changes(
    graph: &Graph,
    touched: &[BTreeSet<String>],
    files: &BTreeMap<String, FileChurn>,
    opts: &ChurnOptions,
) -> Vec<CoChange> {
    let mut support: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    for set in touched
        .iter()
        .filter(|s| s.len() <= opts.max_files_per_commit)
    {
        let v: Vec<&String> = set.iter().collect();
        for i in 0..v.len() {
            for j in i + 1..v.len() {
                *support.entry((v[i].as_str(), v[j].as_str())).or_insert(0) += 1;
            }
        }
    }
    let adj = file_adjacency(graph);
    let mut out: Vec<CoChange> = support
        .into_iter()
        .filter(|&(_, s)| s >= opts.min_support)
        .filter_map(|((a, b), s)| {
            let ca = f64::from(s) / f64::from(files.get(a)?.commits.max(1));
            let cb = f64::from(s) / f64::from(files.get(b)?.commits.max(1));
            if ca.max(cb) < opts.min_confidence {
                return None;
            }
            let both_code = adj.contains_key(a) && adj.contains_key(b);
            let hops = hops_within(&adj, a, b, opts.hidden_hops);
            Some(CoChange {
                a: a.to_owned(),
                b: b.to_owned(),
                support: s,
                confidence_a_b: ca,
                confidence_b_a: cb,
                static_hops: hops,
                hidden: both_code && hops.is_none(),
            })
        })
        .collect();
    out.sort_by(|x, y| {
        y.support
            .cmp(&x.support)
            .then((&x.a, &x.b).cmp(&(&y.a, &y.b)))
    });
    out
}

/// Files joined by an edge between nodes they define (undirected). Every
/// file that defines a node is a key, even with no neighbors.
///
/// Example: `a.clj`'s `f` calling `b.clj`'s `g` joins the two files.
fn file_adjacency(graph: &Graph) -> HashMap<String, BTreeSet<String>> {
    let file_of = |id: &NodeId| -> Option<String> {
        graph
            .nodes
            .get(id)
            .and_then(|n| n.loc.as_ref())
            .map(|l| l.file.as_str().to_owned())
    };
    let mut adj: HashMap<String, BTreeSet<String>> = HashMap::new();
    for n in graph.nodes.values() {
        if let Some(l) = &n.loc {
            adj.entry(l.file.as_str().to_owned()).or_default();
        }
    }
    for e in &graph.edges {
        if matches!(e.kind, EdgeKind::Contains) {
            continue;
        }
        if let (Some(a), Some(b)) = (file_of(&e.from), file_of(&e.to))
            && a != b
        {
            adj.entry(a.clone()).or_default().insert(b.clone());
            adj.entry(b).or_default().insert(a);
        }
    }
    adj
}

/// Hops from `a` to `b` if at most `limit`.
///
/// Example: with `a – c – b`, `hops_within(adj, "a", "b", 2)` is `Some(2)`.
fn hops_within(
    adj: &HashMap<String, BTreeSet<String>>,
    a: &str,
    b: &str,
    limit: u32,
) -> Option<u32> {
    let mut seen: BTreeSet<&str> = BTreeSet::from([a]);
    let mut queue: VecDeque<(&str, u32)> = VecDeque::from([(a, 0)]);
    while let Some((v, d)) = queue.pop_front() {
        if v == b {
            return Some(d);
        }
        if d == limit {
            continue;
        }
        for w in adj.get(v).into_iter().flatten() {
            if seen.insert(w.as_str()) {
                queue.push_back((w.as_str(), d + 1));
            }
        }
    }
    None
}

/// A Unix time as a UTC `YYYY-MM-DD` date (Howard Hinnant's civil-from-days).
///
/// Example: `date(0)` is `"1970-01-01"`; `date(1_700_000_000)` is `"2023-11-14"`.
pub fn date(time: i64) -> String {
    let z = time.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests;
