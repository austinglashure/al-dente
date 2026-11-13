//! Library impact: which repository code uses each library.
//! See "Library impact" in `docs/plan/08-crater-and-traces.md`.

use std::collections::{BTreeMap, BTreeSet};

use aldente_graph::Graph;
use aldente_model::libraries::{
    Attribution, DeclaredVersion, LibrariesReport, LibrarySummary, LibraryUsage, UsageGroup,
    UsageGrouping, UsageRow,
};
use aldente_model::{EdgeKind, NodeId, NodeKind, UnitInfo};

/// Edge kinds that count as using a library.
const USES: &[EdgeKind] = &[
    EdgeKind::Calls,
    EdgeKind::References,
    EdgeKind::Dispatches,
    EdgeKind::Registers,
    EdgeKind::Requires,
];

/// The outcome of looking a library up by what the user typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryLookup {
    Found(NodeId),
    NotFound,
    Ambiguous(Vec<NodeId>),
}

/// Finds a library by full ID (`lib:npm:react`), coordinate
/// (`reagent/reagent`) or artifact name (`reagent`).
///
/// Example: `find_library(g, "reagent")` is `Found(lib:maven:reagent/reagent)`.
pub fn find_library(graph: &Graph, query: &str) -> LibraryLookup {
    let libs = graph.nodes.values().filter(|n| n.kind == NodeKind::Library);
    let matches: Vec<NodeId> = libs
        .filter(|n| {
            let NodeId::Library(l) = &n.id else {
                return false;
            };
            let coord = l.coordinate();
            let artifact = coord.rsplit('/').next().unwrap_or(coord);
            n.id.to_string() == query || coord == query || artifact == query
        })
        .map(|n| n.id.clone())
        .collect();
    match matches.as_slice() {
        [] => LibraryLookup::NotFound,
        [one] => LibraryLookup::Found(one.clone()),
        many => match many.iter().find(|id| id.to_string() == query) {
            Some(exact) => LibraryLookup::Found(exact.clone()),
            None => LibraryLookup::Ambiguous(many.to_vec()),
        },
    }
}

/// What a library provides: its `provides` edges, and every external whose
/// namespace is one it provides (calls land on `ext:…//ns/name`, while
/// attribution names `ext:…//ns`).
///
/// Example: reagent provides `ext:clj//reagent.core`, so
/// `ext:clj//reagent.core/atom` is one of its externals.
pub fn provided(graph: &Graph, lib: &NodeId) -> (Vec<Attribution>, BTreeSet<NodeId>) {
    let mut attribution: Vec<Attribution> = graph
        .edges
        .iter()
        .filter(|e| &e.from == lib && e.kind == EdgeKind::Provides)
        .map(|e| Attribution {
            provides: e.to.clone(),
            certainty: e.certainty,
            reason: e.reason,
        })
        .collect();
    attribution.sort();
    let namespaces: BTreeSet<(aldente_model::Lang, &str)> = attribution
        .iter()
        .filter_map(|a| match &a.provides {
            NodeId::External(x) => Some((x.lang(), x.ns())),
            _ => None,
        })
        .collect();
    let mut targets: BTreeSet<NodeId> = attribution.iter().map(|a| a.provides.clone()).collect();
    for id in graph.nodes.keys() {
        if let NodeId::External(x) = id
            && namespaces.contains(&(x.lang(), x.ns()))
        {
            targets.insert(id.clone());
        }
    }
    (attribution, targets)
}

/// The repository namespace a using node belongs to.
///
/// Example: `clj:.//lib.views/mount!` gives `clj:.//lib.views`.
fn namespace_of(id: &NodeId) -> Option<NodeId> {
    match id {
        NodeId::Namespace(_) => Some(id.clone()),
        NodeId::Symbol(s) => Some(NodeId::Namespace(s.namespace().clone())),
        NodeId::Impl(i) => Some(NodeId::Namespace(i.symbol().namespace().clone())),
        _ => None,
    }
}

