//! `aldente trace normalize <file|-> [TARGET]`: a raw stack trace as
//! canonical frames, each mapped to a node of the target's graph.

use std::io::Read;
use std::path::PathBuf;

use aldente_analysis::trace::normalize;
use aldente_model::trace::{Frame, MatchLevel, Trace, TraceReport};
use aldente_model::{Envelope, ErrorCode, ExitCode};
use clap::{Args, Subcommand};

use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable, token};

/// Most bytes read from a trace file or stdin.
const MAX_INPUT: u64 = 32 * 1024 * 1024;

/// Arguments for `aldente trace`.
#[derive(Debug, Args)]
pub struct TraceArgs {
    #[command(subcommand)]
    action: TraceAction,
}

#[derive(Debug, Subcommand)]
enum TraceAction {
    /// Convert a raw trace to canonical frames mapped to nodes.
    Normalize(NormalizeArgs),
}

/// Arguments for `aldente trace normalize`.
#[derive(Debug, Args)]
pub struct NormalizeArgs {
    /// A file holding the trace (a log is fine), or `-` for stdin.
    input: String,
    #[command(flatten)]
    target: TargetArgs,
    /// Also list noise frames one by one instead of folding them.
    #[arg(long)]
    all: bool,
}

const TAG: &str = "trace";

/// Runs the trace action.
///
/// Example: `aldente trace normalize crash.log` prints each trace with its
/// repository frames mapped.
pub fn run(args: &TraceArgs, json: bool) -> anyhow::Result<ExitCode> {
    let TraceAction::Normalize(args) = &args.action;
    let text = match read_input(&args.input) {
        Ok(t) => t,
        Err(f) => return fail(TAG, &f, json),
    };
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let report = match normalize(&text, &loaded.outcome.snapshot.graph) {
        Ok(r) => r,
        Err(e) => return fail(TAG, &Failure::new(ErrorCode::Usage, e.to_string()), json),
    };
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &report))?;
    } else {
        print_lines(render(&report, args.all).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// Reads the trace from a file or stdin, at most [`MAX_INPUT`] bytes.
///
/// Example: `read_input("-")` reads stdin.
pub fn read_input(input: &str) -> Result<String, Failure> {
    let mut bytes = Vec::new();
    let result = if input == "-" {
        std::io::stdin().take(MAX_INPUT).read_to_end(&mut bytes)
    } else {
        std::fs::File::open(PathBuf::from(input))
            .and_then(|f| f.take(MAX_INPUT).read_to_end(&mut bytes))
    };
    result.map_err(|e| Failure::new(ErrorCode::Usage, format!("cannot read {input}: {e}")))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The text form: each trace's exception, its frames (noise folded unless
/// `all`), then its causes, indented.
///
/// Example lines: `java.lang.ArithmeticException: Divide by zero`,
/// `  #3  shop.pricing/half-off  pricing.clj:10  -> clj:.//shop.pricing/half-off exact`,
/// `  … 4 noise frames`.
pub fn render(report: &TraceReport, all: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for (i, t) in report.traces.iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }
        trace_lines(t, "", all, &mut lines);
    }
    lines.iter().map(|l| printable(l)).collect()
}

/// One trace and its causes.
///
/// Example: see [`render`].
fn trace_lines(t: &Trace, indent: &str, all: bool, lines: &mut Vec<String>) {
    let ex = &t.exception;
    let head = match (&ex.class, &ex.message) {
        (Some(c), Some(m)) => format!("{c}: {m}"),
        (Some(c), None) => c.clone(),
        (None, Some(m)) => m.clone(),
        (None, None) => "(no exception line)".into(),
    };
    lines.push(format!("{indent}{head}"));
    let mut folded = 0;
    for f in &t.frames {
        if f.noise && !all {
            folded += 1;
            continue;
        }
        if folded > 0 {
            lines.push(format!("{indent}  … {}", noise_count(folded)));
            folded = 0;
        }
        lines.push(format!("{indent}  {}", frame_line(f)));
    }
    if folded > 0 {
        lines.push(format!("{indent}  … {}", noise_count(folded)));
    }
    if t.omitted > 0 {
        lines.push(format!("{indent}  ... {} more", t.omitted));
    }
    for c in &t.causes {
        lines.push(format!("{indent}caused by:"));
        trace_lines(c, &format!("{indent}  "), all, lines);
    }
}

/// `"1 noise frame"` or `"4 noise frames"`.
///
/// Example: `noise_count(2)` is `"2 noise frames"`.
fn noise_count(n: usize) -> String {
    if n == 1 {
        "1 noise frame".into()
    } else {
        format!("{n} noise frames")
    }
}

/// `#3  ns/name (anon)  file:line  -> node match`.
///
/// Example: `"#3  shop.pricing/half-off  pricing.clj:10  -> clj:.//shop.pricing/half-off exact"`.
fn frame_line(f: &Frame) -> String {
    let who = match (&f.namespace, &f.name) {
        (Some(ns), Some(n)) => format!("{ns}/{n}"),
        (Some(ns), None) => format!("{ns}/?"),
        (None, Some(n)) => n.clone(),
        (None, None) => "?".into(),
    };
    let anon = if f.anonymous { " (anon)" } else { "" };
    let at = match (&f.file, f.line) {
        (Some(file), Some(l)) => format!("  {file}:{l}"),
        (Some(file), None) => format!("  {file}"),
        _ => String::new(),
    };
    let mapped = match (&f.node, f.match_level) {
        (Some(n), level) => format!("  -> {n} {}", token(&level)),
        (None, MatchLevel::None) if f.noise => "  noise".into(),
        (None, _) => String::new(),
    };
    format!("#{}  {who}{anon}{at}{mapped}", f.index)
}
