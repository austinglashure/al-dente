//! The resolver for rules R1–R4 (`04-clojure-analysis.md`): joins clj-kondo's
//! analysis with the form index and produces one unit's facts.
//!
//! Every clj-kondo var usage ends up as exactly one of: an edge site, an
//! unresolved-symbol blind spot, or one of the documented suppressions in
//! [`Suppression`]. [`UsageStats`] counts them so tests can prove nothing
//! is dropped.
//!
//! Namespaces this unit does not define become `external` targets; the
//! graph build re-targets those that another unit defines.

use std::collections::{BTreeMap, BTreeSet};

use aldente_adapter::{FactBatch, LiteralFact};
use aldente_model::{
    BlindSpot, BlindSpotCategory, Certainty, Diagnostic, Edge, EdgeKind, ExternalId, ImplId, Lang,
    Location, NamespaceId, Node, NodeId, NodeKind, PlatformId, Reason, RepoPath, Site, SymbolFlag,
    SymbolId, SymbolSubkind, UnitInfo,
};

use crate::forms::{FileIndex, LiteralKind, Pos, Span, TopForm};
use crate::kondo::{KondoRun, VarDefinition, VarUsage};

/// How many usages ended where.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageStats {
    /// clj-kondo var usages in.
    pub usages: usize,
    /// Usages that became an edge site.
    pub sites: usize,
    /// Usages deliberately not turned into edges, by [`Suppression`] name.
    pub suppressed: BTreeMap<&'static str, usize>,
    /// Usages whose target clj-kondo could not resolve (blind spots).
    pub unresolved: usize,
    /// Usages without a position: edges without a site.
    pub positionless: usize,
    /// Sites identical to one already on the same edge (same position).
    pub merged_duplicates: usize,
}

/// One unit's facts plus usage accounting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    pub batch: FactBatch,
    pub stats: UsageStats,
}

/// Usages that are part of a definition rather than a use. Documented in
/// `fixtures/clj-mini/EXPECTED.md` ("Scope rules").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Suppression {
    /// The head of a top-level definition form: `defn` in `(defn f …)`.
    DefinitionForm,
    /// The multimethod named by a `defmethod` (Phase 2 adds `implements`).
    DefmethodTarget,
    /// A protocol named in `extend-type`, `defrecord`, `reify`, … (Phase 2
    /// adds `implements`).
    ProtocolInImplForm,
    /// Parameter names in a `defprotocol` signature, which clj-kondo reports
    /// as unresolved symbols.
    ProtocolSignature,
    /// A name in an `ns` form's `:refer`; the `requires` edge covers it.
    NsRefer,
    /// An unresolvable symbol inside a macro's syntax-quoted template: a
    /// gensym (`start#`) or special form (`do`), not a var.
    MacroTemplate,
}

impl Suppression {
    /// The name used in [`UsageStats::suppressed`].
    ///
    /// Example: `Suppression::DefinitionForm.name()` is `"definition-form"`.
    fn name(self) -> &'static str {
        match self {
            Suppression::DefinitionForm => "definition-form",
            Suppression::DefmethodTarget => "defmethod-target",
            Suppression::ProtocolInImplForm => "protocol-in-impl-form",
            Suppression::ProtocolSignature => "protocol-signature",
            Suppression::NsRefer => "ns-refer",
            Suppression::MacroTemplate => "macro-template",
        }
    }
}

/// Heads of top-level forms whose own usage is a definition, not a call.
const DEFINITION_HEADS: &[&str] = &[
    "ns",
    "def",
    "defn",
    "defn-",
    "defonce",
    "defmacro",
    "defmulti",
    "defmethod",
    "defprotocol",
    "defrecord",
    "deftype",
    "definterface",
    "extend-type",
    "extend-protocol",
    "deftest",
];

/// Initializers that make a `def` stateful (R1).
const STATEFUL_INITS: &[&str] = &["atom", "ref", "agent", "volatile!"];

/// clj-kondo's namespace for symbols it could not resolve.
const UNKNOWN_NS: &str = "clj-kondo/unknown-namespace";

/// Resolves one unit.
///
/// Example: on clj-mini gives `clj:.//shop.core/checkout` (fn) with a
/// certain `calls` edge to `clj:.//shop.pricing/price` at `src/shop/core.clj:16:17`.
pub fn resolve(unit: &UnitInfo, run: &KondoRun, indexes: &[FileIndex]) -> Resolved {
    let mut r = Resolver::new(unit, run, indexes);
    r.namespaces();
    r.definitions();
    r.impls();
    r.requires();
    r.usages();
    r.multimethod_dispatch();
    r.protocol_dispatch();
    r.macro_blind_spots();
    r.finish()
}

/// What keeps edges distinct: `(from, to, kind, reason, lang)`.
type EdgeKey = (NodeId, NodeId, EdgeKind, Option<Reason>, Option<Lang>);

/// Working state for one unit.
struct Resolver<'a> {
    unit: &'a UnitInfo,
    run: &'a KondoRun,
    indexes: BTreeMap<&'a str, &'a FileIndex>,
    /// Namespaces defined in this unit.
    local_ns: BTreeSet<String>,
    nodes: BTreeMap<NodeId, Node>,
    edges: BTreeMap<EdgeKey, Edge>,
    blindspots: Vec<BlindSpot>,
    stats: UsageStats,
    /// `(file, form start)` of each defmethod top form → its impl node.
    defmethod_impls: BTreeMap<(String, Pos), NodeId>,
    /// `(file, body span)` of each protocol impl body (not `reify`) → impl node.
    protocol_impls: Vec<(String, Span, NodeId)>,
    /// Each multimethod's impls with their dispatch values as written.
    multi_impls: BTreeMap<NodeId, Vec<(NodeId, String)>>,
    /// Usages of multimethods, for R5 dispatch edges.
    multi_uses: Vec<MultiUse>,
    /// Each protocol method's impls with their type discriminators.
    method_impls: BTreeMap<NodeId, Vec<(NodeId, String)>>,
    /// Usages of protocol methods, for R6 dispatch edges.
    method_uses: Vec<MultiUse>,
}

