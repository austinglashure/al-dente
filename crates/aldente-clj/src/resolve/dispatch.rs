//! R5 and R6: dispatch edges from multimethod and protocol calls to the impls that may run.

use super::*;

impl<'a> Resolver<'a> {
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
    pub(super) fn multimethod_dispatch(&mut self) {
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
    pub(super) fn protocol_dispatch(&mut self) {
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
    pub(super) fn receiver_type(&self, file: &str, pos: Pos) -> Option<String> {
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

    /// Keywords that appear as the child in a `(derive child parent)` call
    /// (or `(derive h child parent)`) anywhere in this unit.
    ///
    /// Example: `(derive :a/poodle :a/dog)` gives `{":a/poodle"}`.
    pub(super) fn derived_children(&self) -> BTreeSet<String> {
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
    pub(super) fn dispatch_keyword(&self, multi: &NodeId) -> Option<String> {
        let NodeId::Symbol(sym) = multi else {
            return None;
        };
        let def = *self.defs_by_name.get(&(sym.namespace().ns(), sym.name()))?;
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
    pub(super) fn literal_entry(&self, file: &str, pos: Pos, key: &str) -> Option<String> {
        let ix = self.indexes.get(file)?;
        let call = ix.calls.iter().find(|c| c.span.start == pos)?;
        let first = call.arg_info.first().filter(|a| a.kind == "map_lit")?;
        first
            .entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
}
