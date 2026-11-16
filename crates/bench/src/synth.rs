//! A synthetic snapshot written straight from parameters, without source
//! files (WP5.1): units, namespaces, symbols and edges with a heavy-tailed
//! degree distribution, a certainty mix and a few very high fan-in hubs.

use std::collections::{BTreeSet, HashSet};

use aldente_analysis::measures::paths::Rng;
use aldente_graph::build::Graph;
use aldente_model::graph::Location;
use aldente_model::units::{RootKind, SourceRoot};
use aldente_model::{
    Certainty, Edge, EdgeKind, ExternalId, Lang, NamespaceId, Node, NodeId, NodeKind, Reason,
    RepoPath, Site, SymbolFlag, SymbolId, SymbolSubkind, UnitInfo,
};

use crate::dist::{Cumulative, pareto, split, unit};

/// What to generate. `edges` counts every edge, `contains` and `requires`
/// included; `symbols` counts repository symbols (a few `clojure.core`
/// externals come on top).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub units: usize,
    pub namespaces: usize,
    pub symbols: usize,
    pub edges: usize,
    /// Repository symbols that receive a large share of all calls.
    pub hubs: usize,
    /// Symbols in the largest namespace (WP5.2 pages through it).
    pub big_namespace: usize,
    pub seed: u64,
}

impl Default for Spec {
    /// The plan's sizes: 20 units, 8,000 namespaces, 300,000 symbols,
    /// 2,000,000 edges.
    ///
    /// Example: `Spec::default().symbols` is `300_000`.
    fn default() -> Spec {
        Spec {
            units: 20,
            namespaces: 8_000,
            symbols: 300_000,
            edges: 2_000_000,
            hubs: 5,
            big_namespace: 5_000,
            seed: 1,
        }
    }
}

/// A generated corpus: what a snapshot needs besides its key and origin.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub units: Vec<UnitInfo>,
    pub graph: Graph,
}

/// `clojure.core` functions every namespace may call; they are the
/// external hubs.
const CORE: &[&str] = &[
    "map",
    "filter",
    "reduce",
    "assoc",
    "dissoc",
    "get",
    "get-in",
    "update",
    "conj",
    "into",
    "str",
    "first",
    "rest",
    "seq",
    "count",
    "keys",
    "vals",
    "merge",
    "select-keys",
    "some",
    "let",
    "fn",
    "when",
    "if-let",
    "cond",
    "->",
    "->>",
    "println",
    "format",
    "keyword",
    "vec",
    "set",
    "mapv",
    "partial",
    "comp",
    "apply",
    "range",
    "atom",
    "deref",
    "swap!",
];

/// Edge kinds, certainties and reasons for invocation edges, with their
/// share. Roughly what the acceptance repositories show: most calls are
/// direct, a tail is dispatch and guesswork.
const MIX: &[(f64, EdgeKind, Certainty, Reason)] = &[
    (0.74, EdgeKind::Calls, Certainty::Certain, Reason::Direct),
    (
        0.08,
        EdgeKind::References,
        Certainty::Likely,
        Reason::ValueRef,
    ),
    (
        0.04,
        EdgeKind::Dispatches,
        Certainty::Likely,
        Reason::ProtocolDispatch,
    ),
    (0.04, EdgeKind::Calls, Certainty::Likely, Reason::ParamFlow),
    (
        0.05,
        EdgeKind::Dispatches,
        Certainty::Possible,
        Reason::MultimethodDispatch,
    ),
    (
        0.03,
        EdgeKind::Calls,
        Certainty::Possible,
        Reason::NameMatch,
    ),
    (
        0.02,
        EdgeKind::Dispatches,
        Certainty::Possible,
        Reason::KeywordDispatch,
    ),
];

/// Where a call's callee is drawn from, by cumulative probability.
const TO_REPO_HUB: f64 = 0.04;
const TO_CORE: f64 = 0.12;
const TO_SAME_NS: f64 = 0.55;
const TO_SAME_UNIT: f64 = 0.80;

/// Lines per generated symbol in the made-up source layout.
const LINES_PER_SYMBOL: u32 = 8;

