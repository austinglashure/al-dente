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

mod comments;
mod definitions;
mod dispatch;
mod dynamic;
mod entry_points;
mod flow;
mod helpers;
mod impls;
mod interop;
mod keyword;
mod libraries;
mod scope;
mod usages;

use helpers::*;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use aldente_adapter::{FactBatch, LiteralFact};
use aldente_model::timing::Timings;
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
pub fn resolve<'a>(
    unit: &'a UnitInfo,
    run: &'a KondoRun,
    indexes: &'a [FileIndex],
    ctx: &'a Context,
) -> Resolved {
    let mut t = Timings::default();
    let mut r = t.time_detail("resolve:setup", || Resolver::new(unit, run, indexes, ctx));
    let steps: [Step<'a>; 15] = [
        ("resolve:namespaces", Resolver::namespaces),
        ("resolve:definitions", Resolver::definitions),
        ("resolve:impls", Resolver::impls),
        ("resolve:registrations", Resolver::registrations),
        ("resolve:requires", Resolver::requires),
        ("resolve:usages", Resolver::usages),
        ("resolve:keyword-dispatch", Resolver::keyword_dispatch),
        (
            "resolve:multimethod-dispatch",
            Resolver::multimethod_dispatch,
        ),
        ("resolve:protocol-dispatch", Resolver::protocol_dispatch),
        ("resolve:param-flow", Resolver::param_flow),
        ("resolve:interop", Resolver::interop),
        ("resolve:macro-blind-spots", Resolver::macro_blind_spots),
        ("resolve:libraries", Resolver::libraries),
        ("resolve:server-handlers", Resolver::server_handlers),
        ("resolve:comment-blocks", Resolver::comment_blocks),
    ];
    for (label, step) in steps {
        t.time_detail(label, || step(&mut r));
    }
    let mut out = t.time_detail("resolve:finish", || r.finish());
    out.batch.timings = t;
    out
}

/// One resolver step and its `--timings` label.
type Step<'a> = (&'static str, fn(&mut Resolver<'a>));

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
    /// clj-kondo var usages by file.
    usages_by_file: HashMap<&'a str, Vec<&'a VarUsage>>,
    /// clj-kondo var usages by target `(ns, name)`.
    usages_by_target: HashMap<(&'a str, &'a str), Vec<&'a VarUsage>>,
    /// Var definitions by file, and by `(ns, name)` (the first one).
    defs_by_file: HashMap<&'a str, Vec<&'a VarDefinition>>,
    defs_by_name: HashMap<(&'a str, &'a str), &'a VarDefinition>,
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
            usages_by_file: group(&run.analysis.var_usages, |u| u.filename.as_str()),
            usages_by_target: run
                .analysis
                .var_usages
                .iter()
                .filter_map(|u| Some(((u.to.as_deref()?, u.name.as_str()), u)))
                .fold(HashMap::new(), |mut m, (k, u)| {
                    m.entry(k).or_insert_with(Vec::new).push(u);
                    m
                }),
            defs_by_file: group(&run.analysis.var_definitions, |d| d.filename.as_str()),
            defs_by_name: run
                .analysis
                .var_definitions
                .iter()
                .fold(HashMap::new(), |mut m, d| {
                    m.entry((d.ns.as_str(), d.name.as_str())).or_insert(d);
                    m
                }),
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

    /// The clj-kondo var usages in `file`.
    ///
    /// Example: `usages_in("src/shop/core.clj")` lists that file's usages.
    fn usages_in(&self, file: &str) -> &[&'a VarUsage] {
        self.usages_by_file.get(file).map_or(&[], Vec::as_slice)
    }

    /// The var definitions in `file`.
    ///
    /// Example: `defs_in("src/shop/core.clj")` lists `checkout`, `-main`, ….
    fn defs_in(&self, file: &str) -> &[&'a VarDefinition] {
        self.defs_by_file.get(file).map_or(&[], Vec::as_slice)
    }

    /// The call site that starts at `pos` in `file`.
    ///
    /// Example: the position of `(rf/dispatch [:a])` gives that call.
    fn call_at(&self, file: &str, pos: Pos) -> Option<&'a crate::forms::CallSite> {
        let ix: &'a FileIndex = self.indexes.get(file)?;
        ix.calls.iter().find(|c| c.span.start == pos)
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
                timings: Default::default(),
            },
            stats: self.stats,
        }
    }
}
