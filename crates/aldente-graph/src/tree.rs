//! The repository's directory tree and the graph at the tree's resolution:
//! a collapsed directory is one node, an expanded one shows what it holds,
//! and a fully expanded tree is the namespace map of the repository's code.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use aldente_model::tree::{FileStatus, TreeDir, TreeFile};
use aldente_model::{
    DiagnosticCode, Edge, EdgeKind, GraphPayload, Node, NodeId, NodeKind, RepoPath,
};

use crate::build::Graph;
use crate::rollup::roll;

/// Edge kinds that are invocation (rolled up between groups).
const INVOCATION: &[EdgeKind] = &[EdgeKind::Calls, EdgeKind::References, EdgeKind::Dispatches];

/// A directory while the tree is being built.
#[derive(Default)]
struct Raw {
    dirs: BTreeMap<String, Raw>,
    files: Vec<TreeFile>,
}

/// The tree of the analyzed source files plus the source files the analyzer
/// skipped (outside every source root), each with the namespaces it defines
/// and its status. A directory holding only one directory is merged into it.
///
/// Example: in mono-mini, `libs/common/src/common` is one row holding
/// `util.clj` (namespace `clj:libs/common//common.util`).
pub fn tree(graph: &Graph, analyzed: &[RepoPath]) -> TreeDir {
    let mut namespaces: HashMap<&str, Vec<NodeId>> = HashMap::new();
    for n in graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Namespace)
    {
        if let Some(loc) = &n.loc {
            namespaces
                .entry(loc.file.as_str())
                .or_default()
                .push(n.id.clone());
        }
    }
    let with_code = |code: DiagnosticCode| -> BTreeSet<&RepoPath> {
        graph
            .diagnostics
            .iter()
            .filter(|d| d.code == code)
            .filter_map(|d| d.path.as_ref())
            .collect()
    };
    let broken = with_code(DiagnosticCode::ParseError);
    let skipped = with_code(DiagnosticCode::OutsideRoots);
    let mut root = Raw::default();
    let files = analyzed
        .iter()
        .map(|p| (p, false))
        .chain(skipped.iter().map(|p| (*p, true)));
    for (path, is_skipped) in files {
        let mut parts: Vec<&str> = path.components().collect();
        let Some(name) = parts.pop() else { continue };
        let mut dir = &mut root;
        for part in parts {
            dir = dir.dirs.entry(part.to_owned()).or_default();
        }
        let mut ns = namespaces.get(path.as_str()).cloned().unwrap_or_default();
        ns.sort();
        dir.files.push(TreeFile {
            path: path.clone(),
            name: name.to_owned(),
            namespaces: ns,
            status: if is_skipped {
                FileStatus::NotAnalyzed
            } else if broken.contains(path) {
                FileStatus::ParseError
            } else {
                FileStatus::Analyzed
            },
        });
    }
    finish(RepoPath::root(), ".".to_owned(), root, true)
}

/// Turns a raw directory into a row, merging single-directory chains.
///
/// Example: `libs` holding only `common` holding only `src` becomes one row
/// `libs/common/src/…`.
fn finish(path: RepoPath, label: String, mut raw: Raw, is_root: bool) -> TreeDir {
    let (mut path, mut label) = (path, label);
    while !is_root && raw.files.is_empty() && raw.dirs.len() == 1 {
        let Some((name, child)) = raw.dirs.pop_first() else {
            break;
        };
        path = child_path(&path, &name);
        label = format!("{label}/{name}");
        raw = child;
    }
    let mut dirs: Vec<TreeDir> = raw
        .dirs
        .into_iter()
        .map(|(name, child)| finish(child_path(&path, &name), name, child, false))
        .collect();
    dirs.sort_by(|a, b| a.label.cmp(&b.label));
    raw.files.sort_by(|a, b| a.name.cmp(&b.name));
    TreeDir {
        id: (!is_root).then(|| NodeId::Dir(path.clone())),
        path,
        label,
        dirs,
        files: raw.files,
    }
}

/// `parent/name`, or `name` under the root.
///
/// Example: `child_path(".", "src")` is `src`; `child_path("a", "b")` is `a/b`.
fn child_path(parent: &RepoPath, name: &str) -> RepoPath {
    let joined = if parent.as_str() == "." {
        name.to_owned()
    } else {
        format!("{}/{name}", parent.as_str())
    };
    RepoPath::parse(&joined).unwrap_or_else(|_| parent.clone())
}

/// Each file's hiding row (if any), and the visible collapsed rows with labels.
type Visibility = (HashMap<RepoPath, Option<RepoPath>>, Vec<(RepoPath, String)>);