/// One unit's place in the node table.
#[derive(Debug, Clone)]
struct UnitPlan {
    dir: RepoPath,
    service: bool,
    /// Global namespace indices.
    namespaces: std::ops::Range<usize>,
    /// Global symbol indices (repository symbols only).
    symbols: std::ops::Range<usize>,
}

/// One namespace's place in the node table.
#[derive(Debug, Clone)]
struct NsPlan {
    unit: usize,
    name: String,
    file: RepoPath,
    test: bool,
    symbols: std::ops::Range<usize>,
}

/// An edge by table index, turned into a model edge at the end.
#[derive(Debug, Clone)]
struct RawEdge {
    from: usize,
    to: usize,
    kind: EdgeKind,
    certainty: Option<Certainty>,
    reason: Option<Reason>,
    site: Option<(usize, u32)>,
}

/// Generates the corpus for `spec`, the same for the same spec.
///
/// Example: `synthesize(&Spec::default())?.graph.edges.len()` is `2_000_000`.
pub fn synthesize(spec: &Spec) -> anyhow::Result<Corpus> {
    validate(spec)?;
    let mut rng = Rng::new(spec.seed);
    let units = plan_units(spec)?;
    let namespaces = plan_namespaces(spec, &units, &mut rng)?;
    let units = attach_symbols(units, &namespaces);
    let table = Table::new(&units, &namespaces, &mut rng)?;
    let mut edges = contains_edges(&units, &namespaces, &table);
    let budget = spec.edges - edges.len();
    let requires = require_edges(&units, &namespaces, &table, budget / 5, &mut rng);
    edges.extend(requires);
    let calls = spec.edges - edges.len();
    edges.extend(call_edges(
        spec,
        &units,
        &namespaces,
        &table,
        calls,
        &mut rng,
    )?);
    Ok(Corpus {
        units: unit_infos(&units, &namespaces),
        graph: table.into_graph(edges, &namespaces),
    })
}

/// Refuses specs that cannot be met.
///
/// Example: 10 symbols in 60 namespaces fails ("symbols").
fn validate(spec: &Spec) -> anyhow::Result<()> {
    anyhow::ensure!(spec.units >= 1, "at least one unit");
    anyhow::ensure!(
        spec.namespaces >= spec.units,
        "namespaces ({}) must be at least units ({})",
        spec.namespaces,
        spec.units
    );
    anyhow::ensure!(spec.big_namespace >= 1, "big namespace needs a symbol");
    anyhow::ensure!(
        spec.symbols >= spec.big_namespace + spec.namespaces - 1,
        "symbols ({}) too few for {} namespaces with one of {}",
        spec.symbols,
        spec.namespaces,
        spec.big_namespace
    );
    let contains = spec.namespaces + spec.symbols;
    anyhow::ensure!(
        spec.edges >= contains * 2,
        "edges ({}) must be at least twice the contains edges ({contains})",
        spec.edges
    );
    Ok(())
}

/// Unit directories and namespace ranges: about three in five units are
/// services, the rest libraries; earlier units hold more namespaces.
///
/// Example: 4 units give `services/svc-00`, `services/svc-01`, `libs/lib-00`, `libs/lib-01`.
fn plan_units(spec: &Spec) -> anyhow::Result<Vec<UnitPlan>> {
    let services = (spec.units * 3 / 5).max(1);
    let weights: Vec<f64> = (0..spec.units)
        .map(|i| 1.0 / ((i + 1) as f64).sqrt())
        .collect();
    let sizes = split(spec.namespaces, &weights, 1)?;
    let mut start = 0;
    let mut units = Vec::new();
    for (i, size) in sizes.into_iter().enumerate() {
        let dir = if i < services {
            format!("services/svc-{i:02}")
        } else {
            format!("libs/lib-{:02}", i - services)
        };
        units.push(UnitPlan {
            dir: RepoPath::parse(&dir)?,
            service: i < services,
            namespaces: start..start + size,
            symbols: 0..0,
        });
        start += size;
    }
    Ok(units)
}

