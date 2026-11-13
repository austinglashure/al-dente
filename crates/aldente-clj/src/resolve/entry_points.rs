//! R13: entry points found in code (ring handlers passed to server start calls).

use super::*;

impl<'a> Resolver<'a> {
    /// R13: a var passed as the first argument of a server start call in
    /// `rules/clj-entry-points.toml` (`(run-jetty #'app opts)`) is a ring
    /// handler and an entry point.
    ///
    /// Example: `(jetty/run-jetty #'app {:port 8080})` flags `app.core/app`.
    pub(super) fn server_handlers(&mut self) {
        let usages: &'a [VarUsage] = &self.run.analysis.var_usages;
        let mut handlers = Vec::new();
        for u in usages {
            let starts =
                u.to.as_deref()
                    .is_some_and(|to| self.ctx.rules.entry_points.is_server_start(to, &u.name));
            if !starts {
                continue;
            }
            let Some(arg) = upos(u)
                .and_then(|p| self.call_at(&u.filename, p))
                .and_then(|c| c.args.first())
            else {
                continue;
            };
            handlers.extend(
                self.usages_in(&u.filename)
                    .iter()
                    .copied()
                    .filter(|v| upos(v).is_some_and(|p| arg.contains(p))),
            );
        }
        for v in handlers {
            if let Some((id @ NodeId::Symbol(_), _)) = self.target(v)
                && let Some(node) = self.nodes.get_mut(&id)
            {
                node.flags.insert(SymbolFlag::EntryPoint);
            }
        }
    }
}
