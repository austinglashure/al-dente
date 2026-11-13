//! `aldente metrics`, `cycles`, `communities` and `deadcode`: graph
//! measures as tables. Results are kept in the snapshot, so a second run
//! reads them back.

use aldente_analysis::measures::cache::{self, ResultStore};
use aldente_analysis::measures::{ProjectionError, SecondOptions};
use aldente_model::measures::{
    CommunitiesReport, CyclesReport, DeadCodeReport, Member, MetricsReport, ProjectionKind,
    ProjectionSpec,
};
use aldente_model::{AggregationLevel, Envelope, ErrorCode, ExitCode, Warning, WarningCode};
use clap::{Args, ValueEnum};
use serde::Serialize;

use crate::commands::loaded::{Loaded, load};
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable};

/// Level on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Level {
    Symbol,
    Namespace,
    Unit,
}

impl From<Level> for AggregationLevel {
    fn from(l: Level) -> Self {
        match l {
            Level::Symbol => AggregationLevel::Symbol,
            Level::Namespace => AggregationLevel::Namespace,
            Level::Unit => AggregationLevel::Unit,
        }
    }
}

/// Projection kind on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    /// calls, references, dispatches, registers.
    Invocation,
    /// requires between namespaces, depends between units.
    Structure,
}

/// Projection options shared by the measure commands.
#[derive(Debug, Args)]
pub struct ProjectionArgs {
    /// Include test code (excluded by default: test helpers look central).
    #[arg(long)]
    include_tests: bool,
    /// Include external libraries and platform code.
    #[arg(long)]
    include_externals: bool,
}

/// Arguments for `aldente metrics`.
#[derive(Debug, Args)]
pub struct MetricsArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    projection: ProjectionArgs,
    #[arg(long, value_enum, default_value = "symbol")]
    level: Level,
    #[arg(long, value_enum, default_value = "invocation")]
    kind: Kind,
    /// Columns, comma-separated (fan-in, fan-out, pagerank,
    /// pagerank-reverse, hub, authority, core, betweenness, closeness,
    /// eigenvector, community, dominates, god-score).
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "fan-in,fan-out,pagerank,betweenness,god-score"
    )]
    metric: Vec<String>,
    /// Sort column (descending).
    #[arg(long, default_value = "pagerank")]
    sort: String,
    /// Rows in text output (JSON is always complete).
    #[arg(long, default_value_t = 20)]
    top: usize,
}

/// Arguments for `aldente cycles`.
#[derive(Debug, Args)]
pub struct CyclesArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    projection: ProjectionArgs,
    #[arg(long, value_enum, default_value = "namespace")]
    level: Level,
    /// Default: structure (requires, depends) at namespace and unit level,
    /// invocation at symbol level.
    #[arg(long, value_enum)]
    kind: Option<Kind>,
}

/// Arguments for `aldente communities`.
#[derive(Debug, Args)]
pub struct CommunitiesArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[command(flatten)]
    projection: ProjectionArgs,
    #[arg(long, value_enum, default_value = "symbol")]
    level: Level,
    /// Seed of the community search (same seed, same result).
    #[arg(long, default_value_t = aldente_analysis::measures::louvain::DEFAULT_SEED)]
    seed: u64,
}

/// Arguments for `aldente deadcode`.
#[derive(Debug, Args)]
pub struct DeadcodeArgs {
    #[command(flatten)]
    target: TargetArgs,
}

/// A projection spec from the command line.
///
/// Example: `--level namespace --kind structure` gives a requires projection.
fn spec(p: &ProjectionArgs, level: Level, kind: Kind) -> ProjectionSpec {
    ProjectionSpec {
        kind: match kind {
            Kind::Invocation => ProjectionKind::Invocation,
            Kind::Structure => ProjectionKind::Structure,
        },
        level: level.into(),
        include_tests: p.include_tests,
        include_externals: p.include_externals,
        ..ProjectionSpec::default()
    }
}

/// Envelope warnings for results that could not be kept.
///
/// Example: a read-only cache gives one `not-cached` warning.
fn kept_warnings(loaded: &Loaded, kept: impl IntoIterator<Item = String>) -> Vec<Warning> {
    let mut w = loaded.warnings();
    w.extend(kept.into_iter().map(|m| Warning {
        code: WarningCode::NotCached,
        message: format!("a measure could not be kept in the snapshot: {m}"),
    }));
    w
}