/// Every use of the library at one site per row: edges of the use kinds from
/// repository namespaces, symbols and impls into what it provides.
///
/// Example: in the libraries fixture, reagent has 5 rows (3 calls, 2 requires
/// from `lib.views`) plus `lib.state`'s require.
fn rows(graph: &Graph, targets: &BTreeSet<NodeId>) -> Vec<UsageRow> {
    let mut rows = Vec::new();
    for e in &graph.edges {
        if !targets.contains(&e.to) || !USES.contains(&e.kind) || namespace_of(&e.from).is_none() {
            continue;
        }
        let sites: Vec<Option<aldente_model::Site>> = if e.sites.is_empty() {
            vec![None]
        } else {
            e.sites.iter().cloned().map(Some).collect()
        };
        for site in sites {
            rows.push(UsageRow {
                from: e.from.clone(),
                kind: e.kind,
                to: e.to.clone(),
                certainty: e.certainty,
                reason: e.reason,
                site,
            });
        }
    }
    rows.sort();
    rows
}

/// `lib usage`: the library's uses grouped by namespace or by using symbol
/// (a namespace's `requires` rows stay under the namespace), and the
/// namespaces that require it without calling into it.
///
/// Example: `usage(g, reagent, Namespace)` groups `lib.views` (5 rows) and
/// lists `lib.state` under `require_only`.
pub fn usage(graph: &Graph, lib: &NodeId, by: UsageGrouping) -> LibraryUsage {
    let (attribution, targets) = provided(graph, lib);
    let all = rows(graph, &targets);
    let calling: BTreeSet<NodeId> = all
        .iter()
        .filter(|r| r.kind != EdgeKind::Requires)
        .filter_map(|r| namespace_of(&r.from))
        .collect();
    let mut groups: BTreeMap<NodeId, Vec<UsageRow>> = BTreeMap::new();
    let mut require_only: BTreeMap<NodeId, Vec<UsageRow>> = BTreeMap::new();
    for r in all {
        let Some(ns) = namespace_of(&r.from) else {
            continue;
        };
        if !calling.contains(&ns) {
            require_only.entry(ns).or_default().push(r);
            continue;
        }
        let key = match by {
            UsageGrouping::Namespace => ns,
            UsageGrouping::Symbol => r.from.clone(),
        };
        groups.entry(key).or_default().push(r);
    }
    let into_groups = |m: BTreeMap<NodeId, Vec<UsageRow>>| {
        m.into_iter()
            .map(|(id, rows)| UsageGroup { id, rows })
            .collect::<Vec<_>>()
    };
    LibraryUsage {
        library: lib.clone(),
        by,
        attribution,
        groups: into_groups(groups),
        require_only: into_groups(require_only),
    }
}