/// Namespace names, files and symbol ranges. The first namespace is the big
/// one; the others get heavy-tailed sizes. Every eighth namespace of a unit
/// is a test namespace.
///
/// Example: unit 0's namespace 7 is `synth.u00.n0007-test` in `services/svc-00/test/synth/u00/n0007_test.clj`.
fn plan_namespaces(spec: &Spec, units: &[UnitPlan], rng: &mut Rng) -> anyhow::Result<Vec<NsPlan>> {
    let rest: Vec<f64> = (1..spec.namespaces)
        .map(|_| pareto(rng, 2.0, 8.0))
        .collect();
    let mut sizes = vec![spec.big_namespace];
    sizes.extend(split(spec.symbols - spec.big_namespace, &rest, 1)?);
    let mut start = 0;
    let mut out = Vec::new();
    for (u, plan) in units.iter().enumerate() {
        for (k, g) in plan.namespaces.clone().enumerate() {
            let test = k % 8 == 7;
            let (name, file) = if test {
                (
                    format!("synth.u{u:02}.n{k:04}-test"),
                    format!("{}/test/synth/u{u:02}/n{k:04}_test.clj", plan.dir),
                )
            } else {
                (
                    format!("synth.u{u:02}.n{k:04}"),
                    format!("{}/src/synth/u{u:02}/n{k:04}.clj", plan.dir),
                )
            };
            out.push(NsPlan {
                unit: u,
                name,
                file: RepoPath::parse(&file)?,
                test,
                symbols: start..start + sizes[g],
            });
            start += sizes[g];
        }
    }
    Ok(out)
}

/// Fills each unit's symbol range from its namespaces.
///
/// Example: a unit with namespaces holding 0..40 and 40..90 gets 0..90.
fn attach_symbols(units: Vec<UnitPlan>, namespaces: &[NsPlan]) -> Vec<UnitPlan> {
    units
        .into_iter()
        .map(|u| {
            let first = namespaces[u.namespaces.start].symbols.start;
            let last = namespaces[u.namespaces.end - 1].symbols.end;
            UnitPlan {
                symbols: first..last,
                ..u
            }
        })
        .collect()
}

/// Every node, by table index: repository symbols first (`0..symbols`),
/// then namespaces, units and externals. Also the symbols' line numbers.
#[derive(Debug)]
struct Table {
    nodes: Vec<Node>,
    lines: Vec<u32>,
    symbols: usize,
    ns_base: usize,
    unit_base: usize,
    core_base: usize,
}

impl Table {
    /// Builds every node.
    ///
    /// Example: for the small test spec, `nodes.len()` is 3,000 + 60 + 4 + 41.
    fn new(units: &[UnitPlan], namespaces: &[NsPlan], rng: &mut Rng) -> anyhow::Result<Table> {
        let symbols = namespaces.last().map_or(0, |n| n.symbols.end);
        let mut nodes = Vec::with_capacity(symbols + namespaces.len() + units.len() + 41);
        let mut lines = Vec::with_capacity(symbols);
        let ns_ids: Vec<NamespaceId> = namespaces
            .iter()
            .map(|n| NamespaceId::new(Lang::Clj, units[n.unit].dir.clone(), &n.name))
            .collect::<Result<_, _>>()?;
        for (i, ns) in namespaces.iter().enumerate() {
            let main = units[ns.unit].service && units[ns.unit].namespaces.start == i;
            for (j, _) in ns.symbols.clone().enumerate() {
                let line = 3 + j as u32 * LINES_PER_SYMBOL;
                let name = if main && j == 0 {
                    "-main".to_owned()
                } else {
                    format!("f{j:05}")
                };
                let id = NodeId::Symbol(SymbolId::new(ns_ids[i].clone(), &name)?);
                nodes.push(symbol_node(
                    id,
                    name,
                    ns,
                    &units[ns.unit],
                    line,
                    main && j == 0,
                    rng,
                ));
                lines.push(line);
            }
        }
        let ns_base = nodes.len();
        for (i, ns) in namespaces.iter().enumerate() {
            nodes.push(namespace_node(ns_ids[i].clone(), ns, &units[ns.unit]));
        }
        let unit_base = nodes.len();
        nodes.extend(units.iter().map(unit_node));
        let core_base = nodes.len();
        nodes.push(external_node(NodeId::External(ExternalId::namespace(
            Lang::Clj,
            "clojure.core",
        )?)));
        for name in CORE {
            nodes.push(external_node(NodeId::External(ExternalId::new(
                Lang::Clj,
                "clojure.core",
                name,
            )?)));
        }
        Ok(Table {
            nodes,
            lines,
            symbols,
            ns_base,
            unit_base,
            core_base,
        })
    }

