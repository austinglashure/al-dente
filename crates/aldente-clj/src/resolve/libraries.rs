//! R10: library attribution of external namespaces.

use super::*;

impl<'a> Resolver<'a> {
    /// R10: a `provides` edge into every external namespace (created at
    /// namespace level if only its symbols were used), from the first rung
    /// of the ladder that matches. npm packages already have theirs.
    ///
    /// Example: `ext:clj//clojure.string` gets
    /// `lib:maven:org.clojure/clojure -provides(certain direct)->`.
    pub(super) fn libraries(&mut self) {
        let provided: BTreeSet<NodeId> = self
            .edges
            .values()
            .filter(|e| e.kind == EdgeKind::Provides)
            .map(|e| e.to.clone())
            .collect();
        let namespaces: BTreeSet<String> = self
            .nodes
            .keys()
            .filter_map(|id| match id {
                NodeId::External(e) => Some(e.ns().to_owned()),
                _ => None,
            })
            .collect();
        for ns in namespaces {
            let Some(ext) = self.external(&ns, None) else {
                continue;
            };
            if provided.contains(&ext) {
                continue;
            }
            let found = self.attribute(&ns);
            if found.is_empty() {
                self.provides(
                    LibraryId::unknown(),
                    &ext,
                    Certainty::Possible,
                    Reason::NameMatch,
                );
                self.blindspots.push(BlindSpot {
                    category: BlindSpotCategory::UnresolvedSymbol,
                    node: Some(ext.clone()),
                    site: None,
                    message: format!(
                        "no declared dependency or known library provides namespace {ns}"
                    ),
                });
            }
            for (coord, certainty, reason) in found {
                if let Ok(lib) = LibraryId::new("maven", &coord) {
                    self.provides(lib, &ext, certainty, reason);
                }
            }
        }
    }

    /// The R10 ladder for one namespace: jar listing (certain), the prefix
    /// table (likely), the core rule (certain), then the declared
    /// coordinates' prefixes (likely; possible for each of a tie).
    ///
    /// Example: `malli.core` with `metosin/malli` declared gives
    /// `[("metosin/malli", Likely, NameMatch)]`.
    pub(super) fn attribute(&self, ns: &str) -> Vec<(String, Certainty, Reason)> {
        let rules = &self.ctx.rules.namespaces;
        if let Some(c) = self.ctx.jars.get(ns) {
            return vec![(c.clone(), Certainty::Certain, Reason::Direct)];
        }
        if let Some(c) = rules.table_for(ns) {
            return vec![(c.to_owned(), Certainty::Likely, Reason::NameMatch)];
        }
        if let Some(c) = rules.core_for(ns) {
            return vec![(c.to_owned(), Certainty::Certain, Reason::Direct)];
        }
        let scored: Vec<(usize, &str)> = self
            .unit
            .deps
            .iter()
            .filter_map(|d| {
                coordinate_prefixes(&d.coord)
                    .into_iter()
                    .filter(|p| rules_clj::prefix_matches(ns, p))
                    .map(|p| p.len())
                    .max()
                    .map(|len| (len, d.coord.as_str()))
            })
            .collect();
        let Some(best) = scored.iter().map(|(len, _)| *len).max() else {
            return Vec::new();
        };
        let winners: BTreeSet<&str> = scored
            .iter()
            .filter(|(len, _)| *len == best)
            .map(|(_, c)| *c)
            .collect();
        let certainty = if winners.len() == 1 {
            Certainty::Likely
        } else {
            Certainty::Possible
        };
        winners
            .into_iter()
            .map(|c| (c.to_owned(), certainty, Reason::NameMatch))
            .collect()
    }

    /// A library node and its `provides` edge.
    ///
    /// Example: `provides(maven reagent/reagent, ext:clj//reagent.core, Likely, NameMatch)`.
    pub(super) fn provides(
        &mut self,
        lib: LibraryId,
        to: &NodeId,
        certainty: Certainty,
        reason: Reason,
    ) {
        let name = if lib.coordinate().is_empty() {
            "unknown".to_owned()
        } else {
            lib.coordinate().to_owned()
        };
        let id = NodeId::Library(lib);
        self.nodes.entry(id.clone()).or_insert_with(|| Node {
            id: id.clone(),
            kind: NodeKind::Library,
            subkind: None,
            name,
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
            members: None,
            symbols: None,
        });
        self.edge(
            id,
            to.clone(),
            EdgeKind::Provides,
            Some(certainty),
            Some(reason),
            None,
            None,
        );
    }
}