/// Prints a result as JSON or text.
///
/// Example: `emit("cycles", &loaded, &report, warnings, lines, json)`.
fn emit<T: Serialize>(
    tag: &str,
    loaded: &Loaded,
    data: &T,
    warnings: Vec<Warning>,
    lines: Vec<String>,
    json: bool,
) -> anyhow::Result<ExitCode> {
    if json {
        let mut env = Envelope::success(tag, Some(loaded.info()), data);
        env.warnings = warnings;
        print_json(&env)?;
    } else {
        for w in &warnings {
            eprintln!("aldente: warning: {}", printable(&w.message));
        }
        print_lines(
            lines
                .iter()
                .map(|l| printable(l))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str),
        )?;
    }
    Ok(ExitCode::Success)
}

/// A projection error as a usage failure.
///
/// Example: `--level symbol --kind structure` exits 2.
fn usage(e: ProjectionError) -> Failure {
    Failure::new(ErrorCode::Usage, e.to_string())
}

/// Runs `aldente metrics`.
///
/// Example: `aldente metrics --sort betweenness --top 10`.
pub fn metrics(args: &MetricsArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "metrics";
    let columns: Vec<&str> = args.metric.iter().map(String::as_str).collect();
    if let Some(bad) = columns
        .iter()
        .chain([&args.sort.as_str()])
        .find(|c| !KNOWN.contains(c))
    {
        return fail(
            TAG,
            &Failure::new(
                ErrorCode::Usage,
                format!("unknown metric {bad:?}; known: {}", KNOWN.join(", ")),
            ),
            json,
        );
    }
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let s = spec(&args.projection, args.level, args.kind);
    let store = loaded.store();
    let both = cache::measures(graph, &s, store.as_ref()).and_then(|(first, w1)| {
        cache::second(graph, &s, SecondOptions::default(), store.as_ref())
            .map(|(second, w2)| (MetricsReport { first, second }, [w1, w2]))
    });
    let (report, kept) = match both {
        Ok(v) => v,
        Err(e) => return fail(TAG, &usage(e), json),
    };
    let lines = render_metrics(&report, &columns, &args.sort, args.top);
    emit(
        TAG,
        &loaded,
        &report,
        kept_warnings(&loaded, kept.into_iter().flatten()),
        lines,
        json,
    )
}

/// Column names `metrics` knows.
const KNOWN: &[&str] = &[
    "fan-in",
    "fan-out",
    "pagerank",
    "pagerank-reverse",
    "hub",
    "authority",
    "core",
    "betweenness",
    "closeness",
    "eigenvector",
    "community",
    "dominates",
    "god-score",
];

/// One column's value for a row, or `None` for an unknown name.
///
/// Example: `column(Some(first), Some(second), "pagerank")` is the PageRank.
fn column(
    f: Option<&aldente_model::measures::NodeMeasures>,
    s: Option<&aldente_model::measures::NodeMeasures2>,
    name: &str,
) -> Option<f64> {
    let (f, s) = (f?, s?);
    Some(match name {
        "fan-in" => f64::from(f.fan_in),
        "fan-out" => f64::from(f.fan_out),
        "pagerank" => f.pagerank,
        "pagerank-reverse" => f.pagerank_reverse,
        "hub" => f.hub,
        "authority" => f.authority,
        "core" => f64::from(f.core),
        "betweenness" => s.betweenness,
        "closeness" => s.closeness,
        "eigenvector" => s.eigenvector,
        "community" => f64::from(s.community),
        "dominates" => f64::from(s.dominates),
        "god-score" => s.god_score,
        _ => return None,
    })
}

/// The metrics table, sorted by `sort` descending, then by ID.
///
/// Example: a header `pagerank  betweenness  node` and one row per node.
fn render_metrics(r: &MetricsReport, columns: &[&str], sort: &str, top: usize) -> Vec<String> {
    let mut rows: Vec<(Vec<f64>, f64, String)> = r
        .first
        .nodes
        .iter()
        .zip(&r.second.nodes)
        .map(|(f, s)| {
            let vals = columns
                .iter()
                .map(|c| column(Some(f), Some(s), c).unwrap_or(0.0))
                .collect();
            (
                vals,
                column(Some(f), Some(s), sort).unwrap_or(0.0),
                f.id.to_string(),
            )
        })
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.2.cmp(&b.2)));
    let mut out = vec![format!(
        "{} nodes{}",
        rows.len(),
        if r.second.sampling.approximate {
            format!(
                "; betweenness and closeness estimated from {} sampled sources",
                r.second.sampling.pivots
            )
        } else {
            String::new()
        }
    )];
    out.push(
        columns
            .iter()
            .map(|c| format!("{c:>14}"))
            .collect::<String>()
            + "  node",
    );
    for (vals, _, id) in rows.into_iter().take(top) {
        let cells: String = vals
            .iter()
            .map(|v| {
                if v.fract() == 0.0 {
                    format!("{v:>14.0}")
                } else {
                    format!("{v:>14.5}")
                }
            })
            .collect();
        out.push(format!("{cells}  {id}"));
    }
    out
}

