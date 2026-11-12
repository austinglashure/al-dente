//! Proptest strategies shared by the crate's property tests.

use proptest::prelude::*;

use crate::graph::{Edge, GraphPayload, Site};
use crate::id::{ExternalId, HoleId, ImplId, LibraryId, NamespaceId, NodeId, PlatformId, SymbolId};
use crate::kinds::{BlindSpotCategory, EdgeKind, Lang, NodeKind};
use crate::path::RepoPath;
use crate::{Certainty, Reason};

/// Parses a path known to be valid in a strategy.
fn path(s: &str) -> RepoPath {
    RepoPath::parse(s).unwrap()
}

/// Repo paths, including the root and components with `:`, `@`, `%`, `\\` and unicode.
pub(crate) fn arb_path() -> impl Strategy<Value = RepoPath> {
    let component =
        "[a-zA-Z0-9_ .:@%\\\\é-]{1,8}".prop_filter("not . or ..", |c| c != "." && c != "..");
    prop_oneof![
        Just(RepoPath::root()),
        prop::collection::vec(component, 1..4).prop_map(|cs| path(&cs.join("/"))),
    ]
}

/// Namespace names (never contain `/`).
pub(crate) fn arb_ns() -> impl Strategy<Value = String> {
    "[a-z0-9._!?*<>@%é-]{1,12}"
}

/// Symbol names with `/`, `!`, `?`, `->`, `@`, `%` and unicode.
pub(crate) fn arb_name() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_!?*<>=+./@%:λé→-]{1,12}"
}

/// Non-empty printable text for discriminators and opaque parts.
pub(crate) fn arb_text() -> impl Strategy<Value = String> {
    "\\PC{1,12}"
}

/// Any language.
pub(crate) fn arb_lang() -> impl Strategy<Value = Lang> {
    prop::sample::select(Lang::ALL)
}

/// Symbol IDs in arbitrary units and namespaces.
pub(crate) fn arb_symbol() -> impl Strategy<Value = SymbolId> {
    (arb_lang(), arb_path(), arb_ns(), arb_name()).prop_map(|(l, u, n, name)| {
        SymbolId::new(NamespaceId::new(l, u, &n).unwrap(), &name).unwrap()
    })
}

/// IDs of every node kind.
pub(crate) fn arb_node_id() -> impl Strategy<Value = NodeId> {
    prop_oneof![
        Just(NodeId::Repo),
        arb_path().prop_map(NodeId::Unit),
        (arb_lang(), arb_path(), arb_ns())
            .prop_map(|(l, u, n)| NodeId::Namespace(NamespaceId::new(l, u, &n).unwrap())),
        arb_symbol().prop_map(NodeId::Symbol),
        (arb_symbol(), arb_text()).prop_map(|(s, d)| NodeId::Impl(ImplId::new(s, &d).unwrap())),
        (arb_lang(), arb_ns(), arb_name())
            .prop_map(|(l, n, name)| NodeId::External(ExternalId::new(l, &n, &name).unwrap())),
        (arb_lang(), arb_ns())
            .prop_map(|(l, n)| NodeId::External(ExternalId::namespace(l, &n).unwrap())),
        ("[a-z]{1,6}", arb_text())
            .prop_map(|(e, c)| NodeId::Library(LibraryId::new(&e, &c).unwrap())),
        ("[a-zA-Z.$]{1,12}", proptest::option::of(arb_text()))
            .prop_map(|(c, m)| { NodeId::Platform(PlatformId::java(&c, m.as_deref()).unwrap()) }),
        arb_text().prop_map(|p| NodeId::Platform(PlatformId::js(&p).unwrap())),
        (
            prop::sample::select(BlindSpotCategory::ALL),
            arb_path(),
            any::<u32>(),
            any::<u32>()
        )
            .prop_map(|(c, f, l, col)| NodeId::Hole(HoleId::new(c, f, l, col))),
        (
            prop::sample::select(
                &[
                    NodeKind::Endpoint,
                    NodeKind::Datastore,
                    NodeKind::Host,
                    NodeKind::State
                ][..]
            ),
            arb_text()
        )
            .prop_map(|(k, t)| NodeId::opaque(k, &t).unwrap()),
    ]
}

/// Arbitrary edges between arbitrary IDs, with or without certainty.
pub(crate) fn arb_edge() -> impl Strategy<Value = Edge> {
    (
        arb_node_id(),
        arb_node_id(),
        prop::sample::select(EdgeKind::ALL),
        proptest::option::of(prop::sample::select(Certainty::ALL)),
        proptest::option::of(prop::sample::select(Reason::ALL)),
        proptest::option::of(arb_lang()),
        prop::collection::vec((arb_path(), any::<u32>(), any::<u32>()), 0..3),
    )
        .prop_map(|(from, to, kind, certainty, reason, lang, sites)| Edge {
            from,
            to,
            kind,
            certainty,
            reason,
            lang,
            sites: sites
                .into_iter()
                .map(|(file, line, col)| Site { file, line, col })
                .collect(),
            count: None,
            by_certainty: None,
        })
}

/// Arbitrary graph payloads (edges only; nodes are covered by unit tests).
pub(crate) fn arb_payload() -> impl Strategy<Value = GraphPayload> {
    (prop::collection::vec(arb_edge(), 0..4), any::<bool>()).prop_map(|(edges, truncated)| {
        GraphPayload {
            nodes: vec![],
            edges,
            truncated,
            aggregated: None,
        }
    })
}
