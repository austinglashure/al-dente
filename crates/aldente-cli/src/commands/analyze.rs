//! `aldente analyze [TARGET] [--no-cache]`: builds or reuses the snapshot and
//! prints counts.

use std::collections::BTreeMap;

use aldente_model::analyze::AnalyzeSummary;
use aldente_model::{Envelope, ExitCode};
use clap::Args;

use crate::commands::loaded::{Loaded, load};
use crate::commands::target::TargetArgs;
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente analyze`.
#[derive(Debug, Args)]
pub struct AnalyzeArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Analyze even if a snapshot for this exact content exists.
    #[arg(long)]
    no_cache: bool,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "analyze";

/// Builds or loads the snapshot and prints its summary.
///
/// Example: `aldente analyze fixtures-copy/clj-mini` prints `units 1, files 13`
/// and node and edge counts.
pub fn run(args: &AnalyzeArgs, json: bool) -> anyhow::Result<ExitCode> {
    let loaded = match load(&args.target, args.no_cache) {
        Ok(loaded) => loaded,
        Err(failure) => return fail(TAG, &failure, json),
    };
    let summary = summarize(&loaded);
    if json {
        let mut env = Envelope::success(TAG, Some(loaded.info()), &summary);
        env.warnings = loaded.warnings();
        print_json(&env)?;
    } else {
        print_lines(render(&summary).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// Counts nodes, edges and blind spots by kind.
///
/// Example: clj-mini gives `units: 1, files: 13, nodes: {namespace: 13, …}`.
fn summarize(loaded: &Loaded) -> AnalyzeSummary {
    let snap = &loaded.outcome.snapshot;
    let mut nodes = BTreeMap::new();
    for n in snap.graph.nodes.values() {
        *nodes.entry(n.kind).or_default() += 1;
    }
    let mut edges = BTreeMap::new();
    for e in &snap.graph.edges {
        *edges.entry(e.kind).or_default() += 1;
    }
    let mut blindspots = BTreeMap::new();
    for b in &snap.graph.blindspots {
        *blindspots.entry(b.category).or_default() += 1;
    }
    let mut diagnostics = snap.graph.diagnostics.clone();
    diagnostics.extend(loaded.target.diagnostics.iter().cloned());
    diagnostics.sort();
    diagnostics.dedup();
    AnalyzeSummary {
        units: snap.units.len(),
        files: snap.units.iter().map(|u| u.files).sum(),
        nodes,
        edges,
        blindspots,
        diagnostics,
    }
}

/// The text form.
///
/// Example: `["units 1, files 13", "nodes: external 40, impl 12, …", …]`.
fn render(s: &AnalyzeSummary) -> Vec<String> {
    let counts = |m: Vec<(String, usize)>| {
        if m.is_empty() {
            "none".to_owned()
        } else {
            m.into_iter()
                .map(|(k, v)| format!("{k} {v}"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut lines = vec![
        format!("units {}, files {}", s.units, s.files),
        format!(
            "nodes: {}",
            counts(s.nodes.iter().map(|(k, v)| (token(k), *v)).collect())
        ),
        format!(
            "edges: {}",
            counts(s.edges.iter().map(|(k, v)| (token(k), *v)).collect())
        ),
        format!(
            "blind spots: {}",
            counts(s.blindspots.iter().map(|(k, v)| (token(k), *v)).collect())
        ),
        format!("diagnostics: {}", s.diagnostics.len()),
    ];
    for d in &s.diagnostics {
        let path = d.path.as_ref().map_or("-", |p| p.as_str());
        lines.push(printable(&format!(
            "  {path}  {}  {}",
            token(&d.code),
            d.message
        )));
    }
    lines
}
