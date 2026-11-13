//! R5 and R6: impl nodes for defmethods and protocol bodies, and their implements edges.

use super::*;

impl<'a> Resolver<'a> {
    /// R3 impl nodes that exist regardless of usages: every `defmethod` and
    /// every protocol method body outside `reify` (Phase 1 leaves `reify`
    /// bodies to the enclosing function).
    ///
    /// Example: `(defmethod price :gift [_] 0)` gives `shop.pricing/price@:gift`.
    pub(super) fn impls(&mut self) {
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

    /// The multimethod a `defmethod` form names, from clj-kondo's usage with
    /// `defmethod: true` inside the form.
    ///
    /// Example: `(defmethod price :book …)` in shop.pricing gives
    /// `("shop.pricing", "price")`.
    pub(super) fn defmethod_target(&self, file: &str, form: &TopForm) -> Option<(String, String)> {
        self.usages_in(file)
            .iter()
            .find(|u| u.defmethod == Some(true) && upos(u).is_some_and(|p| form.span.contains(p)))
            .and_then(|u| Some((u.to.clone()?, u.name.clone())))
    }

    /// Creates an impl node `<target_ns>/<method>@<disc>` defined in `ns`,
    /// contained by its multimethod or protocol method when this unit
    /// defines it, otherwise by the namespace holding the code.
    ///
    /// Example: `impl_node("shop.pricing", "shop.pricing", "price", ":book", …)`
    /// gives `clj:.//shop.pricing/price@:book`.
    pub(super) fn impl_node(
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

    /// Adds `impl implements target`, where the target is this unit's symbol
    /// or, for a protocol or multimethod defined elsewhere, an external.
    ///
    /// Example: `implements(area@Circle, "pr.shapes", "Shape")`.
    pub(super) fn implements(&mut self, impl_id: NodeId, target_ns: &str, name: &str) {
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
    pub(super) fn implements_class(&mut self, impl_id: NodeId, class: &str) {
        let Ok(p) = PlatformId::java(class, None) else {
            return;
        };
        let id = NodeId::Platform(p);
        self.nodes
            .entry(id.clone())
            .or_insert_with(|| platform_node(&id, class));
        self.edge(impl_id, id, EdgeKind::Implements, None, None, None, None);
    }
}
