//! `aldente crater <symbol>…`: the impact subgraph of one or more seeds, as
//! grouped text, a checklist, JSON, DOT or Mermaid.

use std::collections::BTreeMap;

use aldente_analysis::crater::{CraterIndex, CraterOptions, checklist, crater};
use aldente_graph::Graph;
use aldente_model::crater::{Checklist, ChecklistRow, CraterResult, DiffSeeds, Direction};
use aldente_model::{
    Certainty, EdgeKind, Envelope, ErrorCode, ExitCode, NodeId, Warning, WarningCode,
};
use clap::{Args, ValueEnum};

use crate::commands::crater_diff::diff_seeds;
use crate::commands::loaded::load;
use crate::commands::query::{MinCertainty, node_id};
use crate::commands::target::{Failure, TargetArgs};
use crate::formats;
use crate::output::{fail, print_json, print_lines, printable, token};

/// Output formats for `crater`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
    Dot,
    Mermaid,
}

/// Crater directions on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Dir {
    Forward,
    Reverse,
    Both,
}

/// Edge kinds a crater can follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Kind {
    Calls,
    References,
    Dispatches,
    Registers,
    Tests,
}

/// Arguments for `aldente crater`.
#[derive(Debug, Args)]
pub struct CraterArgs {
    /// Seeds: full IDs or unambiguous suffixes (symbols, impls, namespaces,
    /// libraries). Not needed with `--diff`.
    #[arg(required_unless_present = "diff")]
    symbols: Vec<String>,
    /// The repository: a local directory or a git URL (default: the current directory).
    #[arg(long = "target", value_name = "TARGET", default_value = ".")]
    target: String,
    /// Branch, tag or commit to check out (URL targets only).
    #[arg(long = "ref", value_name = "REV")]
    git_ref: Option<String>,
    /// Print wall time per pipeline stage to stderr.
    #[arg(long)]
    timings: bool,
    /// Default: forward, or reverse with `--diff`.
    #[arg(long, value_enum)]
    direction: Option<Dir>,
    /// Steps from the seeds; 0 means unlimited.
    #[arg(long, default_value_t = 6)]
    depth: u32,
    /// Leave out nodes whose best path is weaker than this.
    #[arg(long, value_enum, default_value = "likely")]
    min_certainty: MinCertainty,
    /// Edge kinds to follow (repeatable; default calls, references, dispatches, registers).
    #[arg(long = "kind", value_enum)]
    kinds: Vec<Kind>,
    /// Include but do not expand nodes matching this wildcard (`*`, `?`) on
    /// their ID or `namespace/name` (repeatable).
    #[arg(long = "stop-at", value_name = "PATTERN")]
    stop_at: Vec<String>,
    /// Expand past externals instead of stopping at them.
    #[arg(long)]
    through_externals: bool,
    /// Do not expand nodes outside the seeds' units.
    #[arg(long)]
    stop_at_units: bool,
    /// Leave tests out.
    #[arg(long)]
    no_tests: bool,
    /// Expand past hubs (the busiest 0.1 percent of nodes).
    #[arg(long)]
    through_hubs: bool,
    /// Most nodes in the result.
    #[arg(long, default_value_t = 10_000)]
    limit: usize,
    /// Print the symbols and impls as a flat list of places to verify.
    #[arg(long)]
    checklist: bool,
    /// Seed from every symbol whose source changed since this revision
    /// (`<rev>` or `<rev>..HEAD`; the working tree is the new side).
    #[arg(long, value_name = "REV")]
    diff: Option<String>,
    #[arg(long, value_enum, default_value = "text")]
    format: Format,
}

/// The envelope `schema` tags.
const TAG: &str = "crater";
const CHECKLIST_TAG: &str = "checklist";

