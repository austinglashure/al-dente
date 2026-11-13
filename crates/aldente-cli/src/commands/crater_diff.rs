//! `crater --diff <rev>`: seeds from what changed between a revision and the
//! working tree.
//!
//! History is read with object-only git plumbing (`rev-parse`, `ls-tree`,
//! `cat-file --batch`) and the line diff is computed here. `git diff`
//! against the working tree is not used because it would run clean filters
//! named in the repository's own configuration (`07-safety.md`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use aldente_acquire::cache::{Layout, default_dir};
use aldente_acquire::local_git::{HistoryError, commit_of, read_blobs, tree_files};
use aldente_acquire::remote::{EmptyDirs, Git};
use aldente_acquire::tools;
use aldente_acquire::walk::{FileKind, Limits, classify, read_inside};
use aldente_analysis::crater::seeds::changed_symbols;
use aldente_analysis::textdiff::{Hunk, diff_lines};
use aldente_clj::forms::index_source;
use aldente_graph::Graph;
use aldente_graph::snapshot::{self, Snapshot};
use aldente_model::crater::{DiffSeeds, RemovedSymbol};
use aldente_model::{
    Certainty, EdgeKind, ErrorCode, NodeId, NodeKind, RepoPath, Warning, WarningCode,
};

use crate::commands::loaded::Loaded;
use crate::commands::target::Failure;

/// Seeds and the report for `--diff <spec>`, plus warnings for the envelope.
///
/// `spec` is `<rev>` or `<rev>..<head>`, where `<head>` must name the
/// checked-out commit: the snapshot describes the working tree, so the new
/// side of the diff is always the working tree (uncommitted changes count).
///
/// Example: `diff_seeds(&loaded, "main", Likely)` after editing `checkout`'s
/// body gives `([checkout], DiffSeeds { changed: [checkout], .. }, [])`.
pub fn diff_seeds(
    loaded: &Loaded,
    spec: &str,
    min: Certainty,
) -> Result<(Vec<NodeId>, DiffSeeds, Vec<Warning>), Failure> {
    let usage = |m: String| Failure::new(ErrorCode::Usage, m);
    if !loaded.target.git {
        return Err(usage("--diff needs a git repository".into()));
    }
    let git = hardened_git()?;
    let root = &loaded.target.root;
    let (base, head) = match spec.split_once("..") {
        Some((b, h)) => (b, Some(h)),
        None => (spec, None),
    };
    let history = |e: HistoryError| match e {
        HistoryError::BadRev(_) | HistoryError::UnknownRev(_) => usage(e.to_string()),
        HistoryError::Git { .. } => Failure::new(ErrorCode::Internal, e.to_string()),
    };
    let base_commit = commit_of(&git, root, base).map_err(history)?;
    if let Some(head) = head {
        let head_commit = commit_of(&git, root, head).map_err(history)?;
        let checked_out = commit_of(&git, root, "HEAD").map_err(history)?;
        if head_commit != checked_out {
            return Err(usage(format!(
                "the head of --diff must be the checked-out commit; check out {head} first"
            )));
        }
    }
    let base_tree: BTreeMap<String, String> = tree_files(&git, root, &base_commit)
        .map_err(history)?
        .into_iter()
        .filter(|(path, _)| is_source(path))
        .collect();
    let ids: Vec<String> = base_tree.values().cloned().collect();
    let blobs = read_blobs(&git, root, &ids).map_err(history)?;
    let base_text = |path: &str| -> Option<String> {
        let id = base_tree.get(path)?;
        blobs
            .get(id)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    };

    let snap = &loaded.outcome.snapshot;
    let analyzed: BTreeSet<String> = snap.files.iter().map(|(p, _)| p.to_string()).collect();
    let mut hunks: BTreeMap<RepoPath, Vec<Hunk>> = BTreeMap::new();
    let mut other_files = Vec::new();
    // Files whose base version may hold definitions that are now gone.
    let mut gone_from: Vec<String> = Vec::new();
    let paths: BTreeSet<&String> = analyzed.iter().chain(base_tree.keys()).collect();
    for path in paths {
        let old = base_text(path);
        let now = read_current(root, path);
        if old.as_deref() == now.as_deref() {
            continue;
        }
        match (analyzed.contains(path), RepoPath::parse(path)) {
            (true, Ok(rp)) => {
                let file_hunks =
                    diff_lines(old.as_deref().unwrap_or(""), now.as_deref().unwrap_or(""));
                hunks.insert(rp, file_hunks);
                if old.is_some() {
                    gone_from.push(path.clone());
                }
            }
            _ if now.is_none() && under_a_source_root(snap, path) => gone_from.push(path.clone()),
            _ => other_files.push(path.clone()),
        }
    }
    let changed = changed_symbols(&snap.graph, &hunks);
    let current = defined_names(&snap.graph);
    let mut removed: Vec<RemovedSymbol> = gone_from
        .iter()
        .flat_map(|path| {
            let text = base_text(path).unwrap_or_default();
            definitions(path, &text)
        })
        .filter(|r| !current.contains(&(r.namespace.clone(), r.name.clone())))
        .collect();
    removed.sort();
    removed.dedup();

    let mut warnings = Vec::new();
    let base_snap = if removed.is_empty() {
        None
    } else {
        find_base_snapshot(&snap.repo, &base_commit, &base_tree, &blobs)
    };
    match &base_snap {
        Some(b) => {
            for r in &mut removed {
                r.former_callers = Some(former_callers(&b.graph, &r.namespace, &r.name, min));
            }
        }
        None if !removed.is_empty() => warnings.push(Warning {
            code: WarningCode::NoBaseSnapshot,
            message: format!(
                "no snapshot of {base} ({}) is cached, so the former callers of removed definitions are unknown; analyze that revision once to fill them in",
                &base_commit[..base_commit.len().min(12)]
            ),
        }),
        None => {}
    }
    let report = DiffSeeds {
        base: base.to_owned(),
        base_commit,
        changed: changed.clone(),
        removed,
        other_files,
        base_snapshot: base_snap.is_some(),
    };
    Ok((changed, report, warnings))
}

