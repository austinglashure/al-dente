//! Tying frames to graph nodes, and marking noise.

use std::collections::HashMap;

use aldente_graph::Graph;
use aldente_model::trace::{Frame, FrameLang, MatchLevel};
use aldente_model::{Node, NodeId, NodeKind, SymbolSubkind};
use aldente_rules::trace::Noise;

/// The repository's namespaces with their definitions, for frame lookup.
pub struct TraceIndex<'a> {
    by_ns: HashMap<&'a str, NsDefs<'a>>,
}

/// One namespace's symbols and impls that have a source location.
#[derive(Default)]
struct NsDefs<'a> {
    by_name: HashMap<&'a str, Vec<&'a Node>>,
    nodes: Vec<&'a Node>,
}

impl<'a> TraceIndex<'a> {
    /// Indexes every symbol and impl with a location, by namespace and name.
    /// Namespaces without such definitions are still known as repository
    /// namespaces.
    ///
    /// Example: for clj-mini, `is_repo("shop.pricing")` is `true`.
    pub fn new(graph: &'a Graph) -> TraceIndex<'a> {
        let mut by_ns: HashMap<&str, NsDefs> = HashMap::new();
        for n in graph.nodes.values() {
            match n.kind {
                NodeKind::Namespace => {
                    by_ns.entry(n.name.as_str()).or_default();
                }
                NodeKind::Symbol | NodeKind::Impl => {
                    let (Some(ns), Some(_)) = (&n.namespace, &n.loc) else {
                        continue;
                    };
                    let defs = by_ns.entry(ns.as_str()).or_default();
                    defs.by_name.entry(n.name.as_str()).or_default().push(n);
                    defs.nodes.push(n);
                }
                _ => {}
            }
        }
        TraceIndex { by_ns }
    }

    /// Whether `ns` is one of the repository's namespaces.
    ///
    /// Example: `is_repo("clojure.core")` is `false` for clj-mini.
    pub fn is_repo(&self, ns: &str) -> bool {
        self.by_ns.contains_key(ns)
    }

    /// The repository namespace a frame belongs to: its namespace, or for a
    /// Java frame of a record or type method (`shop.payment.Card.charge`)
    /// the class's package.
    ///
    /// Example: a frame in class `shop.payment.Card` gives `shop.payment`.
    fn repo_ns<'f>(&self, frame: &'f Frame) -> Option<&'f str> {
        let ns = frame.namespace.as_deref()?;
        if self.is_repo(ns) {
            return Some(ns);
        }
        let (pkg, _) = ns.rsplit_once('.')?;
        (frame.lang == FrameLang::Java && self.is_repo(pkg)).then_some(pkg)
    }
}

/// Marks noise and maps every frame of `frames` to a node.
///
/// Example: `at shop.pricing$half_off.invokeStatic(pricing.clj:10)` maps to
/// `clj:.//shop.pricing/half-off`, `exact`; `at clojure.lang.MultiFn.invoke(…)`
/// becomes noise.
pub fn map_frames(index: &TraceIndex<'_>, noise: &Noise, frames: &mut [Frame]) {
    for f in frames {
        let repo = index.repo_ns(f).is_some();
        let js_file_noise =
            f.lang == FrameLang::Js && f.file.as_deref().is_some_and(|p| noise.is_noise(p));
        f.noise =
            !repo && (js_file_noise || f.namespace.as_deref().is_some_and(|ns| noise.is_noise(ns)));
        let (node, level) = if f.lang == FrameLang::Js {
            map_js_by_name(index, f)
        } else {
            map_frame(index, f)
        };
        f.node = node;
        f.match_level = level;
    }
}

