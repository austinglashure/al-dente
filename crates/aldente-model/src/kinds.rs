//! The closed vocabularies of the graph: node kinds, symbol subkinds, edge
//! kinds, symbol flags, languages and blind-spot categories.
//! See `docs/plan/03-graph-model.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a node is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Repo,
    Unit,
    Namespace,
    Symbol,
    Impl,
    External,
    Library,
    Platform,
    Hole,
    Endpoint,
    Datastore,
    Host,
    State,
}

impl NodeKind {
    /// Every node kind, in declaration order.
    pub const ALL: &'static [NodeKind] = &[
        NodeKind::Repo,
        NodeKind::Unit,
        NodeKind::Namespace,
        NodeKind::Symbol,
        NodeKind::Impl,
        NodeKind::External,
        NodeKind::Library,
        NodeKind::Platform,
        NodeKind::Hole,
        NodeKind::Endpoint,
        NodeKind::Datastore,
        NodeKind::Host,
        NodeKind::State,
    ];
}

/// Tells symbol nodes apart.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolSubkind {
    Fn,
    Macro,
    Var,
    Multimethod,
    Protocol,
    ProtocolMethod,
    Record,
    Type,
    Interface,
    Class,
    Method,
    Test,
}

impl SymbolSubkind {
    /// Every subkind, in declaration order.
    pub const ALL: &'static [SymbolSubkind] = &[
        SymbolSubkind::Fn,
        SymbolSubkind::Macro,
        SymbolSubkind::Var,
        SymbolSubkind::Multimethod,
        SymbolSubkind::Protocol,
        SymbolSubkind::ProtocolMethod,
        SymbolSubkind::Record,
        SymbolSubkind::Type,
        SymbolSubkind::Interface,
        SymbolSubkind::Class,
        SymbolSubkind::Method,
        SymbolSubkind::Test,
    ];
}

/// What an edge means.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeKind {
    Contains,
    Requires,
    Calls,
    References,
    Dispatches,
    Implements,
    Registers,
    Tests,
    Provides,
    Depends,
    Routes,
    TalksTo,
    Transitions,
}

impl EdgeKind {
    /// Every edge kind, in declaration order.
    pub const ALL: &'static [EdgeKind] = &[
        EdgeKind::Contains,
        EdgeKind::Requires,
        EdgeKind::Calls,
        EdgeKind::References,
        EdgeKind::Dispatches,
        EdgeKind::Implements,
        EdgeKind::Registers,
        EdgeKind::Tests,
        EdgeKind::Provides,
        EdgeKind::Depends,
        EdgeKind::Routes,
        EdgeKind::TalksTo,
        EdgeKind::Transitions,
    ];

    /// Whether edges of this kind must carry a certainty and a reason.
    ///
    /// Example: `EdgeKind::Calls.needs_certainty()` is `true`;
    /// `EdgeKind::Contains.needs_certainty()` is `false`.
    pub fn needs_certainty(self) -> bool {
        matches!(
            self,
            EdgeKind::Calls
                | EdgeKind::References
                | EdgeKind::Dispatches
                | EdgeKind::Registers
                | EdgeKind::TalksTo
                | EdgeKind::Provides
                | EdgeKind::Tests
        )
    }
}

/// Boolean properties of a symbol.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SymbolFlag {
    Private,
    Test,
    EntryPoint,
    Deprecated,
    Stateful,
    Dynamic,
    /// On units: has an entry point (`02-architecture.md`, "Units").
    Service,
}

impl SymbolFlag {
    /// Every flag, in declaration order.
    pub const ALL: &'static [SymbolFlag] = &[
        SymbolFlag::Private,
        SymbolFlag::Test,
        SymbolFlag::EntryPoint,
        SymbolFlag::Deprecated,
        SymbolFlag::Stateful,
        SymbolFlag::Dynamic,
        SymbolFlag::Service,
    ];
}

/// A source language. `Clj` is also the ID prefix for all Clojure-family code.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    Clj,
    Cljs,
}

