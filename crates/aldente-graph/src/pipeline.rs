//! The full static pipeline for a local root: walk, discover units, run the
//! adapters that claim each unit, build the graph.

use std::path::Path;

use aldente_acquire::units::{Discovery, discover};
use aldente_acquire::walk::{Limits, walk};
use aldente_adapter::{Adapter, AdapterError, AnalyzeCtx, FactBatch};
use aldente_model::{BlindSpot, BlindSpotCategory, Diagnostic, DiagnosticCode, RepoPath, Site};

use crate::build::{Graph, build};

/// Discovery plus the graph.
#[derive(Debug, Clone)]
pub struct Analyzed {
    pub discovery: Discovery,
    pub graph: Graph,
}

/// Runs the pipeline on `root` (canonical). A missing tool stops it
/// (`AdapterError::ToolMissing`); any other adapter failure is recorded as an
/// `analyzer-failed` diagnostic plus an `unparsed-file` blind spot per file
/// of that unit, and the other units continue.
///
/// Example: `analyze(clj_mini, false, &[&clj])` gives a graph with
/// `clj:.//shop.core/checkout`.
pub fn analyze(
    root: &Path,
    git: bool,
    adapters: &[&dyn Adapter],
) -> Result<Analyzed, AdapterError> {
    let discovery = discover(root, git, walk(root, &Limits::default()));
    let mut batches = Vec::new();
    for unit in &discovery.report.units {
        let files: Vec<RepoPath> = discovery
            .analyzed
            .iter()
            .filter(|f| f.unit == unit.dir)
            .map(|f| f.path.clone())
            .collect();
        if files.is_empty() {
            continue;
        }
        for adapter in adapters.iter().filter(|a| a.claims(unit)) {
            match adapter.analyze(
                unit,
                AnalyzeCtx {
                    root,
                    files: &files,
                },
            ) {
                Ok(batch) => batches.push(batch),
                Err(e @ AdapterError::ToolMissing { .. }) => return Err(e),
                Err(AdapterError::Failed(message)) => {
                    batches.push(failed(&unit.dir, &files, adapter.id(), &message))
                }
            }
        }
    }
    let mut graph = build(&discovery.report.units, batches);
    graph
        .diagnostics
        .extend(discovery.report.diagnostics.iter().cloned());
    graph.diagnostics.sort();
    graph.diagnostics.dedup();
    Ok(Analyzed { discovery, graph })
}

/// The facts for a unit whose adapter failed: a diagnostic and one
/// `unparsed-file` blind spot per file.
///
/// Example: a clj-kondo timeout on a 3-file unit gives 1 diagnostic and 3 blind spots.
fn failed(unit: &RepoPath, files: &[RepoPath], adapter: &str, message: &str) -> FactBatch {
    FactBatch {
        diagnostics: vec![Diagnostic::new(
            Some(unit.clone()),
            DiagnosticCode::AnalyzerFailed,
            format!("{adapter} adapter failed: {message}"),
        )],
        blindspots: files
            .iter()
            .map(|f| BlindSpot {
                category: BlindSpotCategory::UnparsedFile,
                node: None,
                site: Some(Site {
                    file: f.clone(),
                    line: 1,
                    col: 1,
                }),
                message: format!("not analyzed: the {adapter} adapter failed for this unit"),
            })
            .collect(),
        ..FactBatch::default()
    }
}
