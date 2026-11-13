//! R11: keyword-keyed dispatch (re-frame, Integrant) from `rules/clj-keyword-dispatch.toml`.

use super::*;

impl<'a> Resolver<'a> {
    /// R11: each call to a catalog registration function with a literal
    /// keyword becomes an impl `<ns>/<fn>@<key>`, a handler of its family;
    /// so do the `defmethod`s of a family's multimethods. `:<-` signals of a
    /// registration are queued as dispatches from it.
    ///
    /// Example: `(rf/reg-event-db :cart/add f)` in shop.ui.events gives
    /// `clj:.//shop.ui.events/reg-event-db@:cart/add`.
    pub(super) fn registrations(&mut self) {
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
    pub(super) fn keyword_dispatch(&mut self) {
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
    pub(super) fn effect_keys(&self, effect: &kd::Effect, value: &Shape, out: &mut Vec<FoundKey>) {
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
    pub(super) fn effect_map_keys(
        &self,
        inside: Option<&String>,
        key: &str,
        out: &mut Vec<FoundKey>,
    ) {
        let Some(inside) = inside else { return };
        for m in self.catalog.effect_maps_inside(inside) {
            out.push((m.family.clone(), Some(key.to_owned()), true));
        }
    }

    /// Turns one R11 dispatch into edges, a hole, or a blind spot.
    ///
    /// Example: a `KwUse` for `:order/submit` with one registered handler
    /// gives one `likely` `calls` edge.
    pub(super) fn link_keyword(&mut self, k: KwUse) {
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
    pub(super) fn resolve_keyword(&self, file: &str, ns: &str, kw: &str) -> String {
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
}