/// git with the hardening, or a failure when it is not on `PATH`.
///
/// Example: `hardened_git()?` on a machine with git installed.
pub(crate) fn hardened_git() -> Result<Git, Failure> {
    let internal = |m: String| Failure::new(ErrorCode::Internal, m);
    let path_var = std::env::var_os("PATH");
    let program = tools::locate("git", path_var.as_deref())
        .ok_or_else(|| Failure::new(ErrorCode::ToolMissing, "git is not on PATH"))?;
    let cache = default_dir().ok_or_else(|| internal("no user cache directory".into()))?;
    let layout =
        Layout::open(&cache).map_err(|e| internal(format!("cannot open the cache: {e}")))?;
    let empty = EmptyDirs::prepare(&layout.tmp)
        .map_err(|e| internal(format!("cannot prepare git's empty config: {e}")))?;
    Ok(Git::new(program, empty))
}

/// Whether a path names a Clojure-family source file.
///
/// Example: `is_source("src/a.cljc")` is `true`; `is_source("README.md")` is `false`.
fn is_source(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(classify(name), Some(FileKind::Source(_)))
}

/// The working-tree text of `path`, read with the walker's inside-the-root
/// check (no symlinks out). `None` when missing or unreadable.
///
/// Example: `read_current(root, "src/a.clj")` gives `Some("(ns a)\n")`.
fn read_current(root: &Path, path: &str) -> Option<String> {
    let rp = RepoPath::parse(path).ok()?;
    let root = root.canonicalize().ok()?;
    read_inside(&root, &rp, Limits::default().max_file_bytes)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Whether `path` lies under one of the snapshot's source roots, i.e. it
/// would have been analyzed had it still existed.
///
/// Example: with root `src`, `src/a/b.clj` is under it and `scripts/x.clj` is not.
fn under_a_source_root(snap: &Snapshot, path: &str) -> bool {
    snap.units.iter().flat_map(|u| &u.roots).any(|r| {
        let root = r.path.as_str();
        root == "." || path.starts_with(&format!("{root}/"))
    })
}

/// Every `(namespace, name)` the graph defines as a symbol.
///
/// Example: clj-mini's graph contains `("shop.core", "checkout")`.
fn defined_names(graph: &Graph) -> HashSet<(String, String)> {
    graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Symbol)
        .filter_map(|n| Some((n.namespace.clone()?, n.name.clone())))
        .collect()
}

