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
    LibraryId, Location, NamespaceId, Node, NodeId, NodeKind, PlatformId, Reason, RepoPath, Site,
    SymbolFlag, SymbolId, SymbolSubkind, UnitInfo,
};
use aldente_rules::RulesError;
use aldente_rules::clj::{self as rules_clj, EntryPoints, Namespaces, Tests};
use aldente_rules::keyword_dispatch::{self as kd, Catalog};

use crate::forms::{Arity, FileIndex, LiteralKind, Pos, Shape, Span, TopForm};
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
    /// The interop special forms `.` and `new`; R9 links the real target.
    InteropForm,
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
            Suppression::InteropForm => "interop-form",
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

/// The built-in rule catalogs the resolver applies.
#[derive(Debug, Clone)]
pub struct Rules {
    pub keyword: Catalog,
    pub namespaces: Namespaces,
    pub tests: Tests,
    pub entry_points: EntryPoints,
}

impl Rules {
    /// The catalogs compiled into the binary.
    ///
    /// Example: `Rules::builtin()?.namespaces.core_for("clojure.set")`.
    pub fn builtin() -> Result<Rules, RulesError> {
        Ok(Rules {
            keyword: kd::builtin()?,
            namespaces: rules_clj::namespaces()?,
            tests: rules_clj::tests()?,
            entry_points: rules_clj::entry_points()?,
        })
    }
}

/// What the resolver needs besides one unit's analysis.
#[derive(Debug, Clone)]
pub struct Context {
    pub rules: Rules,
    /// Namespace → Maven coordinate from the jars of the unit's declared
    /// dependencies in the local Maven cache (R10 rule 2); empty when the
    /// cache is absent or not consulted.
    pub jars: BTreeMap<String, String>,
}

impl Context {
    /// The built-in rules and no jar listings.
    ///
    /// Example: `Context::builtin()?.jars.is_empty()` is `true`.
    pub fn builtin() -> Result<Context, RulesError> {
        Ok(Context {
            rules: Rules::builtin()?,
            jars: BTreeMap::new(),
        })
    }
}

/// Resolves one unit.
///
/// Example: on clj-mini gives `clj:.//shop.core/checkout` (fn) with a
/// certain `calls` edge to `clj:.//shop.pricing/price` at `src/shop/core.clj:16:17`.
pub fn resolve(unit: &UnitInfo, run: &KondoRun, indexes: &[FileIndex], ctx: &Context) -> Resolved {
    let mut r = Resolver::new(unit, run, indexes, ctx);
    r.namespaces();
    r.definitions();
    r.impls();
    r.registrations();
    r.requires();
    r.usages();
    r.keyword_dispatch();
    r.multimethod_dispatch();
    r.protocol_dispatch();
    r.param_flow();
    r.interop();
    r.macro_blind_spots();
    r.libraries();
    r.server_handlers();
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
    /// The R11 keyword-dispatch catalog.
    catalog: &'a Catalog,
    /// All rule catalogs and the jar listings.
    ctx: &'a Context,
    /// R11 registration calls `(file, call span, impl)`; code inside is
    /// scoped to the impl.
    reg_impls: Vec<(String, Span, NodeId)>,
    /// R11 handlers by `(family, keyword)`.
    handlers: BTreeMap<(String, String), BTreeSet<NodeId>>,
    /// The family of each R11 registration impl.
    reg_family: BTreeMap<NodeId, String>,
    /// R11 dispatches waiting to be linked.
    kw_uses: Vec<KwUse>,
}

/// One R11 dispatch: who names which key of which family, and where.
#[derive(Debug, Clone)]
struct KwUse {
    scope: NodeId,
    family: String,
    /// The keyword, resolved (`::a` → `:ns/a`); `None` when computed.
    key: Option<String>,
    /// What does the dispatching, for messages: `dispatch`, `:dispatch`.
    by: String,
    ns: String,
    file: String,
    pos: Pos,
    lang: Option<Lang>,
    /// A key that names no handler is data, not a blind spot (effect maps).
    quiet: bool,
}

/// A dispatch found in an effect: `(family, key, quiet)`.
type FoundKey = (String, Option<String>, bool);