/// The node a frame ran in and how sure the match is.
///
/// Named frames resolve by name: `exact` when the line falls inside the
/// definition, `name` when it does not. Anonymous frames (and Java frames of
/// record methods) take the innermost definition of their namespace whose
/// range holds the line: `exact`, since namespace and line agree. A name
/// that resolves to nothing falls back to the line: `line`.
///
/// Example: `shop.pricing$eval334$fn__335.invoke(pricing.clj:13)` maps to
/// `price@:book`, `exact`.
fn map_frame(index: &TraceIndex<'_>, f: &Frame) -> (Option<NodeId>, MatchLevel) {
    let Some(ns) = index.repo_ns(f) else {
        return (None, MatchLevel::None);
    };
    let Some(defs) = index.by_ns.get(ns) else {
        return (None, MatchLevel::None);
    };
    let file_ok = |n: &Node| match (&f.file, &n.loc) {
        (Some(file), Some(loc)) => basename(file) == basename(loc.file.as_str()),
        _ => true,
    };
    let enclosing = || innermost(defs.nodes.iter().copied().filter(|n| file_ok(n)), f.line);
    let named = f.lang != FrameLang::Java && !f.anonymous;
    let var = if f.lang == FrameLang::Java {
        None
    } else {
        f.name.as_deref()
    };
    if let Some(var) = var {
        let found: Vec<&Node> = defs
            .by_name
            .get(var)
            .map(|v| v.iter().copied().filter(|n| file_ok(n)).collect())
            .unwrap_or_default();
        if let Some(def) = found.iter().find(|n| holds(n, f.line)) {
            let node = if named {
                Some(*def)
            } else {
                enclosing().or(Some(*def))
            };
            return (node.map(|n| n.id.clone()), MatchLevel::Exact);
        }
        if let Some(def) = found.first() {
            return (Some(def.id.clone()), MatchLevel::Name);
        }
        return match enclosing() {
            Some(n) => (Some(n.id.clone()), MatchLevel::Line),
            None => (None, MatchLevel::None),
        };
    }
    match enclosing() {
        Some(n) => protocol_method(defs, n, f).unwrap_or((Some(n.id.clone()), MatchLevel::Exact)),
        None => (None, MatchLevel::None),
    }
}

/// A JavaScript frame that no source map covered: its line is a position
/// in compiled code, so only the name can match, at level `name`. Without
/// a namespace (Safari prints bare names) the name must be unique across
/// the repository.
///
/// Example: `app$core$parse_qty@…/core.js:7:8` without a map gives
/// `clj:.//app.core/parse-qty`, `name`.
fn map_js_by_name(index: &TraceIndex<'_>, f: &Frame) -> (Option<NodeId>, MatchLevel) {
    let Some(name) = f.name.as_deref() else {
        return (None, MatchLevel::None);
    };
    let found: Vec<&Node> = match f.namespace.as_deref() {
        Some(ns) => index
            .by_ns
            .get(ns)
            .and_then(|d| d.by_name.get(name))
            .cloned()
            .unwrap_or_default(),
        None => index
            .by_ns
            .values()
            .filter_map(|d| d.by_name.get(name))
            .flatten()
            .copied()
            .collect(),
    };
    let ids: std::collections::BTreeSet<&NodeId> = found.iter().map(|n| &n.id).collect();
    match ids.into_iter().collect::<Vec<_>>().as_slice() {
        [one] => (Some((*one).clone()), MatchLevel::Name),
        _ => (None, MatchLevel::None),
    }
}

/// A protocol dispatch function (`…$G__12…`) compiled at the `defprotocol`
/// line lands on the protocol; with a single method it is that method,
/// otherwise the protocol at level `line`.
///
/// Example: `shop.payment$eval197$fn__198$G__188__205.invoke(payment.clj:4)`
/// maps to `shop.payment/charge`.
fn protocol_method(
    defs: &NsDefs<'_>,
    protocol: &Node,
    f: &Frame,
) -> Option<(Option<NodeId>, MatchLevel)> {
    if protocol.subkind != Some(SymbolSubkind::Protocol) || !f.raw.contains("$G__") {
        return None;
    }
    let loc = protocol.loc.as_ref()?;
    let methods: Vec<&&Node> = defs
        .nodes
        .iter()
        .filter(|n| {
            n.subkind == Some(SymbolSubkind::ProtocolMethod)
                && n.loc.as_ref().is_some_and(|l| {
                    l.file == loc.file && l.line >= loc.line && l.line <= loc.end_line
                })
        })
        .collect();
    Some(match methods.as_slice() {
        [one] => (Some(one.id.clone()), MatchLevel::Exact),
        _ => (Some(protocol.id.clone()), MatchLevel::Line),
    })
}

/// Whether `n`'s range holds `line`.
///
/// Example: a node on lines 9–10 holds 9 and 10, not 11.
fn holds(n: &Node, line: Option<u32>) -> bool {
    match (line, &n.loc) {
        (Some(l), Some(loc)) => loc.line <= l && l <= loc.end_line.max(loc.line),
        _ => false,
    }
}

/// The smallest range among `nodes` that holds `line`.
///
/// Example: an impl on lines 12–13 inside nothing else is chosen for line 13.
fn innermost<'a>(nodes: impl Iterator<Item = &'a Node>, line: Option<u32>) -> Option<&'a Node> {
    nodes.filter(|n| holds(n, line)).min_by_key(|n| {
        n.loc
            .as_ref()
            .map(|l| (l.end_line - l.line, std::cmp::Reverse(l.line)))
    })
}

/// The last path component.
///
/// Example: `basename("src/shop/core.clj")` is `"core.clj"`.
fn basename(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}