/// The named top-level definitions in one file's text (`def…` forms other
/// than `defmethod`), with the namespace from its `ns` form.
///
/// Example: `"(ns a)\n(defn f [])"` gives one `RemovedSymbol { namespace: "a", name: "f", line: 2, .. }`.
fn definitions(path: &str, text: &str) -> Vec<RemovedSymbol> {
    let Ok(rp) = RepoPath::parse(path) else {
        return Vec::new();
    };
    let index = index_source(rp, text);
    let mut ns: Option<String> = None;
    let mut out = Vec::new();
    for form in &index.forms {
        let head = form.head.as_deref().unwrap_or("");
        let local = head.rsplit('/').next().unwrap_or(head);
        if head == "ns" {
            ns = form.name.clone();
            continue;
        }
        if !local.starts_with("def") || local == "defmethod" {
            continue;
        }
        if let (Some(ns), Some(name)) = (&ns, &form.name) {
            out.push(RemovedSymbol {
                namespace: ns.clone(),
                name: name.clone(),
                file: path.to_owned(),
                line: form.span.start.line,
                former_callers: None,
            });
        }
    }
    out
}

/// A cached snapshot of `repo` at `commit` whose files all match the base
/// tree (so it was not taken from a dirty working tree).
///
/// Example: after analyzing at `HEAD~1` and committing, finds that snapshot.
fn find_base_snapshot(
    repo: &str,
    commit: &str,
    base_tree: &BTreeMap<String, String>,
    blobs: &HashMap<String, Vec<u8>>,
) -> Option<Snapshot> {
    let layout = Layout::open(&default_dir()?).ok()?;
    let mut paths: Vec<_> = std::fs::read_dir(&layout.snapshots)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "db"))
        .collect();
    paths.sort();
    paths.into_iter().find_map(|path| {
        let (r, rev) = snapshot::read_meta(&path).ok()??;
        if r != repo || rev.as_deref() != Some(commit) {
            return None;
        }
        let snap = snapshot::load(&path).ok()??;
        let matches = snap.files.iter().all(|(file, hash)| {
            base_tree
                .get(file.as_str())
                .and_then(|id| blobs.get(id))
                .is_some_and(|bytes| blake3::hash(bytes).to_hex().as_str() == hash)
        });
        matches.then_some(snap)
    })
}

/// The callers of `namespace/name` in `graph`: sources of invocation and
/// registration edges at `min` or stronger, sorted.
///
/// Example: in clj-mini, `shop.pricing/total` gives `[clj:.//shop.core/checkout]`.
fn former_callers(graph: &Graph, namespace: &str, name: &str, min: Certainty) -> Vec<NodeId> {
    let targets: HashSet<&NodeId> = graph
        .nodes
        .values()
        .filter(|n| n.namespace.as_deref() == Some(namespace) && n.name == name)
        .map(|n| &n.id)
        .collect();
    let mut out: Vec<NodeId> = graph
        .edges
        .iter()
        .filter(|e| {
            targets.contains(&e.to)
                && matches!(
                    e.kind,
                    EdgeKind::Calls
                        | EdgeKind::References
                        | EdgeKind::Dispatches
                        | EdgeKind::Registers
                )
                && e.certainty.unwrap_or(Certainty::Certain) >= min
        })
        .map(|e| e.from.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}
