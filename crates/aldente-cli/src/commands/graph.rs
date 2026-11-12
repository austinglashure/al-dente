//! `aldente graph [TARGET]`: the graph at a level, or a neighborhood, as
//! text, JSON, DOT, Mermaid or GraphML.

use aldente_graph::query::{Direction, neighborhood};
use aldente_graph::rollup::{MAX_EDGES, MAX_NODES};
use aldente_model::{AggregationLevel, Envelope, ErrorCode, ExitCode};
use clap::{Args, ValueEnum};

use crate::commands::loaded::load;
use crate::commands::query::{MinCertainty, node_id};
use crate::commands::target::{Failure, TargetArgs};
use crate::formats;
use crate::output::{fail, print_json, print_lines};

/// Output formats for `graph`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Text,
    Json,
    Dot,
    Mermaid,
    Graphml,
}

/// Aggregation levels on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Level {
    Unit,
    Namespace,
    Symbol,
}

/// Walk directions on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Dir {
    Out,
    In,
    Both,
}

/// Arguments for `aldente graph`.
#[derive(Debug, Args)]
pub struct GraphArgs {
    #[command(flatten)]
    target: TargetArgs,
    #[arg(long, value_enum, default_value = "namespace")]
    level: Level,
    /// Show only the neighborhood of this node (full ID or unambiguous suffix).
    #[arg(long)]
    focus: Option<String>,
    /// Steps from the focus node.
    #[arg(long, default_value_t = 1)]
    depth: usize,
    #[arg(long, value_enum, default_value = "both")]
    direction: Dir,
    #[arg(long, value_enum, default_value = "text")]
    format: Format,
    /// Leave out edges weaker than this (edges without a certainty stay).
    #[arg(long, value_enum)]
    min_certainty: Option<MinCertainty>,
    /// `--limit 0` allows a graph larger than the payload cap.
    #[arg(long)]
    limit: Option<usize>,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "graph";

/// Prints the graph. A whole-repository graph above the payload cap (2,000
/// nodes or 10,000 edges) is refused unless `--limit 0`.
///
/// Example: `aldente graph --level namespace --format dot | dot -Tsvg > g.svg`.
pub fn run(args: &GraphArgs, json: bool) -> anyhow::Result<ExitCode> {
    let format = if json { Format::Json } else { args.format };
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, format == Format::Json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let level = match args.level {
        Level::Unit => AggregationLevel::Unit,
        Level::Namespace => AggregationLevel::Namespace,
        Level::Symbol => AggregationLevel::Symbol,
    };
    let filter = aldente_graph::query::Filter {
        min_certainty: args.min_certainty.map(Into::into),
        ..Default::default()
    };
    let mut payload = aldente_graph::query::filter(graph, graph.payload(level), &filter);
    if let Some(focus) = &args.focus {
        let id = match node_id(graph, focus) {
            Ok(id) => id,
            Err(f) => return fail(TAG, &f, format == Format::Json),
        };
        if !payload.nodes.iter().any(|n| n.id == id) {
            let f = Failure::new(
                ErrorCode::Usage,
                format!("{id} is not a node at this level; try --level symbol"),
            );
            return fail(TAG, &f, format == Format::Json);
        }
        let dir = match args.direction {
            Dir::Out => Direction::Out,
            Dir::In => Direction::In,
            Dir::Both => Direction::Both,
        };
        payload = neighborhood(&payload, &id, args.depth, dir);
    } else if args.limit != Some(0)
        && (payload.nodes.len() > MAX_NODES || payload.edges.len() > MAX_EDGES)
    {
        let f = Failure::new(
            ErrorCode::Usage,
            format!(
                "{} nodes and {} edges exceed the payload cap ({MAX_NODES}, {MAX_EDGES}); use --level namespace, --focus, or --limit 0",
                payload.nodes.len(),
                payload.edges.len()
            ),
        );
        return fail(TAG, &f, format == Format::Json);
    }
    match format {
        Format::Json => print_json(&Envelope::success(TAG, Some(loaded.info()), &payload))?,
        Format::Text => print_lines(formats::text(&payload).iter().map(String::as_str))?,
        Format::Dot => print_raw(&formats::dot(&payload))?,
        Format::Mermaid => print_raw(&formats::mermaid(&payload))?,
        Format::Graphml => print_raw(&formats::graphml(&payload))?,
    }
    Ok(ExitCode::Success)
}

/// Writes already formatted output.
///
/// Example: `print_raw("digraph {}\n")`.
fn print_raw(s: &str) -> anyhow::Result<()> {
    use std::io::Write;
    std::io::stdout().lock().write_all(s.as_bytes())?;
    Ok(())
}