    /// Table indices of the `clojure.core` functions.
    ///
    /// Example: with 3,064 nodes before them, `3065..3105`.
    fn core(&self) -> std::ops::Range<usize> {
        self.core_base + 1..self.nodes.len()
    }

    /// The model graph: nodes keyed by ID, edges sorted as the builder
    /// sorts them (by from, to, kind, reason).
    ///
    /// Example: see [`synthesize`].
    fn into_graph(self, mut edges: Vec<RawEdge>, namespaces: &[NsPlan]) -> Graph {
        let mut order: Vec<usize> = (0..self.nodes.len()).collect();
        order.sort_unstable_by(|a, b| self.nodes[*a].id.cmp(&self.nodes[*b].id));
        let mut rank = vec![0; self.nodes.len()];
        for (r, i) in order.into_iter().enumerate() {
            rank[i] = r;
        }
        edges.sort_unstable_by_key(|e| (rank[e.from], rank[e.to], e.kind, e.reason));
        let symbol_ns = symbol_namespaces(namespaces, self.symbols);
        let edges = edges
            .into_iter()
            .map(|e| Edge {
                from: self.nodes[e.from].id.clone(),
                to: self.nodes[e.to].id.clone(),
                kind: e.kind,
                certainty: e.certainty,
                reason: e.reason,
                lang: None,
                sites: e
                    .site
                    .map(|(caller, offset)| Site {
                        file: namespaces[symbol_ns[caller]].file.clone(),
                        line: self.lines[caller] + offset,
                        col: 3,
                    })
                    .into_iter()
                    .collect(),
                count: None,
                by_certainty: None,
            })
            .collect();
        Graph {
            nodes: self.nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
            edges,
            ..Graph::default()
        }
    }
}

/// The namespace index of every repository symbol.
///
/// Example: namespaces holding 0..2 and 2..3 give `[0, 0, 1]`.
fn symbol_namespaces(namespaces: &[NsPlan], symbols: usize) -> Vec<usize> {
    let mut out = vec![0; symbols];
    for (i, ns) in namespaces.iter().enumerate() {
        for s in ns.symbols.clone() {
            out[s] = i;
        }
    }
    out
}

/// A repository symbol node: test symbols in test namespaces, otherwise
/// mostly functions with some vars, macros, multimethods and protocols.
///
/// Example: `clj:services/svc-00//synth.u00.n0001/f00003 symbol/fn`.
fn symbol_node(
    id: NodeId,
    name: String,
    ns: &NsPlan,
    owner: &UnitPlan,
    line: u32,
    main: bool,
    rng: &mut Rng,
) -> Node {
    let mut flags = BTreeSet::new();
    let subkind = if ns.test {
        flags.extend([SymbolFlag::Test, SymbolFlag::EntryPoint]);
        SymbolSubkind::Test
    } else {
        let roll = unit(rng);
        if main {
            flags.insert(SymbolFlag::EntryPoint);
        } else if roll < 0.2 {
            flags.insert(SymbolFlag::Private);
        }
        match roll {
            r if r < 0.85 => SymbolSubkind::Fn,
            r if r < 0.93 => SymbolSubkind::Var,
            r if r < 0.96 => SymbolSubkind::Macro,
            r if r < 0.98 => SymbolSubkind::Multimethod,
            _ => SymbolSubkind::Protocol,
        }
    };
    Node {
        id,
        kind: NodeKind::Symbol,
        subkind: Some(subkind),
        name,
        namespace: Some(ns.name.clone()),
        unit: Some(NodeId::Unit(owner.dir.clone())),
        langs: BTreeSet::from([Lang::Clj]),
        flags,
        loc: Some(Location {
            file: ns.file.clone(),
            line,
            col: 1,
            end_line: line + LINES_PER_SYMBOL - 2,
            end_col: 2,
        }),
        doc: None,
        signature: None,
        members: None,
        symbols: None,
    }
}

