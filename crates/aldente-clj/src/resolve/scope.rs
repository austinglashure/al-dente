//! R3: the enclosing scope of every usage.

use super::*;

impl<'a> Resolver<'a> {
    /// R3: the node a usage belongs to. In order: a `defmethod` body, a
    /// protocol method body (not `reify`), clj-kondo's `from-var`, the
    /// innermost var definition containing it, else a `<top>@L<line>` impl
    /// for its top-level form.
    ///
    /// Example: the `println` at top level on line 10 of shop.core gives
    /// `clj:.//shop.core/<top>@L10`.
    pub(super) fn scope(&mut self, u: &VarUsage) -> Option<NodeId> {
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
    pub(super) fn impl_scope(&self, file: &str, pos: Pos) -> Option<NodeId> {
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
    pub(super) fn scope_at(&mut self, file: &str, pos: Pos, ns: &str) -> Option<NodeId> {
        if let Some(id) = self.impl_scope(file, pos) {
            return Some(id);
        }
        let innermost = self
            .defs_in(file)
            .iter()
            .filter(|d| def_span_contains(d, pos))
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
    pub(super) fn top_impl(&mut self, ns: &str, file: &str, top: &TopForm) -> Option<NodeId> {
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
    pub(super) fn symbol_or_placeholder(
        &mut self,
        ns: &str,
        name: &str,
        u: &VarUsage,
    ) -> Option<NodeId> {
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
                    members: None,
                    symbols: None,
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
}
