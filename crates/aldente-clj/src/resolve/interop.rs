//! R9: Java and JavaScript interop exits.

use super::*;

impl<'a> Resolver<'a> {
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
    pub(super) fn interop(&mut self) {
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
    pub(super) fn platform_edge(
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
    pub(super) fn receiver_class(
        &self,
        file: &str,
        call: &crate::forms::CallSite,
    ) -> Option<String> {
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
    pub(super) fn qualify_class(&self, file: &str, short: &str) -> String {
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
}
