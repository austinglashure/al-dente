//! `aldente hotspots`: ranked findings (god objects, must-be-bulletproof
//! code, bottlenecks, cycles, hidden coupling, risky untested code,
//! misplaced code, dead code), each with its evidence and a reason.

use std::path::PathBuf;

use aldente_analysis::churn::{ChurnOptions, churn};
use aldente_analysis::hotspots::from_graph;
use aldente_model::hotspots::HotspotsReport;
use aldente_model::{Envelope, ErrorCode, ExitCode, RepoPath, Warning, WarningCode};
use aldente_rules::hotspots::HotspotRules;
use clap::Args;

use crate::commands::churn::{HistoryRequest, read_history};
use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente hotspots`.
#[derive(Debug, Args)]
pub struct HotspotsArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Findings shown in text output (JSON is always complete).
    #[arg(long, default_value_t = 20)]
    top: usize,
    /// Thresholds from this file instead of the built-in
    /// `rules/hotspots.toml` (same format).
    #[arg(long, value_name = "FILE")]
    rules: Option<PathBuf>,
    /// Read git history: weight risk by churn and report hidden coupling.
    #[arg(long)]
    with_churn: bool,
}

/// Runs `aldente hotspots`.
///
/// Example: `aldente hotspots --top 10` prints the ten strongest findings.
pub fn run(args: &HotspotsArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "hotspots";
    let (rules, source) = match load_rules(args.rules.as_deref()) {
        Ok(r) => r,
        Err(f) => return fail(TAG, &f, json),
    };
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let mut warnings = loaded.warnings();
    let churn_report = if args.with_churn {
        let (hist, w) = match read_history(
            &HistoryRequest::default(),
            &loaded.target.root,
            loaded.target.git,
            &loaded.target.origin,
        ) {
            Ok(v) => v,
            Err(f) => return fail(TAG, &f, json),
        };
        warnings.extend(w);
        let root = loaded.target.root.clone();
        let exists = move |p: &str| {
            RepoPath::parse(p).is_ok()
                && std::fs::symlink_metadata(root.join(p)).is_ok_and(|m| m.is_file())
        };
        Some(churn(graph, &hist, &exists, &ChurnOptions::default()))
    } else {
        None
    };
    let store = loaded.store();
    let (report, kept) = from_graph(graph, &rules, source, churn_report.as_ref(), store.as_ref());
    warnings.extend(kept.into_iter().map(|m| Warning {
        code: WarningCode::NotCached,
        message: format!("a measure could not be kept in the snapshot: {m}"),
    }));
    if json {
        let mut env = Envelope::success(TAG, Some(loaded.info()), &report);
        env.warnings = warnings;
        print_json(&env)?;
    } else {
        for w in &warnings {
            eprintln!("aldente: warning: {}", printable(&w.message));
        }
        print_lines(render(&report, args.top).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// The rules and where they came from (`builtin` or `file`).
///
/// Example: `load_rules(None)` gives the built-in rules.
fn load_rules(path: Option<&std::path::Path>) -> Result<(HotspotRules, &'static str), Failure> {
    match path {
        None => HotspotRules::builtin()
            .map(|r| (r, "builtin"))
            .map_err(|e| Failure::new(ErrorCode::Internal, e.to_string())),
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| {
                Failure::new(
                    ErrorCode::Usage,
                    format!("cannot read {}: {e}", p.display()),
                )
            })?;
            HotspotRules::parse(&text)
                .map(|r| (r, "file"))
                .map_err(|e| Failure::new(ErrorCode::Usage, format!("{}: {e}", p.display())))
        }
    }
}

/// The text report: one block per finding, strongest first.
///
/// Example: `1. must-be-bulletproof  shop.core/checkout  (0.70)` then the reason.
fn render(r: &HotspotsReport, top: usize) -> Vec<String> {
    let mut out = vec![format!(
        "{} findings{}{}{}",
        r.findings.len(),
        if r.excluded > 0 {
            format!(
                " ({} more in example, benchmark or dev code left out; see [exclude] in the rules)",
                r.excluded
            )
        } else {
            String::new()
        },
        if r.churn { ", with churn" } else { "" },
        if r.rules == "file" {
            ", thresholds from --rules"
        } else {
            ""
        }
    )];
    for (i, f) in r.findings.iter().take(top).enumerate() {
        let evidence: Vec<String> = f
            .evidence
            .iter()
            .map(|(k, v)| format!("{k} {}", trim(*v)))
            .collect();
        out.push(String::new());
        out.push(format!(
            "{:>3}. {}  {}  ({:.2})",
            i + 1,
            token(&f.kind),
            f.subject,
            f.score
        ));
        out.push(format!("     {}", f.reason));
        if !evidence.is_empty() {
            out.push(format!("     evidence: {}", evidence.join(", ")));
        }
    }
    if r.findings.len() > top {
        out.push(String::new());
        out.push(format!(
            "{} more (--top {})",
            r.findings.len() - top,
            r.findings.len()
        ));
    }
    out.iter().map(|l| printable(l)).collect()
}

/// A number without needless decimals.
///
/// Example: `trim(17.0)` is `"17"`; `trim(0.123456)` is `"0.123"`.
fn trim(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e12 {
        format!("{v:.0}")
    } else {
        format!("{v:.3}")
    }
}
