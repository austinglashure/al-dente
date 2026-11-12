//! `aldente blindspots [TARGET]`: what the analyzer could not see through.

use aldente_model::query::BlindspotReport;
use aldente_model::{BlindSpotCategory, Envelope, ExitCode};
use clap::Args;

use crate::commands::loaded::load;
use crate::commands::target::TargetArgs;
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente blindspots`.
#[derive(Debug, Args)]
pub struct BlindspotsArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Only these categories (repeatable), e.g. `unresolved-symbol`.
    #[arg(long, value_parser = parse_category)]
    category: Vec<BlindSpotCategory>,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "blindspots";

/// Parses a category as written in JSON.
///
/// Example: `parse_category("macro-generated")` is `Ok(MacroGenerated)`.
fn parse_category(s: &str) -> Result<BlindSpotCategory, String> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|_| format!("unknown category {s:?}"))
}

/// Prints blind spots and diagnostics (skipped and unreadable files).
///
/// Example: on clj-mini prints one `macro-generated` blind spot for `with-timing`.
pub fn run(args: &BlindspotsArgs, json: bool) -> anyhow::Result<ExitCode> {
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let blindspots = graph
        .blindspots
        .iter()
        .filter(|b| args.category.is_empty() || args.category.contains(&b.category))
        .cloned()
        .collect();
    let mut diagnostics = graph.diagnostics.clone();
    diagnostics.extend(loaded.target.diagnostics.iter().cloned());
    diagnostics.sort();
    diagnostics.dedup();
    let report = BlindspotReport {
        blindspots,
        diagnostics,
    };
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &report))?;
    } else {
        print_lines(render(&report).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// The text form.
///
/// Example: `["blind spots (1):", "  macro-generated  src/shop/util.clj:17:1  calls made by …", "diagnostics (0):"]`.
fn render(r: &BlindspotReport) -> Vec<String> {
    let mut lines = vec![format!("blind spots ({}):", r.blindspots.len())];
    for b in &r.blindspots {
        let site = b
            .site
            .as_ref()
            .map(|s| format!("{}:{}:{}", s.file, s.line, s.col))
            .unwrap_or_else(|| "-".into());
        lines.push(printable(&format!(
            "  {}  {site}  {}",
            token(&b.category),
            b.message
        )));
    }
    lines.push(format!("diagnostics ({}):", r.diagnostics.len()));
    for d in &r.diagnostics {
        let path = d.path.as_ref().map_or("-", |p| p.as_str());
        lines.push(printable(&format!(
            "  {}  {path}  {}",
            token(&d.code),
            d.message
        )));
    }
    lines
}