/// Prints the crater.
///
/// Example: `aldente crater shop.pricing/total --direction reverse --checklist`.
pub fn run(args: &CraterArgs, json: bool) -> anyhow::Result<ExitCode> {
    let format = if json { Format::Json } else { args.format };
    let tag = if args.checklist { CHECKLIST_TAG } else { TAG };
    let as_json = format == Format::Json;
    let target = TargetArgs {
        target: args.target.clone(),
        git_ref: args.git_ref.clone(),
        timings: args.timings,
    };
    let loaded = match load(&target, false) {
        Ok(l) => l,
        Err(f) => return fail(tag, &f, as_json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let min: Certainty = args.min_certainty.into();
    let mut seeds = Vec::new();
    for s in &args.symbols {
        match node_id(graph, s) {
            Ok(id) => seeds.push(id),
            Err(f) => return fail(tag, &f, as_json),
        }
    }
    let mut warnings = loaded.warnings();
    let mut diff: Option<DiffSeeds> = None;
    if let Some(spec) = &args.diff {
        match diff_seeds(&loaded, spec, min) {
            Ok((ids, report, w)) => {
                seeds.extend(ids);
                diff = Some(report);
                warnings.extend(w);
            }
            Err(f) => return fail(tag, &f, as_json),
        }
    }
    if args.checklist && matches!(format, Format::Dot | Format::Mermaid) {
        let f = Failure::new(ErrorCode::Usage, "--checklist prints text or JSON");
        return fail(tag, &f, false);
    }
    let opts = options(args);
    let index = CraterIndex::new(graph);
    let mut result = match crater(graph, &index, &seeds, &opts) {
        Ok(r) => r,
        Err(e) => {
            return fail(
                tag,
                &Failure::new(ErrorCode::UnknownSymbol, e.to_string()),
                as_json,
            );
        }
    };
    result.diff = diff;
    if result.truncated {
        warnings.push(Warning {
            code: WarningCode::Truncated,
            message: "the depth limit or the node cap cut the crater short".into(),
        });
    }
    let snapshot = Some(loaded.info());
    if args.checklist {
        let list = Checklist {
            seeds: result.seeds.clone(),
            direction: result.direction,
            depth: result.depth,
            min_certainty: result.min_certainty,
            rows: checklist(graph, &result),
            hidden_below_min: result.hidden_below_min,
            truncated: result.truncated,
            diff: result.diff.clone(),
        };
        if as_json {
            let mut env = Envelope::success(CHECKLIST_TAG, snapshot, &list);
            env.warnings = warnings;
            print_json(&env)?;
        } else {
            print_lines(render_checklist(&list).iter().map(String::as_str))?;
            print_warnings(&warnings);
        }
        return Ok(ExitCode::Success);
    }
    match format {
        Format::Json => {
            let mut env = Envelope::success(TAG, snapshot, &result);
            env.warnings = warnings;
            print_json(&env)?;
        }
        Format::Text => {
            print_lines(render(graph, &result).iter().map(String::as_str))?;
            print_warnings(&warnings);
        }
        Format::Dot => print_raw(&formats::dot(&result.graph))?,
        Format::Mermaid => print_raw(&formats::mermaid(&result.graph))?,
    }
    Ok(ExitCode::Success)
}

/// Engine settings from the arguments.
///
/// Example: `--direction reverse --no-tests` gives a reverse crater without tests.
fn options(args: &CraterArgs) -> CraterOptions {
    let defaults = CraterOptions::default();
    let direction = match (args.direction, &args.diff) {
        (Some(Dir::Forward), _) => Direction::Forward,
        (Some(Dir::Reverse), _) | (None, Some(_)) => Direction::Reverse,
        (Some(Dir::Both), _) => Direction::Both,
        (None, None) => Direction::Forward,
    };
    let kinds = if args.kinds.is_empty() {
        defaults.kinds.clone()
    } else {
        args.kinds
            .iter()
            .map(|k| match k {
                Kind::Calls => EdgeKind::Calls,
                Kind::References => EdgeKind::References,
                Kind::Dispatches => EdgeKind::Dispatches,
                Kind::Registers => EdgeKind::Registers,
                Kind::Tests => EdgeKind::Tests,
            })
            .collect()
    };
    CraterOptions {
        direction,
        depth: args.depth,
        min_certainty: args.min_certainty.into(),
        kinds,
        stop_at: args.stop_at.clone(),
        stop_at_externals: !args.through_externals,
        stop_at_units: args.stop_at_units,
        exclude_tests: args.no_tests,
        through_hubs: args.through_hubs,
        hub_degree: None,
        node_cap: args.limit,
    }
}

/// The header line: direction, seeds, depth, minimum certainty and size.
///
/// Example: `"reverse crater of clj:.//shop.pricing/total   depth 6   min likely   4 nodes"`.
fn header(
    seeds: &[NodeId],
    direction: Direction,
    depth: u32,
    min: Certainty,
    size: String,
    diff: Option<&DiffSeeds>,
) -> String {
    let of = match (diff, seeds) {
        (Some(d), _) => format!(
            "{} changed since {} ({})",
            plural(d.changed.len(), "symbol"),
            d.base,
            &d.base_commit[..d.base_commit.len().min(12)]
        ),
        (None, [one]) => one.to_string(),
        (None, many) => format!(
            "{}: {}",
            plural(many.len(), "seed"),
            many.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let depth = if depth == 0 {
        "unlimited".to_owned()
    } else {
        depth.to_string()
    };
    format!(
        "{} crater of {of}   depth {depth}   min {}   {size}",
        token(&direction),
        token(&min)
    )
}

/// `"1 symbol"` or `"3 symbols"`.
///
/// Example: `plural(2, "node")` is `"2 nodes"`.
fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The text form: nodes grouped by path certainty, then unit, then
/// namespace; then the boundary, hubs, stops, what `possible` would add,
/// and the diff's removed definitions.
///
/// Example lines: `certain (2)`, `  unit:.`, `    shop.core`,
/// `      checkout  src/shop/core.clj:10:1  seed`.
fn render(graph: &Graph, r: &CraterResult) -> Vec<String> {
    let mut lines = vec![header(
        &r.seeds,
        r.direction,
        r.depth,
        r.min_certainty,
        plural(r.reach.len(), "node"),
        r.diff.as_ref(),
    )];
    // certainty (strongest first) -> unit -> namespace -> lines
    type Groups =
        BTreeMap<std::cmp::Reverse<Certainty>, BTreeMap<String, BTreeMap<String, Vec<String>>>>;
    let mut groups: Groups = BTreeMap::new();
    for reach in &r.reach {
        let node = graph.nodes.get(&reach.id);
        let unit = node
            .and_then(|n| n.unit.as_ref())
            .map(ToString::to_string)
            .unwrap_or_else(|| "outside the repository".to_owned());
        let ns = node.and_then(|n| n.namespace.clone()).unwrap_or_default();
        let name = match node {
            Some(n) if n.namespace.is_some() => n.name.clone(),
            _ => reach.id.to_string(),
        };
        let at = node
            .and_then(|n| n.loc.as_ref())
            .map(|l| format!("  {}:{}:{}", l.file, l.line, l.col))
            .unwrap_or_default();
        let how = match &reach.via {
            None => "seed".to_owned(),
            Some(v) => format!("{} via {v}", reach.distance),
        };
        groups
            .entry(std::cmp::Reverse(reach.path_certainty))
            .or_default()
            .entry(unit)
            .or_default()
            .entry(ns)
            .or_default()
            .push(format!("{name}{at}  {how}"));
    }
    for (cert, units) in groups {
        let n: usize = units.values().flat_map(|u| u.values()).map(Vec::len).sum();
        lines.push(format!("{} ({n})", token(&cert.0)));
        for (unit, namespaces) in units {
            lines.push(format!("  {unit}"));
            for (ns, entries) in namespaces {
                let indent = if ns.is_empty() {
                    "    "
                } else {
                    lines.push(format!("    {ns}"));
                    "      "
                };
                lines.extend(entries.into_iter().map(|e| format!("{indent}{e}")));
            }
        }
    }
    for (title, ids) in [
        ("boundary", &r.boundary),
        ("hubs not expanded", &r.hubs),
        ("stopped", &r.stopped),
    ] {
        if !ids.is_empty() {
            lines.push(format!("{title} ({})", ids.len()));
            lines.extend(ids.iter().map(|id| format!("  {id}")));
        }
    }
    lines.extend(footer(
        r.hidden_below_min,
        r.min_certainty,
        r.truncated,
        r.diff.as_ref(),
    ));
    lines.iter().map(|l| printable(l)).collect()
}

/// The lines after the body: what a weaker minimum would add, truncation,
/// and the diff's removed definitions and other changed files.
///
/// Example: `["3 more at possible (--min-certainty possible)"]`.
fn footer(hidden: u64, min: Certainty, truncated: bool, diff: Option<&DiffSeeds>) -> Vec<String> {
    let mut lines = Vec::new();
    if hidden > 0 && min > Certainty::Possible {
        lines.push(format!(
            "{hidden} more at possible (--min-certainty possible)"
        ));
    }
    if truncated {
        lines.push("(truncated: raise --depth or --limit)".into());
    }
    if let Some(d) = diff {
        if !d.removed.is_empty() {
            lines.push(format!("removed since {} ({})", d.base, d.removed.len()));
            for r in &d.removed {
                let callers = match &r.former_callers {
                    Some(c) if c.is_empty() => "no former callers".to_owned(),
                    Some(c) => format!(
                        "former callers: {}",
                        c.iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None => "former callers unknown".to_owned(),
                };
                lines.push(format!(
                    "  {}/{}  {}:{}  {callers}",
                    r.namespace, r.name, r.file, r.line
                ));
            }
        }
        if !d.other_files.is_empty() {
            lines.push(format!(
                "changed files with no analyzed code ({})",
                d.other_files.len()
            ));
            lines.extend(d.other_files.iter().map(|f| format!("  {f}")));
        }
    }
    lines
}

/// The checklist's text form: one location per line, grouped by certainty.
///
/// Example lines: `certain`, `  src/shop/core.clj:10:1  shop.core/checkout  entry-point`.
fn render_checklist(c: &Checklist) -> Vec<String> {
    let mut lines = vec![header(
        &c.seeds,
        c.direction,
        c.depth,
        c.min_certainty,
        plural(c.rows.len(), "symbol"),
        c.diff.as_ref(),
    )];
    let loc = |row: &ChecklistRow| {
        row.loc
            .as_ref()
            .map(|l| format!("{}:{}:{}", l.file, l.line, l.col))
            .unwrap_or_else(|| "-".into())
    };
    let width = c
        .rows
        .iter()
        .map(|r| loc(r).chars().count())
        .max()
        .unwrap_or(0);
    let mut current: Option<Certainty> = None;
    for row in &c.rows {
        if current != Some(row.path_certainty) {
            current = Some(row.path_certainty);
            lines.push(token(&row.path_certainty));
        }
        let at = loc(row);
        let pad = " ".repeat(width - at.chars().count());
        let mut line = format!("  {at}{pad}  {}", row.name);
        if let Some(cov) = &row.coverage {
            line.push_str(&format!("  {}", token(cov)));
        }
        if row.entry_point {
            line.push_str("  entry-point");
        }
        lines.push(line);
    }
    lines.extend(footer(
        c.hidden_below_min,
        c.min_certainty,
        c.truncated,
        c.diff.as_ref(),
    ));
    lines.iter().map(|l| printable(l)).collect()
}

/// Prints warnings to stderr for the text formats.
///
/// Example: `aldente: warning: the depth limit … cut the crater short`.
fn print_warnings(warnings: &[Warning]) {
    for w in warnings {
        eprintln!("aldente: warning: {}", printable(&w.message));
    }
}

/// Writes already formatted output.
///
/// Example: `print_raw("digraph {}\n")`.
fn print_raw(s: &str) -> anyhow::Result<()> {
    use std::io::Write;
    std::io::stdout().lock().write_all(s.as_bytes())?;
    Ok(())
}