/// A namespace node.
///
/// Example: `clj:services/svc-00//synth.u00.n0000 namespace`.
fn namespace_node(id: NamespaceId, ns: &NsPlan, unit: &UnitPlan) -> Node {
    Node {
        id: NodeId::Namespace(id),
        kind: NodeKind::Namespace,
        subkind: None,
        name: ns.name.clone(),
        namespace: None,
        unit: Some(NodeId::Unit(unit.dir.clone())),
        langs: BTreeSet::from([Lang::Clj]),
        flags: if ns.test {
            BTreeSet::from([SymbolFlag::Test])
        } else {
            BTreeSet::new()
        },
        loc: Some(Location {
            file: ns.file.clone(),
            line: 1,
            col: 1,
            end_line: 1,
            end_col: 2,
        }),
        doc: None,
        signature: None,
        members: None,
        symbols: None,
    }
}

/// A unit node; services carry the `service` flag.
///
/// Example: `unit:services/svc-00 unit [service]`.
fn unit_node(unit: &UnitPlan) -> Node {
    Node {
        id: NodeId::Unit(unit.dir.clone()),
        kind: NodeKind::Unit,
        subkind: None,
        name: unit.dir.to_string(),
        namespace: None,
        unit: None,
        langs: BTreeSet::from([Lang::Clj]),
        flags: if unit.service {
            BTreeSet::from([SymbolFlag::Service])
        } else {
            BTreeSet::new()
        },
        loc: None,
        doc: None,
        signature: None,
        members: None,
        symbols: None,
    }
}

/// An external node (`clojure.core` or one of its functions).
///
/// Example: `ext:clj//clojure.core/map external`.
fn external_node(id: NodeId) -> Node {
    let (name, namespace) = match &id {
        NodeId::External(e) => match e.name() {
            Some(n) => (n.to_owned(), Some(e.ns().to_owned())),
            None => (e.ns().to_owned(), None),
        },
        other => (other.to_string(), None),
    };
    Node {
        id,
        kind: NodeKind::External,
        subkind: None,
        name,
        namespace,
        unit: None,
        langs: BTreeSet::from([Lang::Clj]),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
        members: None,
        symbols: None,
    }
}

/// Unit → namespace and namespace → symbol.
///
/// Example: the small spec gives 60 + 3,000 edges.
fn contains_edges(units: &[UnitPlan], namespaces: &[NsPlan], table: &Table) -> Vec<RawEdge> {
    let mut out = Vec::new();
    for (u, plan) in units.iter().enumerate() {
        for n in plan.namespaces.clone() {
            out.push(structural(
                table.unit_base + u,
                table.ns_base + n,
                EdgeKind::Contains,
            ));
        }
    }
    for (n, ns) in namespaces.iter().enumerate() {
        for s in ns.symbols.clone() {
            out.push(structural(table.ns_base + n, s, EdgeKind::Contains));
        }
    }
    out
}

/// An edge without certainty, reason or site.
///
/// Example: `structural(unit, ns, EdgeKind::Contains)`.
fn structural(from: usize, to: usize, kind: EdgeKind) -> RawEdge {
    RawEdge {
        from,
        to,
        kind,
        certainty: None,
        reason: None,
        site: None,
    }
}