/// One usage of a multimethod: who uses it, where, and how.
#[derive(Debug, Clone)]
struct MultiUse {
    scope: NodeId,
    multi: NodeId,
    file: String,
    pos: Pos,
    is_call: bool,
    lang: Option<Lang>,
    site: Option<Site>,
}

impl<'a> Resolver<'a> {
    /// Indexes the inputs.
    ///
    /// Example: `Resolver::new(unit, run, &indexes)` with clj-mini.
    fn new(unit: &'a UnitInfo, run: &'a KondoRun, indexes: &'a [FileIndex]) -> Resolver<'a> {
        Resolver {
            unit,
            run,
            indexes: indexes.iter().map(|ix| (ix.path.as_str(), ix)).collect(),
            local_ns: run
                .analysis
                .namespace_definitions
                .iter()
                .map(|n| n.name.clone())
                .collect(),
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            blindspots: Vec::new(),
            stats: UsageStats::default(),
            defmethod_impls: BTreeMap::new(),
            protocol_impls: Vec::new(),
            multi_impls: BTreeMap::new(),
            multi_uses: Vec::new(),
            method_impls: BTreeMap::new(),
            method_uses: Vec::new(),
        }
    }

    /// The namespace ID for `ns` in this unit.
    ///
    /// Example: `ns_id("shop.core")` is `clj:.//shop.core`.
    fn ns_id(&self, ns: &str) -> Option<NamespaceId> {
        NamespaceId::new(Lang::Clj, self.unit.dir.clone(), ns).ok()
    }

    /// The symbol ID for `ns/name` in this unit.
    ///
    /// Example: `sym_id("shop.core", "checkout")` is `clj:.//shop.core/checkout`.
    fn sym_id(&self, ns: &str, name: &str) -> Option<SymbolId> {
        SymbolId::new(self.ns_id(ns)?, name).ok()
    }

    /// R2: one namespace node per definition, `.cljc` languages merged, and
    /// `unit contains namespace`.
    ///
    /// Example: `shop.model` from `model.cljc` gets langs `{clj, cljs}`.
    fn namespaces(&mut self) {
        for def in &self.run.analysis.namespace_definitions {
            let Some(id) = self.ns_id(&def.name) else {
                continue;
            };
            let lang = lang_of(def.lang.as_deref(), &def.filename);
            let id = NodeId::Namespace(id);
            let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
                id: id.clone(),
                kind: NodeKind::Namespace,
                subkind: None,
                name: def.name.clone(),
                namespace: None,
                unit: Some(NodeId::Unit(self.unit.dir.clone())),
                langs: BTreeSet::new(),
                flags: BTreeSet::new(),
                loc: location(&def.filename, def.row, def.col, def.end_row, def.end_col),
                doc: def.doc.clone(),
                signature: None,
            });
            node.langs.insert(lang);
        }
    }

    /// R1: one symbol node per var definition, merged across `.cljc`
    /// languages, with subkind and flags; `namespace contains symbol`.
    ///
    /// Example: `(defonce stock (atom {}))` gives `shop.inventory/stock`,
    /// subkind `var`, flag `stateful`.
    fn definitions(&mut self) {
        for def in &self.run.analysis.var_definitions {
            let Some(sym) = self.sym_id(&def.ns, &def.name) else {
                continue;
            };
            let lang = lang_of(def.lang.as_deref(), &def.filename);
            let flags = self.flags(def);
            let id = NodeId::Symbol(sym);
            let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
                id: id.clone(),
                kind: NodeKind::Symbol,
                subkind: Some(subkind(def)),
                name: def.name.clone(),
                namespace: Some(def.ns.clone()),
                unit: Some(NodeId::Unit(self.unit.dir.clone())),
                langs: BTreeSet::new(),
                flags: BTreeSet::new(),
                loc: location(&def.filename, def.row, def.col, def.end_row, def.end_col),
                doc: def.doc.clone(),
                signature: def.arglist_strs.as_ref().map(|a| a.join(" ")),
            });
            node.langs.insert(lang);
            node.flags.extend(flags);
            if let Some(ns) = self.ns_id(&def.ns) {
                self.contains(NodeId::Namespace(ns), id);
            }
        }
    }

    /// R1 flags for a definition.
    ///
    /// Example: a `defn-` gives `{private}`; `(def ^:dynamic *x* 1)` gives `{dynamic}`.
    fn flags(&self, def: &VarDefinition) -> BTreeSet<SymbolFlag> {
        let mut flags = BTreeSet::new();
        if def.private == Some(true) {
            flags.insert(SymbolFlag::Private);
        }
        if def
            .deprecated
            .as_ref()
            .is_some_and(|d| d != &serde_json::Value::Bool(false))
        {
            flags.insert(SymbolFlag::Deprecated);
        }
        if def.meta.as_ref().and_then(|m| m.get("dynamic")) == Some(&serde_json::Value::Bool(true))
        {
            flags.insert(SymbolFlag::Dynamic);
        }
        if self.is_stateful(def) {
            flags.insert(SymbolFlag::Stateful);
        }
        flags
    }

    /// Whether a `def`'s value is `(atom …)`, `(ref …)`, `(agent …)` or
    /// `(volatile! …)`, found through the form index and clj-kondo's usage
    /// at the value's position.
    ///
    /// Example: `(defonce stock (atom {}))` is stateful; `(def n 1)` is not.
    fn is_stateful(&self, def: &VarDefinition) -> bool {
        let Some(ix) = self.indexes.get(def.filename.as_str()) else {
            return false;
        };
        let start = Pos {
            line: def.row,
            col: def.col,
        };
        let Some(value) = ix
            .calls
            .iter()
            .find(|c| c.span.start == start)
            .and_then(|c| c.args.get(1))
        else {
            return false;
        };
        self.run.analysis.var_usages.iter().any(|u| {
            u.filename == def.filename
                && upos(u) == Some(value.start)
                && STATEFUL_INITS.contains(&u.name.as_str())
                && matches!(u.to.as_deref(), Some("clojure.core" | "cljs.core"))
        })
    }

    /// R3 impl nodes that exist regardless of usages: every `defmethod` and
    /// every protocol method body outside `reify` (Phase 1 leaves `reify`
    /// bodies to the enclosing function).
    ///
    /// Example: `(defmethod price :gift [_] 0)` gives `shop.pricing/price@:gift`.
    fn impls(&mut self) {
        let indexes: Vec<&FileIndex> = self.indexes.values().copied().collect();
        for ix in indexes {
            let file = ix.path.as_str();
            let ns = self.file_ns(file);
            for form in ix
                .forms
                .iter()
                .filter(|f| local(f.head.as_deref()) == "defmethod")
            {
                let Some(ns) = ns.clone() else { continue };
                let (multi_ns, multi) = self.defmethod_target(file, form).unwrap_or_else(|| {
                    (
                        ns.clone(),
                        form.name
                            .clone()
                            .map(|n| local(Some(&n)).to_owned())
                            .unwrap_or_default(),
                    )
                });
                let dispatch = form.dispatch.clone().unwrap_or_default();
                let Some(id) = self.impl_node(&ns, &multi_ns, &multi, &dispatch, file, form.span)
                else {
                    continue;
                };
                self.implements(id.clone(), &multi_ns, &multi);
                if let Some(multi_sym) = self.sym_id(&multi_ns, &multi) {
                    self.multi_impls
                        .entry(NodeId::Symbol(multi_sym))
                        .or_default()
                        .push((id.clone(), dispatch.clone()));
                }
                self.defmethod_impls
                    .insert((file.to_owned(), form.span.start), id);
            }
            for body in &ix.impls {
                let Some(ns) = ns.clone() else { continue };
                let entry = self.run.analysis.protocol_impls.iter().find(|p| {
                    p.filename == file
                        && body.span.contains(Pos {
                            line: p.row,
                            col: p.col,
                        })
                });
                let protocol_ns = entry
                    .map(|p| p.protocol_ns.clone())
                    .unwrap_or_else(|| ns.clone());
                let protocol = entry
                    .map(|p| p.protocol_name.clone())
                    .or_else(|| body.protocol.clone());
                let disc = match (&body.type_name, body.owner.as_str()) {
                    (Some(t), _) => t.clone(),
                    (None, "reify") => format!("reify:L{}", body.owner_line),
                    (None, _) => "?".to_owned(),
                };
                let Some(id) =
                    self.impl_node(&ns, &protocol_ns, &body.method, &disc, file, body.span)
                else {
                    continue;
                };
                self.protocol_impls
                    .push((file.to_owned(), body.span, id.clone()));
                if let Some(method) = self.sym_id(&protocol_ns, &body.method) {
                    self.method_impls
                        .entry(NodeId::Symbol(method))
                        .or_default()
                        .push((id.clone(), disc));
                }
                if let Some(p) = protocol {
                    let p = local(Some(&p)).to_owned();
                    self.implements(id, &protocol_ns, &p);
                }
            }
        }
    }

    /// The multimethod a `defmethod` form names, from clj-kondo's usage with
    /// `defmethod: true` inside the form.
    ///
    /// Example: `(defmethod price :book …)` in shop.pricing gives
    /// `("shop.pricing", "price")`.
    fn defmethod_target(&self, file: &str, form: &TopForm) -> Option<(String, String)> {
        self.run
            .analysis
            .var_usages
            .iter()
            .find(|u| {
                u.filename == file
                    && u.defmethod == Some(true)
                    && upos(u).is_some_and(|p| form.span.contains(p))
            })
            .and_then(|u| Some((u.to.clone()?, u.name.clone())))
    }

    /// Creates an impl node `<target_ns>/<method>@<disc>` defined in `ns`,
    /// contained by its multimethod or protocol method when this unit
    /// defines it, otherwise by the namespace holding the code.
    ///
    /// Example: `impl_node("shop.pricing", "shop.pricing", "price", ":book", …)`
    /// gives `clj:.//shop.pricing/price@:book`.
    fn impl_node(
        &mut self,
        ns: &str,
        target_ns: &str,
        method: &str,
        disc: &str,
        file: &str,
        span: Span,
    ) -> Option<NodeId> {
        let sym = self.sym_id(target_ns, method)?;
        let id = NodeId::Impl(ImplId::new(sym.clone(), disc).ok()?);
        let lang = lang_of(None, file);
        let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::Impl,
            subkind: None,
            name: format!("{method}@{disc}"),
            namespace: Some(ns.to_owned()),
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: location(
                file,
                span.start.line,
                span.start.col,
                Some(span.end.line),
                Some(span.end.col),
            ),
            doc: None,
            signature: None,
        });
        node.unit = Some(NodeId::Unit(self.unit.dir.clone()));
        node.langs.insert(lang);
        let parent = if self.nodes.contains_key(&NodeId::Symbol(sym.clone())) {
            NodeId::Symbol(sym)
        } else {
            NodeId::Namespace(self.ns_id(ns)?)
        };
        self.contains(parent, id.clone());
        Some(id)
    }

    /// The namespace defined in `file` (the first `ns` form).
    ///
    /// Example: `file_ns("src/shop/core.clj")` is `Some("shop.core")`.
    fn file_ns(&self, file: &str) -> Option<String> {
        self.run
            .analysis
            .namespace_definitions
            .iter()
            .find(|n| n.filename == file)
            .map(|n| n.name.clone())
    }

    /// R2: one `requires` edge per namespace usage, to this unit's namespace
    /// or a namespace-level external.
    ///
    /// Example: `(:require [re-frame.core :as rf])` in shop.ui.events gives
    /// `clj:.//shop.ui.events -requires-> ext:clj//re-frame.core`.
    fn requires(&mut self) {
        for u in &self.run.analysis.namespace_usages {
            let Some(from) = self.ns_id(&u.from).map(NodeId::Namespace) else {
                continue;
            };
            let to = if self.local_ns.contains(&u.to) {
                self.ns_id(&u.to).map(NodeId::Namespace)
            } else {
                self.external(&u.to, None)
            };
            let Some(to) = to else { continue };
            let site = site(&u.filename, u.row, u.col);
            let lang = u.lang.as_deref().and_then(Lang::from_token);
            self.edge(from, to, EdgeKind::Requires, None, None, lang, site);
        }
    }

    /// R3 and R4 for every var usage.
    ///
    /// Example: `(pricing/price order)` inside `checkout` gives a certain
    /// direct `calls` edge `checkout -> price`.
    fn usages(&mut self) {
        let usages: &'a [VarUsage] = &self.run.analysis.var_usages;
        for u in usages {
            self.stats.usages += 1;
            if let Some(s) = self.suppression(u) {
                *self.stats.suppressed.entry(s.name()).or_default() += 1;
                continue;
            }
            let Some(pos) = upos(u) else {
                self.positionless(u);
                continue;
            };
            let scope = self.scope(u);
            let target = self.target(u);
            let (Some(scope), Some((to, interop))) = (scope.clone(), target) else {
                let in_template = scope.is_some_and(|s| self.in_macro_quote(&s, &u.filename, pos));
                if in_template {
                    let name = Suppression::MacroTemplate.name();
                    *self.stats.suppressed.entry(name).or_default() += 1;
                } else {
                    self.unresolved(u);
                }
                continue;
            };
            let is_call = self.is_call(u);
            let in_macro_quote = self.in_macro_quote(&scope, &u.filename, pos);
            let (kind, certainty, reason) = if in_macro_quote {
                (EdgeKind::References, Certainty::Possible, Reason::MacroBody)
            } else if interop {
                let kind = if is_call {
                    EdgeKind::Calls
                } else {
                    EdgeKind::References
                };
                (kind, Certainty::Certain, Reason::Interop)
            } else if is_call {
                (EdgeKind::Calls, Certainty::Certain, Reason::Direct)
            } else {
                (EdgeKind::References, Certainty::Likely, Reason::ValueRef)
            };
            let lang = u.lang.as_deref().and_then(Lang::from_token);
            let site = site_of(u);
            self.stats.sites += 1;
            let subkind = self.nodes.get(&to).and_then(|n| n.subkind);
            if matches!(
                subkind,
                Some(SymbolSubkind::Multimethod | SymbolSubkind::ProtocolMethod)
            ) {
                let uses = if subkind == Some(SymbolSubkind::Multimethod) {
                    &mut self.multi_uses
                } else {
                    &mut self.method_uses
                };
                uses.push(MultiUse {
                    scope: scope.clone(),
                    multi: to.clone(),
                    file: u.filename.clone(),
                    pos,
                    is_call,
                    lang,
                    site: site.clone(),
                });
            }
            self.edge(scope, to, kind, Some(certainty), Some(reason), lang, site);
        }
    }

    /// Whether a usage is one of the documented non-uses.
    ///
    /// Example: the `defn` in `(defn f [] 1)` gives `Some(DefinitionForm)`.
    fn suppression(&self, u: &VarUsage) -> Option<Suppression> {
        if u.defmethod == Some(true) {
            return Some(Suppression::DefmethodTarget);
        }
        if u.refer == Some(true) {
            return Some(Suppression::NsRefer);
        }
        let ix = self.indexes.get(u.filename.as_str())?;
        let pos = upos(u)?;
        let name_pos = uname_pos(u)?;
        let top = ix.forms.iter().find(|f| f.span.contains(pos));
        if let Some(top) = top {
            let head = local(top.head.as_deref());
            if top.span.start == pos
                && DEFINITION_HEADS.contains(&head)
                && local(Some(&u.name)) == head
            {
                return Some(Suppression::DefinitionForm);
            }
            if head == "defprotocol" && u.to.as_deref() == Some(UNKNOWN_NS) {
                return Some(Suppression::ProtocolSignature);
            }
        }
        let in_impl_header = ix.calls.iter().any(|c| {
            matches!(
                local(Some(&c.head)),
                "extend-type" | "extend-protocol" | "defrecord" | "deftype" | "reify"
            ) && c.args.iter().any(|a| a.start == name_pos)
        });
        let in_body = ix.impls.iter().any(|b| b.span.contains(name_pos));
        if in_impl_header && !in_body {
            return Some(Suppression::ProtocolInImplForm);
        }
        None
    }

    /// R3: the node a usage belongs to. In order: a `defmethod` body, a
    /// protocol method body (not `reify`), clj-kondo's `from-var`, the
    /// innermost var definition containing it, else a `<top>@L<line>` impl
    /// for its top-level form.
    ///
    /// Example: the `println` at top level on line 10 of shop.core gives
    /// `clj:.//shop.core/<top>@L10`.
    fn scope(&mut self, u: &VarUsage) -> Option<NodeId> {
        let pos = upos(u)?;
        let ix = *self.indexes.get(u.filename.as_str())?;
        if let Some(top) = ix.forms.iter().find(|f| f.span.contains(pos))
            && let Some(id) = self
                .defmethod_impls
                .get(&(u.filename.clone(), top.span.start))
        {
            return Some(id.clone());
        }
        if let Some((_, _, id)) = self
            .protocol_impls
            .iter()
            .filter(|(f, span, _)| f == &u.filename && span.contains(pos))
            .min_by_key(|(_, span, _)| span.end_byte - span.start_byte)
        {
            return Some(id.clone());
        }
        if let Some(var) = &u.from_var {
            return self.symbol_or_placeholder(&u.from, var, u);
        }
        let innermost = self
            .run
            .analysis
            .var_definitions
            .iter()
            .filter(|d| d.filename == u.filename && def_span_contains(d, pos))
            .min_by_key(|d| {
                (
                    d.end_row.unwrap_or(d.row) - d.row,
                    d.end_col.unwrap_or(d.col),
                )
            });
        if let Some(def) = innermost {
            return self.sym_id(&def.ns, &def.name).map(NodeId::Symbol);
        }
        let top = ix.forms.iter().find(|f| f.span.contains(pos))?;
        self.top_impl(&u.from, &u.filename, top)
    }

    /// The `<ns>/<top>@L<line>` impl for a top-level form, created on first use.
    ///
    /// Example: the form on line 10 of shop.core gives `clj:.//shop.core/<top>@L10`.
    fn top_impl(&mut self, ns: &str, file: &str, top: &TopForm) -> Option<NodeId> {
        let disc = format!("L{}", top.span.start.line);
        self.impl_node(ns, ns, "<top>", &disc, file, top.span)
    }

    /// The symbol `ns/name` in this unit; if clj-kondo names a var that has no
    /// definition (made by a macro the analyzer cannot expand), a bare
    /// symbol node plus an unresolved-symbol blind spot.
    ///
    /// Example: a usage inside `(defthing widget …)` with `from-var` widget
    /// gives a `var` node `widget` without a location.
    fn symbol_or_placeholder(&mut self, ns: &str, name: &str, u: &VarUsage) -> Option<NodeId> {
        let id = NodeId::Symbol(self.sym_id(ns, name)?);
        if !self.nodes.contains_key(&id) {
            self.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    kind: NodeKind::Symbol,
                    subkind: Some(SymbolSubkind::Var),
                    name: name.to_owned(),
                    namespace: Some(ns.to_owned()),
                    unit: Some(NodeId::Unit(self.unit.dir.clone())),
                    langs: [lang_of(u.lang.as_deref(), &u.filename)].into(),
                    flags: BTreeSet::new(),
                    loc: None,
                    doc: None,
                    signature: None,
                },
            );
            self.blindspots.push(BlindSpot {
                category: BlindSpotCategory::UnresolvedSymbol,
                node: Some(id.clone()),
                site: site_of(u),
                message: format!("{ns}/{name} is used but has no definition the analyzer can see (likely defined by a macro)"),
            });
            if let Some(ns_id) = self.ns_id(ns) {
                self.contains(NodeId::Namespace(ns_id), id.clone());
            }
        }
        Some(id)
    }

    /// R4 target of a usage and whether it is interop. `None` when clj-kondo
    /// could not resolve it.
    ///
    /// Example: `str/join` with `to: "clojure.string"` gives
    /// `(ext:clj//clojure.string/join, false)`; `js/console.log` gives
    /// `(js:console.log, true)`.
    fn target(&mut self, u: &VarUsage) -> Option<(NodeId, bool)> {
        match u.to.as_deref() {
            None => {
                let path = u.name.strip_prefix("js/")?;
                let id = NodeId::Platform(PlatformId::js(path).ok()?);
                self.nodes
                    .entry(id.clone())
                    .or_insert_with(|| platform_node(&id, &u.name));
                Some((id, true))
            }
            Some(UNKNOWN_NS) => None,
            Some(ns) if self.local_ns.contains(ns) => {
                let name = u.name.clone();
                let ns = ns.to_owned();
                self.symbol_or_placeholder(&ns, &name, u)
                    .map(|id| (id, false))
            }
            Some(ns) => self.external(ns, Some(&u.name)).map(|id| (id, false)),
        }
    }

    /// An external node (namespace-level when `name` is `None`), created on first use.
    ///
    /// Example: `external("clojure.core", Some("map"))` gives `ext:clj//clojure.core/map`.
    fn external(&mut self, ns: &str, name: Option<&str>) -> Option<NodeId> {
        let ext = match name {
            Some(n) => ExternalId::new(Lang::Clj, ns, n).ok()?,
            None => ExternalId::namespace(Lang::Clj, ns).ok()?,
        };
        let id = NodeId::External(ext);
        self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::External,
            subkind: None,
            name: name.unwrap_or(ns).to_owned(),
            namespace: name.map(|_| ns.to_owned()),
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
        });
        Some(id)
    }

    /// A usage clj-kondo reports without a position (inside some macro
    /// expansions, e.g. `cond->>`): a certain call from its `from-var`, with
    /// no site, counted in [`UsageStats::positionless`]. Without a
    /// `from-var` it cannot be placed and becomes a blind spot.
    ///
    /// Example: `(defn f [xs] (cond->> xs true (map inc)))` gives
    /// `f -calls-> ext:clj//clojure.core/->>` without sites.
    fn positionless(&mut self, u: &VarUsage) {
        let scope = match &u.from_var {
            Some(var) => self.symbol_or_placeholder(&u.from, var, u),
            None => None,
        };
        let (Some(scope), Some((to, interop))) = (scope, self.target(u)) else {
            self.unresolved(u);
            return;
        };
        self.stats.positionless += 1;
        let reason = if interop {
            Reason::Interop
        } else {
            Reason::Direct
        };
        let lang = u.lang.as_deref().and_then(Lang::from_token);
        self.edge(
            scope,
            to,
            EdgeKind::Calls,
            Some(Certainty::Certain),
            Some(reason),
            lang,
            None,
        );
    }

    /// Records an unresolved usage as a blind spot.
    ///
    /// Example: `(frobnicate x)` with no such var gives an `unresolved-symbol`
    /// blind spot at its position.
    fn unresolved(&mut self, u: &VarUsage) {
        self.stats.unresolved += 1;
        self.blindspots.push(BlindSpot {
            category: BlindSpotCategory::UnresolvedSymbol,
            node: None,
            site: site_of(u),
            message: format!("{} could not be resolved", u.name),
        });
    }

    /// R4: a usage is a call when clj-kondo puts it at an opening paren
    /// (its position differs from its name's) and the form index has a list
    /// starting there. Reader macros that expand to calls count too: `@x`
    /// (`deref`), `#(…)` (`fn*`), `#'x` (`var`), and `(.method x)` (`.`).
    ///
    /// Example: `(f x)` is a call to `f`; `(map f xs)` makes `f` a reference;
    /// `@stock` is a call to `deref`.
    fn is_call(&self, u: &VarUsage) -> bool {
        let (Some(pos), Some(name_pos)) = (upos(u), uname_pos(u)) else {
            return false;
        };
        let core = matches!(u.to.as_deref(), Some("clojure.core" | "cljs.core"));
        if pos == name_pos && core && matches!(u.name.as_str(), "deref" | "fn*" | "var") {
            return true;
        }
        let Some(ix) = self.indexes.get(u.filename.as_str()) else {
            return false;
        };
        let call_here = ix.calls.iter().find(|c| c.span.start == pos);
        if core && u.name == "." {
            return call_here.is_some_and(|c| c.head.starts_with('.'));
        }
        pos != name_pos && call_here.is_some()
    }

    /// Whether `pos` is inside a syntax quote and `scope` is a macro.
    ///
    /// Example: `now-ms` inside `` `(let [s# (now-ms)] …) `` in `with-timing` gives `true`.
    fn in_macro_quote(&self, scope: &NodeId, file: &str, pos: Pos) -> bool {
        let is_macro = self
            .nodes
            .get(scope)
            .is_some_and(|n| n.subkind == Some(SymbolSubkind::Macro));
        is_macro
            && self
                .indexes
                .get(file)
                .is_some_and(|ix| ix.quoted.iter().any(|q| q.contains(pos)))
    }

    /// R5: `dispatches` edges from each caller of a multimethod to the impls
    /// that call may run. A keyword dispatch function (or `(fn [x] (:k x))`)
    /// with a literal map argument selects the impl whose dispatch value
    /// equals the map's value (`likely`, plus `:default` as `possible`); a
    /// value that matches no impl selects `:default` (`likely`); a value with
    /// a `derive` parent, any other argument or dispatch function, or a usage
    /// as a value links every impl, `possible`.
    ///
    /// Example: `(area {:shape :circle})` gives `area@:circle` likely and
    /// `area@:default` possible.
    fn multimethod_dispatch(&mut self) {
        let derived = self.derived_children();
        let uses = std::mem::take(&mut self.multi_uses);
        for u in &uses {
            let Some(impls) = self.multi_impls.get(&u.multi).cloned() else {
                continue;
            };
            let default = impls
                .iter()
                .find(|(_, d)| d == ":default")
                .map(|(id, _)| id.clone());
            let all = || {
                impls
                    .iter()
                    .map(|(id, _)| (id.clone(), Certainty::Possible))
                    .collect::<Vec<_>>()
            };
            let value = if u.is_call {
                self.dispatch_keyword(&u.multi)
                    .and_then(|k| self.literal_entry(&u.file, u.pos, &k))
            } else {
                None
            };
            let targets = match value {
                Some(v) if derived.contains(&v) => all(),
                Some(v) => match impls.iter().find(|(_, d)| *d == v) {
                    Some((hit, _)) => {
                        let mut t = vec![(hit.clone(), Certainty::Likely)];
                        if let Some(d) = default.as_ref().filter(|d| *d != hit) {
                            t.push((d.clone(), Certainty::Possible));
                        }
                        t
                    }
                    None => match &default {
                        Some(d) => vec![(d.clone(), Certainty::Likely)],
                        None => all(),
                    },
                },
                None => all(),
            };
            for (target, certainty) in targets {
                self.edge(
                    u.scope.clone(),
                    target,
                    EdgeKind::Dispatches,
                    Some(certainty),
                    Some(Reason::MultimethodDispatch),
                    u.lang,
                    u.site.clone(),
                );
            }
        }
    }

    /// R6: `dispatches` edges from each caller of a protocol method to its
    /// impls. A receiver (first argument) that is a constructor call, or a
    /// local bound to one in an enclosing `let`-style form (`->Rec`,
    /// `map->Rec`, `Rec.`), selects that type's impl, `likely`; anything else,
    /// and a usage as a value, links every impl, `possible`.
    ///
    /// Example: `(let [c (->Circle 2)] (area c))` gives `area@Circle` likely.
    fn protocol_dispatch(&mut self) {
        let uses = std::mem::take(&mut self.method_uses);
        for u in &uses {
            let Some(impls) = self.method_impls.get(&u.multi).cloned() else {
                continue;
            };
            let receiver = if u.is_call {
                self.receiver_type(&u.file, u.pos)
            } else {
                None
            };
            let hit = receiver.and_then(|t| impls.iter().find(|(_, d)| *d == t).cloned());
            let targets: Vec<(NodeId, Certainty)> = match hit {
                Some((id, _)) => vec![(id, Certainty::Likely)],
                None => impls
                    .iter()
                    .map(|(id, _)| (id.clone(), Certainty::Possible))
                    .collect(),
            };
            for (target, certainty) in targets {
                self.edge(
                    u.scope.clone(),
                    target,
                    EdgeKind::Dispatches,
                    Some(certainty),
                    Some(Reason::ProtocolDispatch),
                    u.lang,
                    u.site.clone(),
                );
            }
        }
    }

    /// The type a call's first argument is known to have: a constructor call,
    /// or a local bound to one by the innermost enclosing binding form.
    ///
    /// Example: in `(let [c (s/->Circle 2)] (s/area c))`, the call to `area`
    /// gives `Some("Circle")`.
    fn receiver_type(&self, file: &str, pos: Pos) -> Option<String> {
        let ix = self.indexes.get(file)?;
        let call = ix.calls.iter().find(|c| c.span.start == pos)?;
        let first = call.arg_info.first()?;
        match first.kind.as_str() {
            "list_lit" => constructor_type(&first.text),
            "sym_lit" => ix
                .calls
                .iter()
                .filter(|c| {
                    matches!(
                        local(Some(&c.head)),
                        "let"
                            | "when-let"
                            | "if-let"
                            | "loop"
                            | "binding"
                            | "when-some"
                            | "if-some"
                    ) && c.span.contains(pos)
                })
                .max_by_key(|c| c.span.start_byte)
                .and_then(|c| c.arg_info.first())
                .and_then(|v| v.entries.iter().rev().find(|(name, _)| *name == first.text))
                .and_then(|(_, init)| constructor_type(init)),
            _ => None,
        }
    }

    /// Adds `impl implements target`, where the target is this unit's symbol
    /// or, for a protocol or multimethod defined elsewhere, an external.
    ///
    /// Example: `implements(area@Circle, "pr.shapes", "Shape")`.
    fn implements(&mut self, impl_id: NodeId, target_ns: &str, name: &str) {
        let target = if self.local_ns.contains(target_ns) {
            self.sym_id(target_ns, name).map(NodeId::Symbol)
        } else {
            self.external(target_ns, Some(name))
        };
        if let Some(target) = target {
            self.edge(
                impl_id,
                target,
                EdgeKind::Implements,
                None,
                None,
                None,
                None,
            );
        }
    }

    /// Keywords that appear as the child in a `(derive child parent)` call
    /// (or `(derive h child parent)`) anywhere in this unit.
    ///
    /// Example: `(derive :a/poodle :a/dog)` gives `{":a/poodle"}`.
    fn derived_children(&self) -> BTreeSet<String> {
        self.indexes
            .values()
            .flat_map(|ix| ix.calls.iter())
            .filter(|c| local(Some(&c.head)) == "derive")
            .filter_map(|c| match c.arg_info.len() {
                2 => c.arg_info.first(),
                3 => c.arg_info.get(1),
                _ => None,
            })
            .map(|a| a.text.clone())
            .collect()
    }

    /// The keyword a multimethod dispatches on, if its dispatch function is a
    /// keyword or `(fn [x] (:k x))`.
    ///
    /// Example: `(defmulti area :shape)` gives `Some(":shape")`;
    /// `(defmulti describe (fn [x y] …))` gives `None`.
    fn dispatch_keyword(&self, multi: &NodeId) -> Option<String> {
        let NodeId::Symbol(sym) = multi else {
            return None;
        };
        let def = self
            .run
            .analysis
            .var_definitions
            .iter()
            .find(|d| d.ns == sym.namespace().ns() && d.name == sym.name())?;
        let ix = self.indexes.get(def.filename.as_str())?;
        let start = Pos {
            line: def.row,
            col: def.col,
        };
        let defmulti = ix.calls.iter().find(|c| c.span.start == start)?;
        let (i, arg) = defmulti
            .arg_info
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, a)| !matches!(a.kind.as_str(), "str_lit" | "map_lit"))?;
        match arg.kind.as_str() {
            "kwd_lit" => Some(arg.text.clone()),
            "list_lit" => {
                let fn_start = defmulti.args.get(i)?.start;
                let f = ix
                    .calls
                    .iter()
                    .find(|c| c.span.start == fn_start && local(Some(&c.head)) == "fn")?;
                let body = f.args.get(1)?;
                let call = ix.calls.iter().find(|c| c.span.start == body.start)?;
                call.head.starts_with(':').then(|| call.head.clone())
            }
            _ => None,
        }
    }

    /// The value under `key` in the literal map that is the first argument of
    /// the call at `pos`, if there is one.
    ///
    /// Example: for `(area {:shape :circle})`, key `:shape` gives `Some(":circle")`.
    fn literal_entry(&self, file: &str, pos: Pos, key: &str) -> Option<String> {
        let ix = self.indexes.get(file)?;
        let call = ix.calls.iter().find(|c| c.span.start == pos)?;
        let first = call.arg_info.first().filter(|a| a.kind == "map_lit")?;
        first
            .entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    /// R4: one `macro-generated` blind spot per macro defined here whose
    /// body syntax-quotes a call.
    ///
    /// Example: `with-timing` gives one blind spot.
    fn macro_blind_spots(&mut self) {
        for def in &self.run.analysis.var_definitions {
            if subkind(def) != SymbolSubkind::Macro {
                continue;
            }
            let Some(ix) = self.indexes.get(def.filename.as_str()) else {
                continue;
            };
            let start = Pos {
                line: def.row,
                col: def.col,
            };
            let Some(form) = ix.forms.iter().find(|f| f.span.start == start) else {
                continue;
            };
            let quotes_a_call = ix.quoted.iter().any(|q| {
                form.span.contains(q.start) && ix.calls.iter().any(|c| q.contains(c.span.start))
            });
            if !quotes_a_call {
                continue;
            }
            let Some(sym) = self.sym_id(&def.ns, &def.name) else {
                continue;
            };
            let spot = BlindSpot {
                category: BlindSpotCategory::MacroGenerated,
                node: Some(NodeId::Symbol(sym)),
                site: site(&def.filename, def.row, def.col),
                message: format!(
                    "calls made by expansions of {}/{} are not visible",
                    def.ns, def.name
                ),
            };
            if !self.blindspots.contains(&spot) {
                self.blindspots.push(spot);
            }
        }
    }

    /// Adds `from contains to`.
    ///
    /// Example: `contains(ns, sym)`.
    fn contains(&mut self, from: NodeId, to: NodeId) {
        let key = (from.clone(), to.clone(), EdgeKind::Contains, None, None);
        self.edges.entry(key).or_insert(Edge {
            from,
            to,
            kind: EdgeKind::Contains,
            certainty: None,
            reason: None,
            lang: None,
            sites: Vec::new(),
            count: None,
            by_certainty: None,
        });
    }

    /// Adds a site to the edge `(from, to, kind, reason, lang)`, creating it.
    ///
    /// Example: two calls from `f` to `g` give one edge with two sites.
    #[allow(clippy::too_many_arguments)]
    fn edge(
        &mut self,
        from: NodeId,
        to: NodeId,
        kind: EdgeKind,
        certainty: Option<Certainty>,
        reason: Option<Reason>,
        lang: Option<Lang>,
        site: Option<Site>,
    ) {
        let key = (from.clone(), to.clone(), kind, reason, lang);
        let edge = self.edges.entry(key).or_insert(Edge {
            from,
            to,
            kind,
            certainty,
            reason,
            lang,
            sites: Vec::new(),
            count: None,
            by_certainty: None,
        });
        if let Some(site) = site {
            if edge.sites.contains(&site) {
                if matches!(kind, EdgeKind::Calls | EdgeKind::References) {
                    self.stats.merged_duplicates += 1;
                }
            } else {
                edge.sites.push(site);
            }
        }
    }

    /// Assembles the batch: nodes and edges sorted, sites sorted, literals,
    /// diagnostics from clj-kondo and the form index (one per file).
    ///
    /// Example: clj-mini gives a batch whose first node is `clj:.//shop.audit`.
    fn finish(self) -> Resolved {
        let mut edges: Vec<Edge> = self.edges.into_values().collect();
        for e in &mut edges {
            e.sites.sort();
        }
        let mut diagnostics: Vec<Diagnostic> = self.run.diagnostics.clone();
        let mut literals = Vec::new();
        for ix in self.indexes.values() {
            for d in &ix.diagnostics {
                if !diagnostics
                    .iter()
                    .any(|x| x.path == d.path && x.code == d.code)
                {
                    diagnostics.push(d.clone());
                }
            }
            for l in &ix.literals {
                literals.push(LiteralFact {
                    file: ix.path.clone(),
                    line: l.span.start.line,
                    col: l.span.start.col,
                    kind: match l.kind {
                        LiteralKind::Str => "string",
                        LiteralKind::Keyword => "keyword",
                        LiteralKind::Number => "number",
                        LiteralKind::Regex => "regex",
                    },
                    text: l.text.clone(),
                });
            }
        }
        diagnostics.sort();
        literals.sort();
        let mut blindspots = self.blindspots;
        blindspots.sort();
        blindspots.dedup();
        Resolved {
            batch: FactBatch {
                nodes: self.nodes.into_values().collect(),
                edges,
                literals,
                diagnostics,
                blindspots,
            },
            stats: self.stats,
        }
    }
}