/// Where a hole goes and what made it.
struct HoleAt<'s> {
    /// What could not be seen through: `eval`, `dispatch`, `:dispatch`.
    name: &'s str,
    ns: &'s str,
    file: &'s str,
    lang: Option<Lang>,
    pos: Pos,
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
    fn new(
        unit: &'a UnitInfo,
        run: &'a KondoRun,
        indexes: &'a [FileIndex],
        ctx: &'a Context,
    ) -> Resolver<'a> {
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
            catalog: &ctx.rules.keyword,
            ctx,
            reg_impls: Vec::new(),
            handlers: BTreeMap::new(),
            reg_family: BTreeMap::new(),
            kw_uses: Vec::new(),
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
            let is_test = self.is_test_namespace(&def.name, &def.filename);
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
            if is_test {
                node.flags.insert(SymbolFlag::Test);
            }
        }
    }

    /// R14: a namespace in a file under one of the unit's test roots, or
    /// whose name marks it (`rules/clj-tests.toml`).
    ///
    /// Example: `app.thing-test` anywhere, or `app.helpers` in `test/app/helpers.clj`.
    fn is_test_namespace(&self, ns: &str, file: &str) -> bool {
        let under_test_root = RepoPath::parse(file).is_ok_and(|f| {
            self.unit
                .roots
                .iter()
                .any(|r| r.kind == aldente_model::RootKind::Test && f.starts_with(&r.path))
        });
        under_test_root || self.ctx.rules.tests.is_test_namespace(ns)
    }

    /// R14: whether a var is a test: clj-kondo says so, or its defining form
    /// is in `rules/clj-tests.toml`.
    ///
    /// Example: `(defspec p 10 …)` is a test; `(defn f [] …)` is not.
    fn is_test_var(&self, def: &VarDefinition) -> bool {
        let by = [def.defined_by_lint_as.as_deref(), def.defined_by.as_deref()];
        def.test == Some(true)
            || by.into_iter().flatten().any(|b| {
                let name = local(Some(b));
                self.ctx.rules.tests.definers.iter().any(|d| d == name)
            })
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
            let sub = if self.is_test_var(def) {
                SymbolSubkind::Test
            } else {
                subkind(def)
            };
            let id = NodeId::Symbol(sym);
            let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
                id: id.clone(),
                kind: NodeKind::Symbol,
                subkind: Some(sub),
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
        // R13: a manifest names it, it is `-main` or `^:export`, or a test.
        let qualified = format!("{}/{}", def.ns, def.name);
        let export =
            def.meta.as_ref().and_then(|m| m.get("export")) == Some(&serde_json::Value::Bool(true));
        if self.unit.entry_points.contains(&qualified) || def.name == "-main" || export {
            flags.insert(SymbolFlag::EntryPoint);
        }
        if self.is_test_var(def) {
            flags.insert(SymbolFlag::Test);
            flags.insert(SymbolFlag::EntryPoint);
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
                    // As written: `java.lang.Runnable` is a class; Clojure
                    // protocols are `P` or `ns/P`.
                    let written = body.protocol.as_deref().unwrap_or_default();
                    let dotted_class = written.contains('.') && !written.contains('/');
                    let p = local(Some(&p)).to_owned();
                    let local_protocol = self
                        .sym_id(&protocol_ns, &p)
                        .is_some_and(|s| self.nodes.contains_key(&NodeId::Symbol(s)));
                    if dotted_class || (entry.is_none() && !local_protocol) {
                        // clj-kondo knows no protocol by this name and this
                        // unit defines none: a Java interface or class.
                        let class =
                            self.qualify_class(file, if dotted_class { written } else { &p });
                        self.implements_class(id, &class);
                    } else {
                        self.implements(id, &protocol_ns, &p);
                    }
                }
            }
        }
    }

    /// R11: each call to a catalog registration function with a literal
    /// keyword becomes an impl `<ns>/<fn>@<key>`, a handler of its family;
    /// so do the `defmethod`s of a family's multimethods. `:<-` signals of a
    /// registration are queued as dispatches from it.
    ///
    /// Example: `(rf/reg-event-db :cart/add f)` in shop.ui.events gives
    /// `clj:.//shop.ui.events/reg-event-db@:cart/add`.
    fn registrations(&mut self) {
        let usages: &'a [VarUsage] = &self.run.analysis.var_usages;
        for u in usages {
            let Some(reg) =
                u.to.as_deref()
                    .and_then(|to| self.catalog.registration(to, &u.name))
            else {
                continue;
            };
            let Some(call) = upos(u).and_then(|p| self.call_at(&u.filename, p)) else {
                continue;
            };
            let Some(Shape::Kw(raw)) = call.arg_info.first().map(|a| &a.shape) else {
                continue;
            };
            let key = self.resolve_keyword(&u.filename, &u.from, raw);
            let Some(id) = self.impl_node(&u.from, &u.from, &u.name, &key, &u.filename, call.span)
            else {
                continue;
            };
            self.reg_impls
                .push((u.filename.clone(), call.span, id.clone()));
            self.reg_family.insert(id.clone(), reg.family.clone());
            self.handlers
                .entry((reg.family.clone(), key))
                .or_default()
                .insert(id.clone());
            let Some(marker) = reg.signal_marker.as_deref() else {
                continue;
            };
            let lang = u.lang.as_deref().and_then(Lang::from_token);
            for (i, a) in call.arg_info.iter().enumerate() {
                let (Some(next), Some(span)) = (call.arg_info.get(i + 1), call.args.get(i + 1))
                else {
                    continue;
                };
                if a.text != marker {
                    continue;
                }
                let key = next
                    .shape
                    .head_keyword()
                    .map(|k| self.resolve_keyword(&u.filename, &u.from, k));
                self.kw_uses.push(KwUse {
                    scope: id.clone(),
                    family: reg.family.clone(),
                    key,
                    by: marker.to_owned(),
                    ns: u.from.clone(),
                    file: u.filename.clone(),
                    pos: span.start,
                    lang,
                    quiet: false,
                });
            }
        }
        let catalog: &'a Catalog = self.catalog;
        for family in &catalog.families {
            for q in &family.multimethods {
                let Some(sym) = q.split_once('/').and_then(|(ns, n)| self.sym_id(ns, n)) else {
                    continue;
                };
                for (impl_id, dispatch) in self
                    .multi_impls
                    .get(&NodeId::Symbol(sym))
                    .cloned()
                    .unwrap_or_default()
                {
                    self.handlers
                        .entry((family.name.clone(), dispatch))
                        .or_default()
                        .insert(impl_id);
                }
            }
        }
    }

    /// R11: catalog dispatch calls and effect-map entries, plus the queued
    /// signals, become edges to the handlers of the named key (the family's
    /// edge kind and certainty, reason `keyword-dispatch`). A computed key
    /// gives a hole; a literal key with no handler in this unit gives an
    /// `unresolved-symbol` blind spot.
    ///
    /// Example: `(rf/dispatch [:order/submit])` in `submit-button` gives
    /// `submit-button -calls(likely keyword-dispatch)-> reg-event-fx@:order/submit`.
    fn keyword_dispatch(&mut self) {
        let usages: &'a [VarUsage] = &self.run.analysis.var_usages;
        for u in usages {
            let Some(d) =
                u.to.as_deref()
                    .and_then(|to| self.catalog.dispatch(to, &u.name))
            else {
                continue;
            };
            let Some(pos) = upos(u) else { continue };
            let Some(call) = self.call_at(&u.filename, pos) else {
                continue;
            };
            let Some(scope) = self.scope(u) else { continue };
            let shape = call.arg_info.first().map(|a| &a.shape);
            let raw = match (d.arg, shape) {
                (kd::Arg::Vector, Some(s)) => s.head_keyword(),
                (kd::Arg::Keyword, Some(Shape::Kw(k))) => Some(k.as_str()),
                _ => None,
            };
            self.kw_uses.push(KwUse {
                scope,
                family: d.family.clone(),
                key: raw.map(|k| self.resolve_keyword(&u.filename, &u.from, k)),
                by: u.name.clone(),
                ns: u.from.clone(),
                file: u.filename.clone(),
                pos,
                lang: u.lang.as_deref().and_then(Lang::from_token),
                quiet: false,
            });
        }
        let indexes: Vec<&FileIndex> = self.indexes.values().copied().collect();
        for ix in indexes {
            let file = ix.path.as_str();
            let Some(ns) = self.file_ns(file) else {
                continue;
            };
            for e in &ix.entries {
                let effect = self.catalog.effect(&e.key);
                let inside = self
                    .impl_scope(file, e.pos)
                    .and_then(|id| self.reg_family.get(&id).cloned());
                let maps = inside
                    .as_ref()
                    .map_or(0, |f| self.catalog.effect_maps_inside(f).count());
                if effect.is_none() && maps == 0 {
                    continue;
                }
                let Some(scope) = self.scope_at(file, e.pos, &ns) else {
                    continue;
                };
                let mut found = Vec::new();
                match effect {
                    Some(effect) => self.effect_keys(effect, &e.value, &mut found),
                    None => self.effect_map_keys(inside.as_ref(), &e.key, &mut found),
                }
                for (family, raw, quiet) in found {
                    self.kw_uses.push(KwUse {
                        scope: scope.clone(),
                        family,
                        key: raw.map(|k| self.resolve_keyword(file, &ns, &k)),
                        by: e.key.clone(),
                        ns: ns.clone(),
                        file: file.to_owned(),
                        pos: e.pos,
                        // Like a usage outside `.cljc` (clj-kondo tags only
                        // those), and the index reads a `.cljc` file once.
                        lang: None,
                        quiet,
                    });
                }
            }
        }
        for k in std::mem::take(&mut self.kw_uses) {
            self.link_keyword(k);
        }
    }

    /// The `(family, key, quiet)` dispatches an effect's value makes; key
    /// `None` for a computed one.
    ///
    /// Example: `:dispatch-n [[:a] [e]]` gives `[(event, Some(":a"), false), (event, None, false)]`.
    fn effect_keys(&self, effect: &kd::Effect, value: &Shape, out: &mut Vec<FoundKey>) {
        let one = |s: &Shape| s.head_keyword().map(str::to_owned);
        match (effect.value, value) {
            (kd::EffectValue::Event, s) => out.push((effect.family.clone(), one(s), false)),
            (kd::EffectValue::Events, Shape::Vec(items)) => {
                out.extend(items.iter().map(|s| (effect.family.clone(), one(s), false)));
            }
            (kd::EffectValue::Effects, Shape::Vec(items)) => {
                for item in items {
                    let Shape::Vec(pair) = item else { continue };
                    let [Shape::Kw(k), v, ..] = pair.as_slice() else {
                        continue;
                    };
                    match self.catalog.effect(k) {
                        Some(inner) => self.effect_keys(inner, v, out),
                        None => self.effect_map_keys(Some(&effect.family), k, out),
                    }
                }
            }
            (_, _) => out.push((effect.family.clone(), None, false)),
        }
    }

    /// The quiet dispatches a plain effect key makes inside a handler of
    /// family `inside`, one per applicable effect map.
    ///
    /// Example: `:http-xhrio` inside an event handler gives
    /// `[("re-frame/fx", Some(":http-xhrio"), true)]`.
    fn effect_map_keys(&self, inside: Option<&String>, key: &str, out: &mut Vec<FoundKey>) {
        let Some(inside) = inside else { return };
        for m in self.catalog.effect_maps_inside(inside) {
            out.push((m.family.clone(), Some(key.to_owned()), true));
        }
    }

    /// Turns one R11 dispatch into edges, a hole, or a blind spot.
    ///
    /// Example: a `KwUse` for `:order/submit` with one registered handler
    /// gives one `likely` `calls` edge.
    fn link_keyword(&mut self, k: KwUse) {
        let Some(key) = k.key.clone() else {
            let at = HoleAt {
                name: &k.by,
                ns: &k.ns,
                file: &k.file,
                lang: k.lang,
                pos: k.pos,
            };
            self.hole_at(&k.scope, &at, BlindSpotCategory::DynamicResolve);
            return;
        };
        let Some(family) = self.catalog.family(&k.family) else {
            return;
        };
        let kind = match family.edge {
            kd::EdgeKind::Calls => EdgeKind::Calls,
            kd::EdgeKind::References => EdgeKind::References,
        };
        let certainty = match family.certainty {
            kd::Certainty::Likely => Certainty::Likely,
            kd::Certainty::Possible => Certainty::Possible,
        };
        let targets = self
            .handlers
            .get(&(k.family.clone(), key.clone()))
            .cloned()
            .unwrap_or_default();
        if targets.is_empty() && k.quiet {
            return;
        }
        if targets.is_empty() {
            self.blindspots.push(BlindSpot {
                category: BlindSpotCategory::UnresolvedSymbol,
                node: None,
                site: site(&k.file, k.pos.line, k.pos.col),
                message: format!(
                    "{} names {key}, but nothing in this unit registers it ({})",
                    k.by, k.family
                ),
            });
            return;
        }
        for to in targets {
            self.edge(
                k.scope.clone(),
                to,
                kind,
                Some(certainty),
                Some(Reason::KeywordDispatch),
                k.lang,
                site(&k.file, k.pos.line, k.pos.col),
            );
        }
    }

    /// A keyword with auto-resolution applied: `::a` in ns `x` is `:x/a`;
    /// `::al/a` uses the file's alias `al`. Others are returned as written.
    ///
    /// Example: `resolve_keyword("src/v.cljs", "app.views", "::ev/local")`
    /// with `[app.events :as ev]` gives `":app.events/local"`.
    fn resolve_keyword(&self, file: &str, ns: &str, kw: &str) -> String {
        let Some(rest) = kw.strip_prefix("::") else {
            return kw.to_owned();
        };
        match rest.split_once('/') {
            None => format!(":{ns}/{rest}"),
            Some((alias, name)) => self
                .run
                .analysis
                .namespace_usages
                .iter()
                .find(|n| n.filename == file && n.alias.as_deref() == Some(alias))
                .map(|n| format!(":{}/{name}", n.to))
                .unwrap_or_else(|| kw.to_owned()),
        }
    }

    /// R13: a var passed as the first argument of a server start call in
    /// `rules/clj-entry-points.toml` (`(run-jetty #'app opts)`) is a ring
    /// handler and an entry point.
    ///
    /// Example: `(jetty/run-jetty #'app {:port 8080})` flags `app.core/app`.
    fn server_handlers(&mut self) {
        let usages: &'a [VarUsage] = &self.run.analysis.var_usages;
        let mut handlers = Vec::new();
        for u in usages {
            let starts =
                u.to.as_deref()
                    .is_some_and(|to| self.ctx.rules.entry_points.is_server_start(to, &u.name));
            if !starts {
                continue;
            }
            let Some(arg) = upos(u)
                .and_then(|p| self.call_at(&u.filename, p))
                .and_then(|c| c.args.first())
            else {
                continue;
            };
            handlers.extend(
                usages.iter().filter(|v| {
                    v.filename == u.filename && upos(v).is_some_and(|p| arg.contains(p))
                }),
            );
        }
        for v in handlers {
            if let Some((id @ NodeId::Symbol(_), _)) = self.target(v)
                && let Some(node) = self.nodes.get_mut(&id)
            {
                node.flags.insert(SymbolFlag::EntryPoint);
            }
        }
    }

    /// R10: a `provides` edge into every external namespace (created at
    /// namespace level if only its symbols were used), from the first rung
    /// of the ladder that matches. npm packages already have theirs.
    ///
    /// Example: `ext:clj//clojure.string` gets
    /// `lib:maven:org.clojure/clojure -provides(certain direct)->`.
    fn libraries(&mut self) {
        let provided: BTreeSet<NodeId> = self
            .edges
            .values()
            .filter(|e| e.kind == EdgeKind::Provides)
            .map(|e| e.to.clone())
            .collect();
        let namespaces: BTreeSet<String> = self
            .nodes
            .keys()
            .filter_map(|id| match id {
                NodeId::External(e) => Some(e.ns().to_owned()),
                _ => None,
            })
            .collect();
        for ns in namespaces {
            let Some(ext) = self.external(&ns, None) else {
                continue;
            };
            if provided.contains(&ext) {
                continue;
            }
            let found = self.attribute(&ns);
            if found.is_empty() {
                self.provides(
                    LibraryId::unknown(),
                    &ext,
                    Certainty::Possible,
                    Reason::NameMatch,
                );
                self.blindspots.push(BlindSpot {
                    category: BlindSpotCategory::UnresolvedSymbol,
                    node: Some(ext.clone()),
                    site: None,
                    message: format!(
                        "no declared dependency or known library provides namespace {ns}"
                    ),
                });
            }
            for (coord, certainty, reason) in found {
                if let Ok(lib) = LibraryId::new("maven", &coord) {
                    self.provides(lib, &ext, certainty, reason);
                }
            }
        }
    }

    /// The R10 ladder for one namespace: jar listing (certain), the prefix
    /// table (likely), the core rule (certain), then the declared
    /// coordinates' prefixes (likely; possible for each of a tie).
    ///
    /// Example: `malli.core` with `metosin/malli` declared gives
    /// `[("metosin/malli", Likely, NameMatch)]`.
    fn attribute(&self, ns: &str) -> Vec<(String, Certainty, Reason)> {
        let rules = &self.ctx.rules.namespaces;
        if let Some(c) = self.ctx.jars.get(ns) {
            return vec![(c.clone(), Certainty::Certain, Reason::Direct)];
        }
        if let Some(c) = rules.table_for(ns) {
            return vec![(c.to_owned(), Certainty::Likely, Reason::NameMatch)];
        }
        if let Some(c) = rules.core_for(ns) {
            return vec![(c.to_owned(), Certainty::Certain, Reason::Direct)];
        }
        let scored: Vec<(usize, &str)> = self
            .unit
            .deps
            .iter()
            .filter_map(|d| {
                coordinate_prefixes(&d.coord)
                    .into_iter()
                    .filter(|p| rules_clj::prefix_matches(ns, p))
                    .map(|p| p.len())
                    .max()
                    .map(|len| (len, d.coord.as_str()))
            })
            .collect();
        let Some(best) = scored.iter().map(|(len, _)| *len).max() else {
            return Vec::new();
        };
        let winners: BTreeSet<&str> = scored
            .iter()
            .filter(|(len, _)| *len == best)
            .map(|(_, c)| *c)
            .collect();
        let certainty = if winners.len() == 1 {
            Certainty::Likely
        } else {
            Certainty::Possible
        };
        winners
            .into_iter()
            .map(|c| (c.to_owned(), certainty, Reason::NameMatch))
            .collect()
    }

    /// A library node and its `provides` edge.
    ///
    /// Example: `provides(maven reagent/reagent, ext:clj//reagent.core, Likely, NameMatch)`.
    fn provides(&mut self, lib: LibraryId, to: &NodeId, certainty: Certainty, reason: Reason) {
        let name = if lib.coordinate().is_empty() {
            "unknown".to_owned()
        } else {
            lib.coordinate().to_owned()
        };
        let id = NodeId::Library(lib);
        self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::Library,
            subkind: None,
            name,
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
        });
        self.edge(
            id,
            to.clone(),
            EdgeKind::Provides,
            Some(certainty),
            Some(reason),
            None,
            None,
        );
    }

    /// The call site that starts at `pos` in `file`.
    ///
    /// Example: the position of `(rf/dispatch [:a])` gives that call.
    fn call_at(&self, file: &str, pos: Pos) -> Option<&'a crate::forms::CallSite> {
        let ix: &'a FileIndex = self.indexes.get(file)?;
        ix.calls.iter().find(|c| c.span.start == pos)
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
            } else if u.to == "goog" || u.to.starts_with("goog.") {
                PlatformId::js(&u.to)
                    .ok()
                    .map(NodeId::Platform)
                    .inspect(|id| {
                        self.nodes
                            .entry(id.clone())
                            .or_insert_with(|| platform_node(id, &u.to));
                    })
            } else {
                self.external(&u.to, None)
            };
            let Some(to) = to else { continue };
            let site = site(&u.filename, u.row, u.col);
            let lang = u.lang.as_deref().and_then(Lang::from_token);
            if self.is_string_require(
                &u.filename,
                Pos {
                    line: u.row,
                    col: u.col,
                },
            ) {
                self.npm_library(&u.to, &to);
            }
            self.edge(from, to, EdgeKind::Requires, None, None, lang, site);
        }
    }

    /// Whether a namespace usage is a string libspec (`["react" :as r]`),
    /// i.e. an npm package: the form index has a string literal there.
    ///
    /// Example: the `"react"` in `(:require ["react" :as react])` gives `true`.
    fn is_string_require(&self, file: &str, pos: Pos) -> bool {
        self.indexes.get(file).is_some_and(|ix| {
            ix.literals
                .iter()
                .any(|l| l.kind == LiteralKind::Str && l.span.start == pos)
        })
    }

    /// R10 rule 1: an npm package `lib:npm:<name>` that certainly provides
    /// the namespace required by its string name.
    ///
    /// Example: `npm_library("react", ext:clj//react)` adds
    /// `lib:npm:react -provides(certain)-> ext:clj//react`.
    fn npm_library(&mut self, package: &str, provided: &NodeId) {
        let Ok(lib) = aldente_model::LibraryId::new("npm", package) else {
            return;
        };
        let id = NodeId::Library(lib);
        self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::Library,
            subkind: None,
            name: package.to_owned(),
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
        });
        self.edge(
            id,
            provided.clone(),
            EdgeKind::Provides,
            Some(Certainty::Certain),
            Some(Reason::Direct),
            None,
            None,
        );
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
            let core_call =
                is_call && matches!(u.to.as_deref(), Some("clojure.core" | "cljs.core"));
            self.edge(
                scope.clone(),
                to,
                kind,
                Some(certainty),
                Some(reason),
                lang,
                site,
            );
            if core_call {
                self.dynamic_code(&scope, u, pos);
            }
        }
    }

    /// R8 for a call to a core function: quoted-literal resolution becomes a
    /// `likely` `quoted-resolve` call to the named var; resolution of
    /// anything else, and evaluation, becomes a hole with a certain `dynamic`
    /// call and a blind spot; `intern`, `alter-var-root` and `with-redefs`
    /// flag their target var `dynamic`.
    ///
    /// Example: `(requiring-resolve 'a.b/c)` gives `calls(likely quoted-resolve) a.b/c`;
    /// `(eval x)` gives `calls(certain dynamic) hole:dynamic-eval@…`.
    fn dynamic_code(&mut self, scope: &NodeId, u: &VarUsage, pos: Pos) {
        let Some(ix) = self.indexes.get(u.filename.as_str()) else {
            return;
        };
        let Some(call) = ix.calls.iter().find(|c| c.span.start == pos).cloned() else {
            return;
        };
        let quoted = |i: usize| {
            call.arg_info
                .get(i)
                .filter(|a| a.kind == "quoting_lit")
                .map(|a| a.text.trim_start_matches('\'').to_owned())
        };
        match u.name.as_str() {
            "resolve" | "requiring-resolve" | "find-var" | "ns-resolve" => {
                let named = if u.name == "ns-resolve" {
                    quoted(0).zip(quoted(1))
                } else {
                    quoted(0)
                        .and_then(|s| s.split_once('/').map(|(a, b)| (a.to_owned(), b.to_owned())))
                };
                match named {
                    Some((ns, name)) => {
                        let target = if self.local_ns.contains(&ns) {
                            self.symbol_or_placeholder(&ns, &name, u)
                        } else {
                            self.external(&ns, Some(&name))
                        };
                        if let Some(target) = target {
                            let lang = u.lang.as_deref().and_then(Lang::from_token);
                            self.edge(
                                scope.clone(),
                                target,
                                EdgeKind::Calls,
                                Some(Certainty::Likely),
                                Some(Reason::QuotedResolve),
                                lang,
                                site_of(u),
                            );
                        }
                    }
                    None => self.hole(scope, u, pos, BlindSpotCategory::DynamicResolve),
                }
            }
            "eval" | "load-string" | "load-file" | "load-reader" | "load" => {
                self.hole(scope, u, pos, BlindSpotCategory::DynamicEval);
            }
            "alter-var-root" | "intern" | "with-redefs" => self.flag_dynamic(u, &call),
            _ => {}
        }
    }

    /// A hole node at the call, a certain `dynamic` call from `scope` to it,
    /// and a blind spot of the same category.
    ///
    /// Example: `(eval x)` at 16:20 gives `hole:dynamic-eval@src/dyn/core.clj:16:20`.
    fn hole(&mut self, scope: &NodeId, u: &VarUsage, pos: Pos, category: BlindSpotCategory) {
        let at = HoleAt {
            name: &u.name,
            ns: &u.from,
            file: &u.filename,
            lang: u.lang.as_deref().and_then(Lang::from_token),
            pos,
        };
        self.hole_at(scope, &at, category);
    }

    /// [`Self::hole`] for a position that is not a clj-kondo usage (an R11
    /// effect entry).
    ///
    /// Example: `{:dispatch [evt]}` at 5:4 gives `hole:dynamic-resolve@…:5:4`.
    fn hole_at(&mut self, scope: &NodeId, at: &HoleAt<'_>, category: BlindSpotCategory) {
        let (pos, name) = (at.pos, at.name);
        let Ok(file) = RepoPath::parse(at.file) else {
            return;
        };
        let id = NodeId::Hole(aldente_model::HoleId::new(
            category, file, pos.line, pos.col,
        ));
        let lang = at.lang.unwrap_or_else(|| lang_of(None, at.file));
        let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::Hole,
            subkind: None,
            name: format!("{name} ({})", category.as_str()),
            namespace: Some(at.ns.to_owned()),
            unit: Some(NodeId::Unit(self.unit.dir.clone())),
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: location(at.file, pos.line, pos.col, None, None),
            doc: None,
            signature: None,
        });
        node.langs.insert(lang);
        self.blindspots.push(BlindSpot {
            category,
            node: Some(id.clone()),
            site: site(at.file, pos.line, pos.col),
            message: format!(
                "{name} with a computed argument: the target cannot be known statically"
            ),
        });
        self.edge(
            scope.clone(),
            id,
            EdgeKind::Calls,
            Some(Certainty::Certain),
            Some(Reason::Dynamic),
            at.lang,
            site(at.file, pos.line, pos.col),
        );
    }

    /// Flags the var an `alter-var-root`, `intern` or `with-redefs` changes as
    /// `dynamic`: the var usage inside the first argument (`#'a/b`, the
    /// binding names of `with-redefs`), or `intern`'s quoted namespace and name.
    ///
    /// Example: `(alter-var-root #'t/counter inc)` flags `dyn.target/counter`.
    fn flag_dynamic(&mut self, u: &VarUsage, call: &crate::forms::CallSite) {
        let mut targets: Vec<NodeId> = Vec::new();
        if u.name == "intern" {
            let q = |i: usize| {
                call.arg_info
                    .get(i)
                    .filter(|a| a.kind == "quoting_lit")
                    .map(|a| a.text.trim_start_matches('\'').to_owned())
            };
            if let (Some(ns), Some(name)) = (q(0), q(1))
                && self.local_ns.contains(&ns)
                && let Some(id) = self.symbol_or_placeholder(&ns, &name, u)
            {
                targets.push(id);
            }
        } else if let Some(span) = call.args.first() {
            let names: Vec<String> = match call.arg_info.first() {
                Some(a) if a.kind == "vec_lit" => a
                    .entries
                    .iter()
                    .map(|(k, _)| local(Some(k)).to_owned())
                    .collect(),
                _ => Vec::new(),
            };
            let inside: Vec<VarUsage> = self
                .run
                .analysis
                .var_usages
                .iter()
                .filter(|v| v.filename == u.filename && upos(v).is_some_and(|p| span.contains(p)))
                .filter(|v| u.name != "with-redefs" || names.contains(&v.name))
                .cloned()
                .collect();
            for v in inside {
                if let Some((NodeId::Symbol(s), _)) = self.target(&v) {
                    targets.push(NodeId::Symbol(s));
                }
            }
        }
        for id in targets {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.flags.insert(SymbolFlag::Dynamic);
            }
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
        if matches!(u.name.as_str(), "." | "new")
            && matches!(u.to.as_deref(), Some("clojure.core" | "cljs.core"))
        {
            return Some(Suppression::InteropForm);
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
        if let Some(id) = self.impl_scope(&u.filename, pos) {
            return Some(id);
        }
        if let Some(var) = &u.from_var {
            return self.symbol_or_placeholder(&u.from, var, u);
        }
        self.scope_at(&u.filename, pos, &u.from)
    }

    /// The defmethod, protocol impl or R11 registration whose body contains
    /// `pos`, if any (the innermost).
    ///
    /// Example: a position inside `(defmethod price :book …)` gives `price@:book`.
    fn impl_scope(&self, file: &str, pos: Pos) -> Option<NodeId> {
        let ix = *self.indexes.get(file)?;
        if let Some(top) = ix.forms.iter().find(|f| f.span.contains(pos))
            && let Some(id) = self.defmethod_impls.get(&(file.to_owned(), top.span.start))
        {
            return Some(id.clone());
        }
        self.protocol_impls
            .iter()
            .chain(&self.reg_impls)
            .filter(|(f, span, _)| f == file && span.contains(pos))
            .min_by_key(|(_, span, _)| span.end_byte - span.start_byte)
            .map(|(_, _, id)| id.clone())
    }

    /// R3 for a bare position (no clj-kondo `from-var`): an impl body, the
    /// innermost var definition, else the top-level form's `<top>` impl.
    ///
    /// Example: the static call on line 6 of shop.interop gives `shop.interop/now-ms`.
    fn scope_at(&mut self, file: &str, pos: Pos, ns: &str) -> Option<NodeId> {
        if let Some(id) = self.impl_scope(file, pos) {
            return Some(id);
        }
        let innermost = self
            .run
            .analysis
            .var_definitions
            .iter()
            .filter(|d| d.filename == file && def_span_contains(d, pos))
            .min_by_key(|d| {
                (
                    d.end_row.unwrap_or(d.row) - d.row,
                    d.end_col.unwrap_or(d.col),
                )
            });
        if let Some(def) = innermost {
            return self.sym_id(&def.ns, &def.name).map(NodeId::Symbol);
        }
        let ix = *self.indexes.get(file)?;
        let top = ix.forms.iter().find(|f| f.span.contains(pos))?;
        self.top_impl(ns, file, top)
    }

    /// The `<ns>/<top>@L<line>` impl for a top-level form, created on first use.
    ///
    /// Example: the form on line 10 of shop.core gives `clj:.//shop.core/<top>@L10`.
    fn top_impl(&mut self, ns: &str, file: &str, top: &TopForm) -> Option<NodeId> {
        let disc = format!("L{}", top.span.start.line);
        let id = self.impl_node(ns, ns, "<top>", &disc, file, top.span)?;
        // R14: a top-level test form without a var (Midje `fact`).
        let head = local(top.head.as_deref());
        if self.ctx.rules.tests.top_level.iter().any(|t| t == head)
            && let Some(node) = self.nodes.get_mut(&id)
        {
            node.flags.insert(SymbolFlag::Test);
            node.flags.insert(SymbolFlag::EntryPoint);
        }
        Some(id)
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
            Some(ns) if ns == "goog" || ns.starts_with("goog.") => {
                let id = NodeId::Platform(PlatformId::js(&format!("{ns}.{}", u.name)).ok()?);
                self.nodes
                    .entry(id.clone())
                    .or_insert_with(|| platform_node(&id, &format!("{ns}/{}", u.name)));
                Some((id, true))
            }
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

    /// R7 layer 2: when a function calls one of its own parameters, emit
    /// `calls` (possible, `param-flow`) from it to each var its callers pass
    /// in that position, matching the caller's argument count to an arity.
    /// One level only; anonymous functions and other non-symbol arguments add
    /// nothing.
    ///
    /// Example: `(defn apply-it [f x] (f x))` with a caller
    /// `(apply-it inc2 1)` gives `apply-it -calls(param-flow)-> inc2`.
    fn param_flow(&mut self) {
        let mut flows: Vec<(NodeId, Vec<Arity>, usize, usize, Site)> = Vec::new();
        let analysis = &self.run.analysis;
        let bindings: BTreeMap<u64, (&str, Pos)> = analysis
            .locals
            .iter()
            .filter_map(|l| {
                Some((
                    l.id?,
                    (
                        l.filename.as_str(),
                        Pos {
                            line: l.row,
                            col: l.col,
                        },
                    ),
                ))
            })
            .collect();
        for usage in &analysis.local_usages {
            let (Some(id), Some(name)) = (usage.id, usage.name.as_deref()) else {
                continue;
            };
            let Some(ix) = self.indexes.get(usage.filename.as_str()) else {
                continue;
            };
            let at = Pos {
                line: usage.row,
                col: usage.col,
            };
            let in_call_position = ix
                .calls
                .iter()
                // Like var usages, a local in call position is reported at the `(`.
                .any(|c| c.head == name && c.span.start == at);
            let Some((file, binding)) = bindings.get(&id) else {
                continue;
            };
            if !in_call_position || *file != usage.filename {
                continue;
            }
            let Some(form) = ix
                .forms
                .iter()
                .find(|f| f.span.contains(at) && !f.arities.is_empty())
            else {
                continue;
            };
            let found = form.arities.iter().enumerate().find_map(|(ai, a)| {
                a.params
                    .iter()
                    .position(|p| p.as_ref().is_some_and(|(_, pos)| pos == binding))
                    .map(|pi| (ai, pi))
            });
            let Some((arity, param)) = found else {
                continue;
            };
            let Some(def) = analysis.var_definitions.iter().find(|d| {
                d.filename == usage.filename
                    && d.row == form.span.start.line
                    && d.col == form.span.start.col
            }) else {
                continue;
            };
            let Some(f) = self.sym_id(&def.ns, &def.name) else {
                continue;
            };
            let Some(site) = site(&usage.filename, at.line, at.col) else {
                continue;
            };
            flows.push((NodeId::Symbol(f), form.arities.clone(), arity, param, site));
        }
        for (f, arities, arity, param, site) in flows {
            let NodeId::Symbol(sym) = &f else { continue };
            let callers: Vec<VarUsage> = self
                .run
                .analysis
                .var_usages
                .iter()
                .filter(|u| {
                    u.to.as_deref() == Some(sym.namespace().ns())
                        && u.name == sym.name()
                        && self.is_call(u)
                })
                .cloned()
                .collect();
            for caller in callers {
                let Some(pos) = upos(&caller) else { continue };
                let Some(ix) = self.indexes.get(caller.filename.as_str()) else {
                    continue;
                };
                let Some(call) = ix.calls.iter().find(|c| c.span.start == pos) else {
                    continue;
                };
                let n = call.arg_info.len();
                let chosen = arities
                    .iter()
                    .position(|a| a.params.len() == n && !a.variadic)
                    .or_else(|| {
                        arities
                            .iter()
                            .position(|a| a.variadic && n >= a.params.len())
                    });
                if chosen != Some(arity) {
                    continue;
                }
                let (Some(arg), Some(span)) = (call.arg_info.get(param), call.args.get(param))
                else {
                    continue;
                };
                if arg.kind != "sym_lit" {
                    continue;
                }
                let passed = self
                    .run
                    .analysis
                    .var_usages
                    .iter()
                    .find(|u| u.filename == caller.filename && upos(u) == Some(span.start))
                    .cloned();
                let Some(passed) = passed else { continue };
                if let Some((to, _)) = self.target(&passed) {
                    self.edge(
                        f.clone(),
                        to,
                        EdgeKind::Calls,
                        Some(Certainty::Possible),
                        Some(Reason::ParamFlow),
                        None,
                        Some(site.clone()),
                    );
                }
            }
        }
    }

    /// R9: Java and JavaScript interop to `platform` nodes, certain, reason
    /// `interop`. Imports: the namespace `references` the class. Static calls
    /// (`Class/method`): `calls java:<class>#<method>`. Constructors
    /// (`Class.`, `new`): `calls java:<class>`. `proxy`: `references` its
    /// classes. Instance methods: `calls java:<class>#<method>` with the class
    /// from a type hint on the receiver or its parameter, else
    /// `java:?#<method>`; in ClojureScript `js:?.<method>`, and `.-prop` is a
    /// `references` to `js:?.<prop>`. Other class mentions (hints,
    /// `extend-type` targets) add no edge. Each platform target is one
    /// `interop` blind spot.
    ///
    /// Example: `(System/currentTimeMillis)` in `now-ms` gives
    /// `now-ms -calls-> java:java.lang.System#currentTimeMillis`.
    fn interop(&mut self) {
        let generated: Vec<String> = self.local_ns.iter().map(|ns| format!("{ns}.")).collect();
        let class_uses = self.run.analysis.java_class_usages.clone();
        for cu in &class_uses {
            if generated.iter().any(|g| cu.class.starts_with(g.as_str())) {
                continue;
            }
            let Some(ns) = self.file_ns(&cu.filename) else {
                continue;
            };
            let site = cu
                .row
                .zip(cu.col)
                .and_then(|(r, c)| site(&cu.filename, r, c));
            if cu.import == Some(true) {
                if let Some(from) = self.ns_id(&ns).map(NodeId::Namespace) {
                    let lang = cu.lang.as_deref().and_then(Lang::from_token);
                    self.platform_edge(from, &cu.class, None, EdgeKind::References, lang, site);
                }
                continue;
            }
            let (Some(row), Some(col)) = (cu.row, cu.col) else {
                continue;
            };
            let pos = Pos { line: row, col };
            let Some(ix) = self.indexes.get(cu.filename.as_str()).copied() else {
                continue;
            };
            let kind_member = if let (Some(m), Some(true)) = (&cu.method_name, cu.call) {
                Some((EdgeKind::Calls, Some(m.clone())))
            } else if ix
                .calls
                .iter()
                .any(|c| c.span.start == pos && (c.head == "new" || c.head.ends_with('.')))
            {
                Some((EdgeKind::Calls, None))
            } else if ix.calls.iter().any(|c| {
                local(Some(&c.head)) == "proxy" && c.args.first().is_some_and(|a| a.contains(pos))
            }) {
                Some((EdgeKind::References, None))
            } else {
                None
            };
            let Some((kind, member)) = kind_member else {
                continue;
            };
            if let Some(scope) = self.scope_at(&cu.filename, pos, &ns) {
                let lang = cu.lang.as_deref().and_then(Lang::from_token);
                self.platform_edge(scope, &cu.class, member.as_deref(), kind, lang, site);
            }
        }
        let invocations = self.run.analysis.instance_invocations.clone();
        for inv in &invocations {
            let Some(ix) = self.indexes.get(inv.filename.as_str()).copied() else {
                continue;
            };
            let at = Pos {
                line: inv.name_row,
                col: inv.name_col,
            };
            let Some(call) = ix
                .calls
                .iter()
                .filter(|c| c.span.contains(at) && c.head.starts_with('.'))
                .max_by_key(|c| c.span.start_byte)
                .cloned()
            else {
                continue;
            };
            let Some(ns) = self.file_ns(&inv.filename) else {
                continue;
            };
            let Some(scope) = self.scope_at(&inv.filename, call.span.start, &ns) else {
                continue;
            };
            let site = site(&inv.filename, call.span.start.line, call.span.start.col);
            if lang_of(inv.lang.as_deref(), &inv.filename) == Lang::Cljs {
                let (kind, name) = match inv.method_name.strip_prefix('-') {
                    Some(prop) => (EdgeKind::References, prop),
                    None => (EdgeKind::Calls, inv.method_name.as_str()),
                };
                let Ok(p) = PlatformId::js(&format!("?.{name}")) else {
                    continue;
                };
                let id = NodeId::Platform(p);
                self.nodes
                    .entry(id.clone())
                    .or_insert_with(|| platform_node(&id, &format!(".{}", inv.method_name)));
                self.edge(
                    scope,
                    id,
                    kind,
                    Some(Certainty::Certain),
                    Some(Reason::Interop),
                    None,
                    site,
                );
            } else {
                let class = self
                    .receiver_class(&inv.filename, &call)
                    .unwrap_or_else(|| "?".to_owned());
                let lang = inv.lang.as_deref().and_then(Lang::from_token);
                self.platform_edge(
                    scope,
                    &class,
                    Some(&inv.method_name),
                    EdgeKind::Calls,
                    lang,
                    site,
                );
            }
        }
        let platforms: Vec<NodeId> = self
            .nodes
            .keys()
            .filter(|id| matches!(id, NodeId::Platform(_)))
            .cloned()
            .collect();
        for id in platforms {
            self.blindspots.push(BlindSpot {
                category: BlindSpotCategory::Interop,
                message: format!("{id} is outside the analyzed code (interop)"),
                node: Some(id),
                site: None,
            });
        }
    }

    /// A certain `interop` edge to `java:<class>[#member]`, creating the node.
    ///
    /// Example: `platform_edge(f, "java.util.UUID", Some("randomUUID"), Calls, site)`.
    fn platform_edge(
        &mut self,
        from: NodeId,
        class: &str,
        member: Option<&str>,
        kind: EdgeKind,
        lang: Option<Lang>,
        site: Option<Site>,
    ) {
        let Ok(p) = PlatformId::java(class, member) else {
            return;
        };
        let id = NodeId::Platform(p);
        let name = match member {
            Some(m) => format!("{class}#{m}"),
            None => class.to_owned(),
        };
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| platform_node(&id, &name));
        self.edge(
            from,
            id,
            kind,
            Some(Certainty::Certain),
            Some(Reason::Interop),
            lang,
            site,
        );
    }

    /// The Java class of an instance call's receiver: a hint on the argument
    /// (`(.trim ^String x)`) or on the parameter it is bound to
    /// (`(defn f [^String s] (.trim s))`), qualified through `java.lang` and
    /// the file's imports.
    ///
    /// Example: in `(defn hinted [^String s] (.trim s))` gives `"java.lang.String"`.
    fn receiver_class(&self, file: &str, call: &crate::forms::CallSite) -> Option<String> {
        // `(.m x)` and `(. x m)` both have the receiver first.
        let (arg, span) = (call.arg_info.first()?, call.args.first()?);
        let short = match &arg.tag {
            Some(t) => t.clone(),
            None if arg.kind == "sym_lit" => {
                let usage = self.run.analysis.local_usages.iter().find(|l| {
                    l.filename == file && l.row == span.start.line && l.col == span.start.col
                })?;
                let local = self
                    .run
                    .analysis
                    .locals
                    .iter()
                    .find(|l| l.id.is_some() && l.id == usage.id)?;
                let binding = Pos {
                    line: local.row,
                    col: local.col,
                };
                let ix = self.indexes.get(file)?;
                let form = ix.forms.iter().find(|f| f.span.contains(binding))?;
                form.arities.iter().find_map(|a| {
                    let i = a
                        .params
                        .iter()
                        .position(|p| p.as_ref().is_some_and(|(_, pos)| *pos == binding))?;
                    a.tags.get(i).cloned().flatten()
                })?
            }
            None => return None,
        };
        Some(self.qualify_class(file, &short))
    }

    /// A class name as written, qualified through the file's imports, else
    /// `java.lang`. Qualified names are returned unchanged.
    ///
    /// Example: `Runnable` gives `java.lang.Runnable`; with
    /// `(:import [java.util Comparator])`, `Comparator` gives `java.util.Comparator`.
    fn qualify_class(&self, file: &str, short: &str) -> String {
        if short.contains('.') {
            return short.to_owned();
        }
        let imported = self.run.analysis.java_class_usages.iter().find(|c| {
            c.filename == file
                && c.import == Some(true)
                && c.class.rsplit('.').next() == Some(short)
        });
        imported.map_or_else(|| format!("java.lang.{short}"), |c| c.class.clone())
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

    /// Adds `impl implements java:<class>` for a Java interface in `reify`,
    /// `deftype`, `defrecord` or `proxy`.
    ///
    /// Example: `(reify clojure.lang.IDeref (deref [_] 1))` gives
    /// `…/deref@reify:L1 -implements-> java:clojure.lang.IDeref`.
    fn implements_class(&mut self, impl_id: NodeId, class: &str) {
        let Ok(p) = PlatformId::java(class, None) else {
            return;
        };
        let id = NodeId::Platform(p);
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| platform_node(&id, class));
        self.edge(impl_id, id, EdgeKind::Implements, None, None, None, None);
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
    fn finish(mut self) -> Resolved {
        let (mut edges, dropped) = merge_languages(self.edges.into_values().collect());
        self.stats.merged_duplicates += dropped;
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

/// R12: an edge that exists for both `clj` and `cljs` (from the two reads of
/// a `.cljc` file) with the same certainty is stored once, without a
/// language, its sites combined. Edges that differ between the languages
/// keep theirs. Also returns how many call/reference sites were dropped as
/// duplicates (the same position seen by both reads), for [`UsageStats`].
///
/// Example: `both -calls-> now` from clj and from cljs becomes one edge with
/// `lang: None` and 1 dropped site; `now -calls-> java:…` (clj only) keeps
/// `lang: Some(Clj)`.
fn merge_languages(edges: Vec<Edge>) -> (Vec<Edge>, usize) {
    type Key = (NodeId, NodeId, EdgeKind, Option<Reason>, Option<Certainty>);
    let mut groups: BTreeMap<Key, Vec<Edge>> = BTreeMap::new();
    for e in edges {
        groups
            .entry((e.from.clone(), e.to.clone(), e.kind, e.reason, e.certainty))
            .or_default()
            .push(e);
    }
    let mut out = Vec::new();
    let mut dropped = 0;
    for (_, group) in groups {
        let langs: BTreeSet<Option<Lang>> = group.iter().map(|e| e.lang).collect();
        let both = langs.contains(&Some(Lang::Clj)) && langs.contains(&Some(Lang::Cljs));
        if !both {
            out.extend(group);
            continue;
        }
        let mut merged = group[0].clone();
        merged.lang = None;
        merged.sites = Vec::new();
        for e in group {
            for site in e.sites {
                if !merged.sites.contains(&site) {
                    merged.sites.push(site);
                } else if matches!(merged.kind, EdgeKind::Calls | EdgeKind::References) {
                    dropped += 1;
                }
            }
        }
        out.push(merged);
    }
    (out, dropped)
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

/// Namespace prefixes a Maven coordinate suggests: the artifact, the
/// artifact with `-` as `.`, the group, and the group's last part joined to
/// the artifact (both spellings).
///
/// Example: `org.clojure/core.async` gives `core.async`, `org.clojure`,
/// `clojure.core.async`; `metosin/reitit-core` gives `reitit-core`,
/// `reitit.core`, `metosin`, `metosin.reitit-core`, `metosin.reitit.core`.
fn coordinate_prefixes(coord: &str) -> Vec<String> {
    let Some((group, artifact)) = coord.split_once('/') else {
        return Vec::new();
    };
    let dotted = artifact.replace('-', ".");
    let last = group.rsplit('.').next().unwrap_or(group);
    let mut out = vec![
        artifact.to_owned(),
        dotted.clone(),
        group.to_owned(),
        format!("{last}.{artifact}"),
        format!("{last}.{dotted}"),
    ];
    out.sort();
    out.dedup();
    out
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