/// Up to three requires per namespace onto earlier namespaces of its unit,
/// plus one onto a library namespace for service namespaces. No cycles.
/// At most `budget` edges.
///
/// Example: `synth.u00.n0003` may require `synth.u00.n0000` and `synth.u00.n0002`.
fn require_edges(
    units: &[UnitPlan],
    namespaces: &[NsPlan],
    table: &Table,
    budget: usize,
    rng: &mut Rng,
) -> Vec<RawEdge> {
    let libs: Vec<&UnitPlan> = units.iter().filter(|u| !u.service).collect();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (n, ns) in namespaces.iter().enumerate() {
        let first = units[ns.unit].namespaces.start;
        let mut targets: Vec<usize> = (0..3.min(n - first))
            .map(|_| first + rng.below(n - first))
            .collect();
        if units[ns.unit].service && !libs.is_empty() {
            let lib = libs[rng.below(libs.len())];
            targets.push(lib.namespaces.start + rng.below(lib.namespaces.len()));
        }
        for t in targets {
            if out.len() >= budget {
                return out;
            }
            if seen.insert((n, t)) {
                out.push(structural(
                    table.ns_base + n,
                    table.ns_base + t,
                    EdgeKind::Requires,
                ));
            }
        }
    }
    out
}

/// Exactly `count` distinct invocation edges between repository symbols and
/// `clojure.core`. Callers and callees have heavy-tailed weights; most
/// callees are near the caller; a few hubs take a large share.
///
/// Example: with the default spec, about 1.6 million edges.
fn call_edges(
    spec: &Spec,
    units: &[UnitPlan],
    namespaces: &[NsPlan],
    table: &Table,
    count: usize,
    rng: &mut Rng,
) -> anyhow::Result<Vec<RawEdge>> {
    let n = table.symbols;
    let out_w: Vec<f64> = (0..n).map(|_| pareto(rng, 1.6, 200.0)).collect();
    let in_w: Vec<f64> = (0..n).map(|_| pareto(rng, 1.2, 2_000.0)).collect();
    let (callers, callees) = (Cumulative::new(&out_w), Cumulative::new(&in_w));
    let core = table.core();
    let core_w: Vec<f64> = core.clone().map(|_| pareto(rng, 1.0, 100.0)).collect();
    let core_pick = Cumulative::new(&core_w);
    let hubs = pick_hubs(spec.hubs, units, rng);
    let symbol_ns = symbol_namespaces(namespaces, n);
    let mut seen: HashSet<(u32, u32, u8)> = HashSet::with_capacity(count);
    let mut out = Vec::with_capacity(count);
    let mut attempts = 0usize;
    let limit = count.saturating_mul(20) + 1_000;
    while out.len() < count {
        attempts += 1;
        anyhow::ensure!(
            attempts <= limit,
            "could place only {} of {count} call edges; lower --edges or raise --symbols",
            out.len()
        );
        let from = callers.pick(rng, 0..n);
        let ns = &namespaces[symbol_ns[from]];
        let roll = unit(rng);
        let to = if roll < TO_REPO_HUB && !hubs.is_empty() {
            hubs[rng.below(hubs.len())]
        } else if roll < TO_CORE {
            core.start + core_pick.pick(rng, 0..core.len())
        } else if roll < TO_SAME_NS {
            callees.pick(rng, ns.symbols.clone())
        } else if roll < TO_SAME_UNIT {
            callees.pick(rng, units[ns.unit].symbols.clone())
        } else {
            callees.pick(rng, 0..n)
        };
        let (kind, certainty, reason) = if to >= core.start {
            (EdgeKind::Calls, Certainty::Certain, Reason::Direct)
        } else {
            pick_mix(rng)
        };
        if from == to || !seen.insert((from as u32, to as u32, mix_index(kind, reason))) {
            continue;
        }
        out.push(RawEdge {
            from,
            to,
            kind,
            certainty: Some(certainty),
            reason: Some(reason),
            site: Some((from, 1 + rng.below(5) as u32)),
        });
    }
    Ok(out)
}

/// `count` hub symbols, from library units when there are any.
///
/// Example: with one library unit holding symbols 900..1000, hubs lie in that range.
fn pick_hubs(count: usize, units: &[UnitPlan], rng: &mut Rng) -> Vec<usize> {
    let libs: Vec<&UnitPlan> = units.iter().filter(|u| !u.service).collect();
    let pool = if libs.is_empty() {
        units.iter().collect()
    } else {
        libs
    };
    (0..count)
        .map(|_| {
            let u = pool[rng.below(pool.len())];
            u.symbols.start + rng.below(u.symbols.len())
        })
        .collect()
}

