//! R7: higher-order flow, functions passed as arguments.

use super::*;

impl<'a> Resolver<'a> {
    /// R7 layer 2: when a function calls one of its own parameters, emit
    /// `calls` (possible, `param-flow`) from it to each var its callers pass
    /// in that position, matching the caller's argument count to an arity.
    /// One level only; anonymous functions and other non-symbol arguments add
    /// nothing.
    ///
    /// Example: `(defn apply-it [f x] (f x))` with a caller
    /// `(apply-it inc2 1)` gives `apply-it -calls(param-flow)-> inc2`.
    pub(super) fn param_flow(&mut self) {
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
            let Some(def) = self
                .defs_in(&usage.filename)
                .iter()
                .find(|d| d.row == form.span.start.line && d.col == form.span.start.col)
            else {
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
                .usages_by_target
                .get(&(sym.namespace().ns(), sym.name()))
                .map_or(&[][..], Vec::as_slice)
                .iter()
                .filter(|u| self.is_call(u))
                .map(|u| (*u).clone())
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
                    .usages_in(&caller.filename)
                    .iter()
                    .find(|u| upos(u) == Some(span.start))
                    .map(|u| (*u).clone());
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
}
