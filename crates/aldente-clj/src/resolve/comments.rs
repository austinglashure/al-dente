//! Rich comment blocks: code in a top-level `(comment …)` form runs only
//! when someone evaluates it by hand, so its edges are `possible` (decided
//! 2026-11-13 during M3; reasons are kept).

use super::*;

impl<'a> Resolver<'a> {
    /// Downgrades to `possible` every edge whose sites all lie inside a
    /// top-level `comment` form (`comment` or `clojure.core/comment`).
    /// Edges with a site outside such a form, or with no sites, keep their
    /// certainty.
    ///
    /// Example: `(comment (helper))` gives `<top>@L8 -calls(possible direct)-> helper`.
    pub(super) fn comment_blocks(&mut self) {
        let blocks: HashMap<&str, Vec<Span>> = self
            .indexes
            .iter()
            .map(|(file, ix)| {
                let spans = ix
                    .forms
                    .iter()
                    .filter(|f| {
                        matches!(f.head.as_deref(), Some("comment" | "clojure.core/comment"))
                    })
                    .map(|f| f.span)
                    .collect::<Vec<_>>();
                (*file, spans)
            })
            .filter(|(_, spans)| !spans.is_empty())
            .collect();
        if blocks.is_empty() {
            return;
        }
        let inside = |s: &Site| {
            blocks.get(s.file.as_str()).is_some_and(|spans| {
                spans.iter().any(|span| {
                    span.contains(Pos {
                        line: s.line,
                        col: s.col,
                    })
                })
            })
        };
        for e in self.edges.values_mut() {
            if e.certainty.is_some() && !e.sites.is_empty() && e.sites.iter().all(inside) {
                e.certainty = Some(Certainty::Possible);
            }
        }
    }
}