/// A kind, certainty and reason drawn from [`MIX`].
///
/// Example: three times in four, `(Calls, Certain, Direct)`.
fn pick_mix(rng: &mut Rng) -> (EdgeKind, Certainty, Reason) {
    let mut roll = unit(rng);
    for (share, kind, certainty, reason) in MIX {
        if roll < *share {
            return (*kind, *certainty, *reason);
        }
        roll -= share;
    }
    (EdgeKind::Calls, Certainty::Certain, Reason::Direct)
}

/// A small number identifying a (kind, reason) pair, for duplicate checks.
///
/// Example: `(Calls, Direct)` is 0.
fn mix_index(kind: EdgeKind, reason: Reason) -> u8 {
    MIX.iter()
        .position(|(_, k, _, r)| *k == kind && *r == reason)
        .unwrap_or(0) as u8
}

/// Unit records as discovery would report them.
///
/// Example: `services/svc-00` with `deps.edn`, roots `src` and `test`, `service: true`.
fn unit_infos(units: &[UnitPlan], namespaces: &[NsPlan]) -> Vec<UnitInfo> {
    units
        .iter()
        .map(|u| {
            let root = |sub: &str, kind| SourceRoot {
                path: RepoPath::parse(&format!("{}/{sub}", u.dir))
                    .unwrap_or_else(|_| u.dir.clone()),
                kind,
                manifest: Some("deps.edn".to_owned()),
                alias: None,
            };
            UnitInfo {
                id: NodeId::Unit(u.dir.clone()),
                dir: u.dir.clone(),
                manifests: vec!["deps.edn".to_owned()],
                langs: BTreeSet::from([Lang::Clj]),
                roots: vec![root("src", RootKind::Source), root("test", RootKind::Test)],
                service: u.service,
                entry_points: Vec::new(),
                deps: Vec::new(),
                files: namespaces[u.namespaces.clone()].len(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use aldente_model::{Certainty, EdgeKind, NodeKind, SymbolFlag};

    use super::*;

    fn small() -> Spec {
        Spec {
            units: 4,
            namespaces: 60,
            symbols: 3_000,
            edges: 25_000,
            hubs: 2,
            big_namespace: 400,
            seed: 9,
        }
    }

    fn count(graph: &Graph, kind: NodeKind) -> usize {
        graph.nodes.values().filter(|n| n.kind == kind).count()
    }

    #[test]
    fn sizes_are_exact() {
        let c = synthesize(&small()).unwrap();
        assert_eq!(c.units.len(), 4);
        assert_eq!(count(&c.graph, NodeKind::Unit), 4);
        assert_eq!(count(&c.graph, NodeKind::Namespace), 60);
        let repo_symbols = c
            .graph
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Symbol)
            .count();
        assert_eq!(repo_symbols, 3_000);
        assert_eq!(c.graph.edges.len(), 25_000);
    }

    #[test]
    fn every_edge_names_existing_nodes_and_is_distinct() {
        let c = synthesize(&small()).unwrap();
        let mut keys = HashSet::new();
        for e in &c.graph.edges {
            assert!(c.graph.nodes.contains_key(&e.from), "{}", e.from);
            assert!(c.graph.nodes.contains_key(&e.to), "{}", e.to);
            assert_ne!(e.from, e.to);
            assert!(keys.insert((&e.from, &e.to, e.kind, e.reason, e.lang)));
        }
        let sorted = c.graph.edges.windows(2).all(|w| {
            (&w[0].from, &w[0].to, w[0].kind, w[0].reason)
                <= (&w[1].from, &w[1].to, w[1].kind, w[1].reason)
        });
        assert!(sorted, "edges sorted like the builder sorts them");
    }

    #[test]
    fn same_seed_same_corpus_other_seed_other_corpus() {
        let a = synthesize(&small()).unwrap();
        let b = synthesize(&small()).unwrap();
        assert_eq!(a.graph, b.graph);
        let c = synthesize(&Spec {
            seed: 10,
            ..small()
        })
        .unwrap();
        assert_ne!(a.graph.edges, c.graph.edges);
    }

    #[test]
    fn structure_looks_like_the_builder_output() {
        let c = synthesize(&small()).unwrap();
        let g = &c.graph;
        // Every namespace sits in one unit; every symbol in one namespace.
        let contains_to: BTreeMap<_, usize> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .fold(BTreeMap::new(), |mut m, e| {
                *m.entry(&e.to).or_default() += 1;
                m
            });
        for n in g.nodes.values() {
            let expected = matches!(n.kind, NodeKind::Namespace | NodeKind::Symbol) as usize;
            assert_eq!(
                contains_to.get(&n.id).copied().unwrap_or(0),
                expected,
                "{}",
                n.id
            );
        }
        assert!(g.edges.iter().any(|e| e.kind == EdgeKind::Requires));
        // Invocation edges carry a certainty, a reason and one site.
        for e in g.edges.iter().filter(|e| {
            matches!(
                e.kind,
                EdgeKind::Calls | EdgeKind::References | EdgeKind::Dispatches
            )
        }) {
            assert!(e.certainty.is_some() && e.reason.is_some(), "{e:?}");
            assert_eq!(e.sites.len(), 1);
        }
        let services = g
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Unit && n.flags.contains(&SymbolFlag::Service))
            .count();
        assert!(services > 0 && services < 4);
        assert!(
            g.nodes
                .values()
                .any(|n| n.flags.contains(&SymbolFlag::Test))
        );
        assert!(
            g.nodes
                .values()
                .any(|n| n.flags.contains(&SymbolFlag::EntryPoint))
        );
    }

    #[test]
    fn certainty_mix_and_hubs() {
        let c = synthesize(&small()).unwrap();
        let g = &c.graph;
        let mut by: BTreeMap<Certainty, usize> = BTreeMap::new();
        let mut fan_in: BTreeMap<_, usize> = BTreeMap::new();
        let mut invocations = 0;
        for e in &g.edges {
            if let Some(cert) = e.certainty {
                *by.entry(cert).or_default() += 1;
                *fan_in.entry(&e.to).or_default() += 1;
                invocations += 1;
            }
        }
        let share = |c| by.get(&c).copied().unwrap_or(0) as f64 / invocations as f64;
        assert!(share(Certainty::Certain) > 0.6, "{by:?}");
        assert!(share(Certainty::Likely) > 0.05, "{by:?}");
        assert!(share(Certainty::Possible) > 0.03, "{by:?}");
        let mean = invocations as f64 / g.nodes.len() as f64;
        let mut top: Vec<usize> = fan_in.values().copied().collect();
        top.sort_unstable_by(|a, b| b.cmp(a));
        assert!(top[0] as f64 > 30.0 * mean, "top {} mean {mean}", top[0]);
        // A long tail: half of the nodes with any callers have very few.
        let median = top[top.len() / 2];
        assert!((median as f64) < mean * 2.0, "median {median} mean {mean}");
    }

    #[test]
    fn largest_namespace_has_the_requested_size() {
        let c = synthesize(&small()).unwrap();
        let mut per_ns: BTreeMap<_, usize> = BTreeMap::new();
        for e in c
            .graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
        {
            if c.graph.nodes[&e.from].kind == NodeKind::Namespace {
                *per_ns.entry(&e.from).or_default() += 1;
            }
        }
        assert_eq!(per_ns.values().max().copied(), Some(400));
    }

    #[test]
    fn impossible_specs_fail_with_a_reason() {
        let too_few_edges = Spec {
            edges: 100,
            ..small()
        };
        let e = synthesize(&too_few_edges).unwrap_err().to_string();
        assert!(e.contains("edges"), "{e}");
        let too_few_symbols = Spec {
            symbols: 10,
            ..small()
        };
        assert!(synthesize(&too_few_symbols).is_err());
    }
}