/// Runs `aldente cycles`.
///
/// Example: `aldente cycles --level namespace` lists require cycles.
pub fn cycles(args: &CyclesArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "cycles";
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let kind = args.kind.unwrap_or(if args.level == Level::Symbol {
        Kind::Invocation
    } else {
        Kind::Structure
    });
    let s = spec(&args.projection, args.level, kind);
    let store = loaded.store();
    let (first, kept) = match cache::measures(&loaded.outcome.snapshot.graph, &s, store.as_ref()) {
        Ok(v) => v,
        Err(e) => return fail(TAG, &usage(e), json),
    };
    let report = CyclesReport {
        projection: s,
        components: first.components,
    };
    let mut lines = vec![format!("{} cycles", report.components.len())];
    for (i, c) in report.components.iter().enumerate() {
        lines.push(String::new());
        lines.push(format!("{}. {} nodes:", i + 1, c.size));
        lines.extend(c.nodes.iter().map(|n| format!("     {n}")));
        lines.push(format!(
            "   to break it, remove{}:",
            if c.cut_minimal {
                ""
            } else {
                " (not minimized)"
            }
        ));
        lines.extend(c.cut.iter().map(|e| {
            format!(
                "     {} -> {}  (weight {:.2}, {} edges)",
                e.from, e.to, e.weight, e.count
            )
        }));
    }
    emit(
        TAG,
        &loaded,
        &report,
        kept_warnings(&loaded, kept),
        lines,
        json,
    )
}

/// Runs `aldente communities`.
///
/// Example: `aldente communities --seed 7`.
pub fn communities(args: &CommunitiesArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "communities";
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let s = spec(&args.projection, args.level, Kind::Invocation);
    let opts = SecondOptions {
        community_seed: args.seed,
        ..SecondOptions::default()
    };
    let store: Box<dyn ResultStore> = loaded.store();
    let (second, kept) =
        match cache::second(&loaded.outcome.snapshot.graph, &s, opts, store.as_ref()) {
            Ok(v) => v,
            Err(e) => return fail(TAG, &usage(e), json),
        };
    let report = CommunitiesReport {
        projection: s,
        seed: args.seed,
        modularity: second.modularity,
        members: second
            .nodes
            .iter()
            .map(|n| Member {
                id: n.id.clone(),
                community: n.community,
            })
            .collect(),
        communities: second.communities,
        split_namespaces: second.split_namespaces,
    };
    let mut lines = vec![format!(
        "{} communities, modularity {:.3} (seed {})",
        report.communities.len(),
        report.modularity,
        report.seed
    )];
    for (i, c) in report
        .communities
        .iter()
        .enumerate()
        .filter(|(_, c)| c.size > 1)
    {
        lines.push(format!(
            "  {i:>3}: {} nodes in {}",
            c.size,
            c.namespaces.join(", ")
        ));
    }
    if !report.split_namespaces.is_empty() {
        lines.push(String::new());
        lines.push("Namespaces split across communities:".into());
        for sp in &report.split_namespaces {
            let parts: Vec<String> = sp
                .communities
                .iter()
                .map(|(c, n)| format!("{n} in {c}"))
                .collect();
            lines.push(format!("  {}: {}", sp.namespace, parts.join(", ")));
        }
    }
    emit(
        TAG,
        &loaded,
        &report,
        kept_warnings(&loaded, kept),
        lines,
        json,
    )
}

/// Runs `aldente deadcode`.
///
/// Example: `aldente deadcode` lists never-reached symbols with locations.
pub fn deadcode(args: &DeadcodeArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "deadcode";
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let store = loaded.store();
    let (report, kept): (DeadCodeReport, _) =
        cache::dead_code(&loaded.outcome.snapshot.graph, store.as_ref());
    let mut lines = vec![format!(
        "{} symbols unreachable from {} entry points, tests and load-time forms (every certainty counted):",
        report.dead.len(),
        report.roots
    )];
    for d in &report.dead {
        let at = d
            .loc
            .as_ref()
            .map(|l| format!("{}:{}", l.file, l.line))
            .unwrap_or_default();
        lines.push(format!("  {at}  {}", d.id));
    }
    emit(
        TAG,
        &loaded,
        &report,
        kept_warnings(&loaded, kept),
        lines,
        json,
    )
}