/// R1 subkind from `defined-by` (and the name for record constructors).
///
/// Example: `defined-by: clojure.core/defrecord`, name `->Card` gives `Fn`;
/// name `Card` gives `Record`.
fn subkind(def: &VarDefinition) -> SymbolSubkind {
    let by = def
        .defined_by_lint_as
        .as_deref()
        .or(def.defined_by.as_deref())
        .map(|s| local(Some(s)))
        .unwrap_or("");
    if def.test == Some(true) {
        return SymbolSubkind::Test;
    }
    match by {
        "defn" | "defn-" | "fn" => SymbolSubkind::Fn,
        "defmacro" => SymbolSubkind::Macro,
        "defmulti" => SymbolSubkind::Multimethod,
        "defprotocol" if def.protocol_name.is_some() => SymbolSubkind::ProtocolMethod,
        "defprotocol" => SymbolSubkind::Protocol,
        "defrecord" | "deftype" if def.name.starts_with("->") || def.name.starts_with("map->") => {
            SymbolSubkind::Fn
        }
        "defrecord" => SymbolSubkind::Record,
        "deftype" => SymbolSubkind::Type,
        "definterface" => SymbolSubkind::Interface,
        "deftest" => SymbolSubkind::Test,
        "def" | "defonce" => SymbolSubkind::Var,
        _ if def.arglist_strs.as_ref().is_some_and(|a| !a.is_empty()) => SymbolSubkind::Fn,
        _ => SymbolSubkind::Var,
    }
}

