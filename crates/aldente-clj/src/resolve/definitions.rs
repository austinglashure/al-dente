//! R1, R2 and R14: namespaces, definitions and their flags, requires, test namespaces.

use super::*;

impl<'a> Resolver<'a> {
    /// R2: one namespace node per definition, `.cljc` languages merged, and
    /// `unit contains namespace`.
    ///
    /// Example: `shop.model` from `model.cljc` gets langs `{clj, cljs}`.
    pub(super) fn namespaces(&mut self) {
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
        self.implicit_namespace();
    }

    /// One `user` node for the unit's scripts (files with forms but no `ns`
    /// form), located at the first script, so their code has a home.
    ///
    /// Example: `gen.clj` with `(defn f …)` and no `ns` gives `clj:.//user`.
    fn implicit_namespace(&mut self) {
        let scripts = scripts(self.run, self.indexes.values().copied());
        let Some(first) = scripts.first() else {
            return;
        };
        let Some(ns) = self.ns_id(IMPLICIT_NS) else {
            return;
        };
        let id = NodeId::Namespace(ns);
        let file = first.path.as_str().to_owned();
        let langs: BTreeSet<Lang> = scripts
            .iter()
            .map(|ix| lang_of(None, ix.path.as_str()))
            .collect();
        let node = self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::Namespace,
            subkind: None,
            name: IMPLICIT_NS.to_owned(),
            namespace: None,
            unit: Some(NodeId::Unit(self.unit.dir.clone())),
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: location(&file, 1, 1, None, None),
            doc: None,
            signature: None,
        });
        node.langs.extend(langs);
    }

    /// R14: a namespace in a file under one of the unit's test roots, or
    /// whose name marks it (`rules/clj-tests.toml`).
    ///
    /// Example: `app.thing-test` anywhere, or `app.helpers` in `test/app/helpers.clj`.
    pub(super) fn is_test_namespace(&self, ns: &str, file: &str) -> bool {
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
    pub(super) fn is_test_var(&self, def: &VarDefinition) -> bool {
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
    pub(super) fn definitions(&mut self) {
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
    pub(super) fn flags(&self, def: &VarDefinition) -> BTreeSet<SymbolFlag> {
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
    pub(super) fn is_stateful(&self, def: &VarDefinition) -> bool {
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
        self.usages_in(&def.filename).iter().any(|u| {
            upos(u) == Some(value.start)
                && STATEFUL_INITS.contains(&u.name.as_str())
                && matches!(u.to.as_deref(), Some("clojure.core" | "cljs.core"))
        })
    }

    /// R2: one `requires` edge per namespace usage, to this unit's namespace
    /// or a namespace-level external.
    ///
    /// Example: `(:require [re-frame.core :as rf])` in shop.ui.events gives
    /// `clj:.//shop.ui.events -requires-> ext:clj//re-frame.core`.
    pub(super) fn requires(&mut self) {
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
    pub(super) fn is_string_require(&self, file: &str, pos: Pos) -> bool {
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
    pub(super) fn npm_library(&mut self, package: &str, provided: &NodeId) {
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
}
