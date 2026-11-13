//! `aldente coverage [--import <report>]`: the static coverage estimate per
//! symbol, namespace and unit, merged with a measured LCOV or Codecov JSON
//! report when one is given. Reading a report runs nothing.

use std::collections::BTreeSet;
use std::path::PathBuf;

use aldente_analysis::coverage::{DEFAULT_DEPTH, coverage, parse_report};
use aldente_model::coverage::{CoverageReport, CoverageStatus, GroupCoverage};
use aldente_model::{Envelope, ErrorCode, ExitCode};
use clap::Args;

use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable, token};

/// Largest report read, in bytes.
const MAX_REPORT: u64 = 256 * 1024 * 1024;

/// Arguments for `aldente coverage`.
#[derive(Debug, Args)]
pub struct CoverageArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// A measured report to merge: LCOV (Cloverage `--lcov`) or Codecov
    /// JSON (Cloverage `--codecov`).
    #[arg(long, value_name = "REPORT")]
    import: Option<PathBuf>,
    /// Hops from a test within which reachable code counts as `indirect`.
    #[arg(long, default_value_t = DEFAULT_DEPTH)]
    depth: u32,
    /// Rows per table in text output (JSON is always complete).
    #[arg(long, default_value_t = 20)]
    top: usize,
}

/// Runs `aldente coverage`.
///
/// Example: `aldente coverage --import target/coverage/lcov.info`.
pub fn run(args: &CoverageArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "coverage";
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let measured = match &args.import {
        None => None,
        Some(path) => match read_report(path, graph) {
            Ok(v) => Some(v),
            Err(f) => return fail(TAG, &f, json),
        },
    };
    let report = coverage(
        graph,
        args.depth,
        measured.as_ref().map(|(h, i)| (h, i.clone())),
    );
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &report))?;
    } else {
        print_lines(render(&report, args.top).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// Reads and parses a report against the snapshot's files.
///
/// Example: a missing file is a usage error naming it.
fn read_report(
    path: &std::path::Path,
    graph: &aldente_graph::Graph,
) -> Result<
    (
        aldente_analysis::coverage::LineHits,
        aldente_model::coverage::ImportInfo,
    ),
    Failure,
> {
    let usage = |m: String| Failure::new(ErrorCode::Usage, m);
    let meta = std::fs::metadata(path)
        .map_err(|e| usage(format!("cannot read {}: {e}", path.display())))?;
    if meta.len() > MAX_REPORT {
        return Err(usage(format!("{} is larger than 256 MB", path.display())));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| usage(format!("cannot read {}: {e}", path.display())))?;
    let files: BTreeSet<String> = graph
        .nodes
        .values()
        .filter_map(|n| n.loc.as_ref().map(|l| l.file.as_str().to_owned()))
        .collect();
    parse_report(&text, &files).map_err(|e| usage(format!("{}: {e}", path.display())))
}

/// The text report: totals, namespaces least covered first, untested
/// symbols, and disagreements with the measured report.
///
/// Example: a line `  0.25 direct  0.00 indirect …  shop.pricing`.
fn render(r: &CoverageReport, top: usize) -> Vec<String> {
    let count = |s: CoverageStatus| r.symbols.iter().filter(|x| x.status == s).count();
    let mut out = vec![format!(
        "Estimated coverage (static, depth {}, {} tests): {} direct, {} indirect, {} weak, {} untested",
        r.depth,
        r.tests,
        count(CoverageStatus::Direct),
        count(CoverageStatus::Indirect),
        count(CoverageStatus::Weak),
        count(CoverageStatus::Untested)
    )];
    if let Some(i) = &r.imported {
        out.push(format!(
            "Imported {} report: {} of {} files matched",
            i.format, i.matched, i.files
        ));
        for u in i.unmatched.iter().take(top) {
            out.push(format!("  unmatched: {u}"));
        }
    }
    out.push(String::new());
    out.push("Namespaces, least covered first (shares weighted by lines):".into());
    let mut ns: Vec<&GroupCoverage> = r.namespaces.iter().collect();
    ns.sort_by(|a, b| {
        b.weighted
            .untested
            .total_cmp(&a.weighted.untested)
            .then(a.id.cmp(&b.id))
    });
    for g in ns.iter().take(top) {
        let measured = g
            .measured
            .map(|m| format!("  measured {:.0}%", m * 100.0))
            .unwrap_or_default();
        out.push(format!(
            "  {:>4.0}% direct {:>4.0}% indirect {:>4.0}% weak {:>4.0}% untested  {}{measured}",
            g.weighted.direct * 100.0,
            g.weighted.indirect * 100.0,
            g.weighted.weak * 100.0,
            g.weighted.untested * 100.0,
            g.id
        ));
    }
    if !r.disagreements.is_empty() {
        out.push(String::new());
        out.push("Estimate and measurement disagree:".into());
        for d in r.disagreements.iter().take(top) {
            out.push(format!(
                "  {} estimated {}, measured {:.0}%: {}",
                d.id,
                token(&d.estimated),
                d.measured * 100.0,
                d.kind
            ));
        }
    }
    out.iter().map(|l| printable(l)).collect()
}