/// The part of a symbol after its namespace; `""` for `None`.
///
/// Example: `local(Some("clojure.core/defn"))` is `"defn"`; `local(Some("/"))` is `"/"`.
fn local(sym: Option<&str>) -> &str {
    let Some(sym) = sym else { return "" };
    match sym.rsplit_once('/') {
        Some((_, n)) if !n.is_empty() => n,
        _ => sym,
    }
}

/// The language of a record: clj-kondo's `lang` for `.cljc`, else the extension.
///
/// Example: `lang_of(None, "src/a.cljs")` is `Cljs`; `lang_of(Some("clj"), "a.cljc")` is `Clj`.
fn lang_of(lang: Option<&str>, file: &str) -> Lang {
    lang.and_then(Lang::from_token)
        .unwrap_or(if file.ends_with(".cljs") {
            Lang::Cljs
        } else {
            Lang::Clj
        })
}

/// A location from clj-kondo coordinates; `None` if the file is not a valid path.
///
/// Example: `location("src/a.clj", 1, 1, Some(3), Some(9))`.
fn location(
    file: &str,
    line: u32,
    col: u32,
    end_line: Option<u32>,
    end_col: Option<u32>,
) -> Option<Location> {
    Some(Location {
        file: RepoPath::parse(file).ok()?,
        line,
        col,
        end_line: end_line.unwrap_or(line),
        end_col: end_col.unwrap_or(col),
    })
}

