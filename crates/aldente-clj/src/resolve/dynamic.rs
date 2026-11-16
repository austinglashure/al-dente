//! R8: dynamic code: holes, quoted resolution, vars changed at run time, macro blind spots.

use super::*;

impl<'a> Resolver<'a> {
    /// R8 for a call to a core function: quoted-literal resolution becomes a
    /// `likely` `quoted-resolve` call to the named var; resolution of
    /// anything else, and evaluation, becomes a hole with a certain `dynamic`
    /// call and a blind spot; `intern`, `alter-var-root` and `with-redefs`
    /// flag their target var `dynamic`.
    ///
    /// Example: `(requiring-resolve 'a.b/c)` gives `calls(likely quoted-resolve) a.b/c`;
    /// `(eval x)` gives `calls(certain dynamic) hole:dynamic-eval@…`.
    pub(super) fn dynamic_code(&mut self, scope: &NodeId, u: &VarUsage, pos: Pos) {
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
    pub(super) fn hole(
        &mut self,
        scope: &NodeId,
        u: &VarUsage,
        pos: Pos,
        category: BlindSpotCategory,
    ) {
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
    pub(super) fn hole_at(&mut self, scope: &NodeId, at: &HoleAt<'_>, category: BlindSpotCategory) {
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
            members: None,
            symbols: None,
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
    pub(super) fn flag_dynamic(&mut self, u: &VarUsage, call: &crate::forms::CallSite) {
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
                .usages_in(&u.filename)
                .iter()
                .filter(|v| upos(v).is_some_and(|p| span.contains(p)))
                .filter(|v| u.name != "with-redefs" || names.contains(&v.name))
                .map(|v| (*v).clone())
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

    /// R4: one `macro-generated` blind spot per macro defined here whose
    /// body syntax-quotes a call.
    ///
    /// Example: `with-timing` gives one blind spot.
    pub(super) fn macro_blind_spots(&mut self) {
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
}