/// `lib ls`: every library node with its users counted and the versions
/// units declare for it.
///
/// Example: in the libraries fixture, reagent is used by 3 symbols in 2
/// namespaces of 1 unit and declared as `1.2.0` by `unit:.`.
pub fn summaries(graph: &Graph, units: &[UnitInfo]) -> LibrariesReport {
    let libraries = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Library)
        .map(|n| {
            let (attribution, targets) = provided(graph, &n.id);
            let used = rows(graph, &targets);
            let symbols: BTreeSet<&NodeId> = used
                .iter()
                .filter(|r| !matches!(r.from, NodeId::Namespace(_)))
                .map(|r| &r.from)
                .collect();
            let namespaces: BTreeSet<NodeId> =
                used.iter().filter_map(|r| namespace_of(&r.from)).collect();
            let units_used: BTreeSet<&NodeId> = namespaces
                .iter()
                .filter_map(|ns| graph.nodes.get(ns).and_then(|n| n.unit.as_ref()))
                .collect();
            let (ecosystem, coordinate) = match &n.id {
                NodeId::Library(l) => (l.ecosystem().to_owned(), l.coordinate().to_owned()),
                _ => (String::new(), String::new()),
            };
            let mut versions: Vec<DeclaredVersion> = units
                .iter()
                .flat_map(|u| {
                    u.deps
                        .iter()
                        .filter(|d| ecosystem == "maven" && d.coord == coordinate)
                        .map(|d| DeclaredVersion {
                            unit: u.id.clone(),
                            version: d.version.clone(),
                        })
                })
                .collect();
            versions.sort();
            LibrarySummary {
                id: n.id.clone(),
                ecosystem,
                coordinate,
                attribution,
                symbols: symbols.len() as u64,
                namespaces: namespaces.len() as u64,
                units: units_used.len() as u64,
                versions,
            }
        })
        .collect();
    LibrariesReport { libraries }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_testkit::golden::{analyze_fixture, fixture_dir};
    use std::str::FromStr;

    fn id(s: &str) -> NodeId {
        NodeId::from_str(s).unwrap()
    }

    fn site(r: &UsageRow) -> String {
        r.site
            .as_ref()
            .map(|s| format!("{}:{}:{}", s.file, s.line, s.col))
            .unwrap_or_default()
    }

    #[test]
    fn reagent_usage_matches_expected_md() {
        let a = analyze_fixture(&fixture_dir("clj-rules/libraries"));
        let LibraryLookup::Found(lib) = find_library(&a.graph, "reagent") else {
            panic!("reagent not found");
        };
        assert_eq!(lib, id("lib:maven:reagent/reagent"));
        let u = usage(&a.graph, &lib, UsageGrouping::Namespace);
        let got: Vec<(String, EdgeKind, String, String)> = u
            .groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .map(|r| (r.from.to_string(), r.kind, r.to.to_string(), site(r)))
            .collect();
        let want = [
            (
                "clj:.//lib.views",
                EdgeKind::Requires,
                "ext:clj//reagent.core",
                "src/lib/views.cljs:3:14",
            ),
            (
                "clj:.//lib.views",
                EdgeKind::Requires,
                "ext:clj//reagent.dom",
                "src/lib/views.cljs:4:14",
            ),
            (
                "clj:.//lib.views/counter",
                EdgeKind::Calls,
                "ext:clj//reagent.core/atom",
                "src/lib/views.cljs:7:18",
            ),
            (
                "clj:.//lib.views/mount!",
                EdgeKind::Calls,
                "ext:clj//reagent.dom/render",
                "src/lib/views.cljs:13:3",
            ),
            (
                "clj:.//lib.views/wrapped",
                EdgeKind::Calls,
                "ext:clj//reagent.core/as-element",
                "src/lib/views.cljs:16:3",
            ),
        ]
        .map(|(a, k, b, s)| (a.to_owned(), k, b.to_owned(), s.to_owned()));
        let mut got_sorted = got.clone();
        got_sorted.sort();
        let mut want_sorted = want.to_vec();
        want_sorted.sort();
        assert_eq!(got_sorted, want_sorted);
        assert_eq!(u.require_only.len(), 1);
        assert_eq!(u.require_only[0].id, id("clj:.//lib.state"));
        assert_eq!(site(&u.require_only[0].rows[0]), "src/lib/state.cljs:3:14");

        let by_symbol = usage(&a.graph, &lib, UsageGrouping::Symbol);
        let keys: Vec<String> = by_symbol.groups.iter().map(|g| g.id.to_string()).collect();
        assert_eq!(
            keys,
            [
                "clj:.//lib.views",
                "clj:.//lib.views/counter",
                "clj:.//lib.views/mount!",
                "clj:.//lib.views/wrapped"
            ]
        );

        let ls = summaries(&a.graph, &a.discovery.report.units);
        let reagent = ls.libraries.iter().find(|l| l.id == lib).unwrap();
        assert_eq!(
            (reagent.symbols, reagent.namespaces, reagent.units),
            (3, 2, 1)
        );
        assert_eq!(reagent.versions.len(), 1);
        assert_eq!(reagent.versions[0].version.as_deref(), Some("1.2.0"));
        assert_eq!(find_library(&a.graph, "nope"), LibraryLookup::NotFound);
    }
}