/// The record or type a constructor call builds: `(->Rec …)`,
/// `(map->Rec …)` and `(Rec. …)` give `Rec`; anything else `None`.
///
/// Example: `constructor_type("(s/->Circle 2)")` is `Some("Circle")`.
fn constructor_type(text: &str) -> Option<String> {
    let inner = text.strip_prefix('(')?;
    let head: String = inner
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ')')
        .collect();
    let name = local(Some(&head));
    if let Some(t) = name
        .strip_prefix("map->")
        .or_else(|| name.strip_prefix("->"))
    {
        return (!t.is_empty()).then(|| t.to_owned());
    }
    let t = head.strip_suffix('.')?;
    (!t.is_empty()).then(|| t.to_owned())
}

/// A usage's position, if clj-kondo gave one.
///
/// Example: a usage at row 3, col 5 gives `Some(Pos { line: 3, col: 5 })`.
fn upos(u: &VarUsage) -> Option<Pos> {
    Some(Pos {
        line: u.row?,
        col: u.col?,
    })
}

/// A usage's name position, falling back to its position.
///
/// Example: `(f x)` at 3:5 has name position 3:6.
fn uname_pos(u: &VarUsage) -> Option<Pos> {
    let pos = upos(u)?;
    Some(Pos {
        line: u.name_row.unwrap_or(pos.line),
        col: u.name_col.unwrap_or(pos.col),
    })
}

