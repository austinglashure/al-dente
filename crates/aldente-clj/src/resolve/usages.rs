//! R4: var usages as calls or references, their targets, and the documented suppressions.

use super::*;

impl<'a> Resolver<'a> {
    /// R3 and R4 for every var usage.
    ///
    /// Example: `(pricing/price order)` inside `checkout` gives a certain
    /// direct `calls` edge `checkout -> price`.
    pub(super) fn usages(&mut self) {
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

    /// Whether a usage is one of the documented non-uses.
    ///
    /// Example: the `defn` in `(defn f [] 1)` gives `Some(DefinitionForm)`.
    pub(super) fn suppression(&self, u: &VarUsage) -> Option<Suppression> {
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

    /// R4 target of a usage and whether it is interop. `None` when clj-kondo
    /// could not resolve it.
    ///
    /// Example: `str/join` with `to: "clojure.string"` gives
    /// `(ext:clj//clojure.string/join, false)`; `js/console.log` gives
    /// `(js:console.log, true)`.
    pub(super) fn target(&mut self, u: &VarUsage) -> Option<(NodeId, bool)> {
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
    pub(super) fn external(&mut self, ns: &str, name: Option<&str>) -> Option<NodeId> {
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
            members: None,
            symbols: None,
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
    pub(super) fn positionless(&mut self, u: &VarUsage) {
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
    pub(super) fn unresolved(&mut self, u: &VarUsage) {
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
    pub(super) fn is_call(&self, u: &VarUsage) -> bool {
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
    pub(super) fn in_macro_quote(&self, scope: &NodeId, file: &str, pos: Pos) -> bool {
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
}