impl Lang {
    /// Every language, in declaration order.
    pub const ALL: &'static [Lang] = &[Lang::Clj, Lang::Cljs];

    /// The token used in IDs and JSON.
    ///
    /// Example: `Lang::Cljs.as_str()` is `"cljs"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::Clj => "clj",
            Lang::Cljs => "cljs",
        }
    }

    /// Parses an ID token.
    ///
    /// Example: `Lang::from_token("clj")` is `Some(Lang::Clj)`; `"java"` gives `None`.
    pub fn from_token(token: &str) -> Option<Lang> {
        Lang::ALL.iter().copied().find(|l| l.as_str() == token)
    }
}

/// Why the analyzer could not see through something. Used by hole IDs and the
/// blind-spot report.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum BlindSpotCategory {
    DynamicEval,
    DynamicResolve,
    Interop,
    UnresolvedSymbol,
    UnparsedFile,
    MacroGenerated,
    Reflection,
}

impl BlindSpotCategory {
    /// Every category, in declaration order.
    pub const ALL: &'static [BlindSpotCategory] = &[
        BlindSpotCategory::DynamicEval,
        BlindSpotCategory::DynamicResolve,
        BlindSpotCategory::Interop,
        BlindSpotCategory::UnresolvedSymbol,
        BlindSpotCategory::UnparsedFile,
        BlindSpotCategory::MacroGenerated,
        BlindSpotCategory::Reflection,
    ];

    /// The token used in IDs and JSON.
    ///
    /// Example: `BlindSpotCategory::DynamicEval.as_str()` is `"dynamic-eval"`.
    pub fn as_str(self) -> &'static str {
        match self {
            BlindSpotCategory::DynamicEval => "dynamic-eval",
            BlindSpotCategory::DynamicResolve => "dynamic-resolve",
            BlindSpotCategory::Interop => "interop",
            BlindSpotCategory::UnresolvedSymbol => "unresolved-symbol",
            BlindSpotCategory::UnparsedFile => "unparsed-file",
            BlindSpotCategory::MacroGenerated => "macro-generated",
            BlindSpotCategory::Reflection => "reflection",
        }
    }

    /// Parses an ID token.
    ///
    /// Example: `from_token("interop")` is `Some(BlindSpotCategory::Interop)`.
    pub fn from_token(token: &str) -> Option<BlindSpotCategory> {
        BlindSpotCategory::ALL
            .iter()
            .copied()
            .find(|c| c.as_str() == token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_name<T: Serialize>(value: T) -> String {
        serde_json::to_value(value)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn id_tokens_match_json_names() {
        for l in Lang::ALL {
            assert_eq!(l.as_str(), json_name(l));
            assert_eq!(Lang::from_token(l.as_str()), Some(*l));
        }
        for c in BlindSpotCategory::ALL {
            assert_eq!(c.as_str(), json_name(c));
            assert_eq!(BlindSpotCategory::from_token(c.as_str()), Some(*c));
        }
    }

    #[test]
    fn json_names_follow_the_plan() {
        assert_eq!(json_name(SymbolSubkind::Fn), "fn");
        assert_eq!(json_name(SymbolSubkind::ProtocolMethod), "protocol-method");
        assert_eq!(json_name(EdgeKind::TalksTo), "talks-to");
        assert_eq!(json_name(SymbolFlag::EntryPoint), "entry-point");
        assert_eq!(json_name(NodeKind::Datastore), "datastore");
    }

    #[test]
    fn certainty_bearing_edge_kinds_match_the_plan() {
        let needing: Vec<_> = EdgeKind::ALL
            .iter()
            .copied()
            .filter(|k| k.needs_certainty())
            .collect();
        assert_eq!(
            needing,
            vec![
                EdgeKind::Calls,
                EdgeKind::References,
                EdgeKind::Dispatches,
                EdgeKind::Registers,
                EdgeKind::Tests,
                EdgeKind::Provides,
                EdgeKind::TalksTo,
            ]
        );
    }
}