/// A usage's site, if it has a position and a valid file.
///
/// Example: a usage in `src/a.clj` at 3:5 gives that site.
fn site_of(u: &VarUsage) -> Option<Site> {
    let pos = upos(u)?;
    site(&u.filename, pos.line, pos.col)
}

/// A site; `None` if the file is not a valid path.
///
/// Example: `site("src/a.clj", 3, 5)`.
fn site(file: &str, line: u32, col: u32) -> Option<Site> {
    Some(Site {
        file: RepoPath::parse(file).ok()?,
        line,
        col,
    })
}

/// Whether a var definition's range contains `pos`.
///
/// Example: a def spanning 3:1–5:20 contains 4:7.
fn def_span_contains(d: &VarDefinition, pos: Pos) -> bool {
    let start = Pos {
        line: d.row,
        col: d.col,
    };
    let end = Pos {
        line: d.end_row.unwrap_or(d.row),
        col: d.end_col.unwrap_or(d.col),
    };
    start <= pos && pos < end
}

/// A platform node for a JS global.
///
/// Example: `js:console.log` named `js/console.log`.
fn platform_node(id: &NodeId, name: &str) -> Node {
    Node {
        id: id.clone(),
        kind: NodeKind::Platform,
        subkind: None,
        name: name.to_owned(),
        namespace: None,
        unit: None,
        langs: BTreeSet::new(),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
    }
}