/// For every file in the tree, the collapsed row hiding it (the topmost
/// collapsed one), if any; and the visible collapsed rows with their labels.
///
/// Example: with `services` collapsed, `services/orders/src/x.clj` maps to
/// `services`, and `services` is visible with label `services`.
fn visibility(tree: &TreeDir, collapsed: &BTreeSet<RepoPath>) -> Visibility {
    fn walk(
        d: &TreeDir,
        hidden_by: Option<&RepoPath>,
        collapsed: &BTreeSet<RepoPath>,
        files: &mut HashMap<RepoPath, Option<RepoPath>>,
        visible: &mut Vec<(RepoPath, String)>,
    ) {
        for f in &d.files {
            files.insert(f.path.clone(), hidden_by.cloned());
        }
        for c in &d.dirs {
            let hides = hidden_by.or_else(|| collapsed.contains(&c.path).then_some(&c.path));
            if hidden_by.is_none() && hides.is_some() {
                visible.push((c.path.clone(), c.label.clone()));
            }
            walk(c, hides, collapsed, files, visible);
        }
    }
    let mut files = HashMap::new();
    let mut visible = Vec::new();
    walk(tree, None, collapsed, &mut files, &mut visible);
    (files, visible)
}

/// The repository's code at the tree's resolution: each visible collapsed
/// row is one `directory` node, files under expanded rows show their
/// namespaces, holes stay their own nodes; externals, platforms and
/// libraries are left out. Invocation edges roll up between visible nodes;
/// `requires` edges between two namespaces stay as stored and roll up when
/// a directory is involved. With nothing collapsed this is the repository's
/// namespace map (`namespace` level with `scope=repo`).
///
/// Example: mono-mini with `services` collapsed gives `dir:services` with a
/// rolled `calls` edge to `clj:libs/common//common.util`.
pub fn tree_graph(graph: &Graph, tree: &TreeDir, collapsed: &BTreeSet<RepoPath>) -> GraphPayload {
    let (files, visible_dirs) = visibility(tree, collapsed);
    let mut memo: HashMap<&NodeId, Option<NodeId>> = HashMap::new();
    let place = |id: &NodeId| -> Option<NodeId> {
        let node = graph.nodes.get(id)?;
        match node.kind {
            NodeKind::Hole => Some(id.clone()),
            NodeKind::Namespace | NodeKind::Symbol | NodeKind::Impl => {
                let hidden = node
                    .loc
                    .as_ref()
                    .and_then(|l| files.get(&l.file))
                    .and_then(|h| h.as_ref());
                match hidden {
                    Some(dir) => Some(NodeId::Dir(dir.clone())),
                    None => graph.namespace_of(id),
                }
            }
            _ => None,
        }
    };
    let mut kept: Vec<Edge> = Vec::new();
    let mut members: HashMap<(NodeId, NodeId, EdgeKind), Vec<&Edge>> = HashMap::new();
    for e in &graph.edges {
        let invocation = INVOCATION.contains(&e.kind);
        if !invocation && e.kind != EdgeKind::Requires {
            continue;
        }
        let from = match memo.get(&e.from) {
            Some(v) => v.clone(),
            None => {
                let v = place(&e.from);
                memo.insert(&e.from, v.clone());
                v
            }
        };
        let to = match memo.get(&e.to) {
            Some(v) => v.clone(),
            None => {
                let v = place(&e.to);
                memo.insert(&e.to, v.clone());
                v
            }
        };
        let (Some(from), Some(to)) = (from, to) else {
            continue;
        };
        if !invocation && from == e.from && to == e.to {
            kept.push(e.clone());
        } else if from != to {
            members.entry((from, to, e.kind)).or_default().push(e);
        }
    }
    let mut ids: BTreeSet<NodeId> = kept
        .iter()
        .flat_map(|e| [e.from.clone(), e.to.clone()])
        .chain(members.keys().flat_map(|(f, t, _)| [f.clone(), t.clone()]))
        .collect();
    for n in graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Namespace)
    {
        if place(&n.id).as_ref() == Some(&n.id) {
            ids.insert(n.id.clone());
        }
    }
    let labels: HashMap<RepoPath, String> = visible_dirs.into_iter().collect();
    for path in labels.keys() {
        ids.insert(NodeId::Dir(path.clone()));
    }
    let mut nodes: Vec<Node> = ids
        .into_iter()
        .map(|id| match &id {
            NodeId::Dir(path) => {
                dir_node(&id, labels.get(path).map_or(path.as_str(), String::as_str))
            }
            _ => graph.group_node(&id),
        })
        .collect();
    graph.set_members(&mut nodes);
    let mut edges = kept;
    edges.extend(
        members
            .into_iter()
            .map(|((from, to, kind), es)| roll(from, to, kind, &es)),
    );
    edges.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    GraphPayload {
        nodes,
        edges,
        truncated: false,
        aggregated: None,
    }
}

/// A `directory` node for a collapsed row.
///
/// Example: `dir_node(dir:services, "services")`.
fn dir_node(id: &NodeId, label: &str) -> Node {
    Node {
        id: id.clone(),
        kind: NodeKind::Directory,
        subkind: None,
        name: label.to_owned(),
        namespace: None,
        unit: None,
        langs: BTreeSet::new(),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
        members: None,
        symbols: None,
    }
}
