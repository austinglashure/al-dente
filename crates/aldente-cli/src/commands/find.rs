//! `aldente find <pattern> [TARGET]`: searches symbols, impls and namespaces.

use aldente_graph::query::search;
use aldente_model::query::FindResult;
use aldente_model::{Envelope, ExitCode, NodeKind};
use clap::Args;

use crate::commands::loaded::load;
use crate::commands::target::TargetArgs;
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente find`.
#[derive(Debug, Args)]
pub struct FindArgs {
    /// Substring of the name or ID (case-insensitive).
    pattern: String,
    #[command(flatten)]
    target: TargetArgs,
    /// Only these node kinds (repeatable). Default: symbol, impl, namespace.
    #[arg(long, value_parser = parse_kind)]
    kind: Vec<NodeKind>,
    /// At most this many results; 0 for all.
    #[arg(long, default_value_t = 50)]
    limit: usize,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "find";

/// Parses a node kind as written in JSON.
///
/// Example: `parse_kind("namespace")` is `Ok(NodeKind::Namespace)`.
fn parse_kind(s: &str) -> Result<NodeKind, String> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|_| format!("unknown kind {s:?}"))
}

/// Prints matching nodes.
///
/// Example: `aldente find charge` lists `clj:.//shop.payment/charge` and its impls.
pub fn run(args: &FindArgs, json: bool) -> anyhow::Result<ExitCode> {
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let (matches, truncated) = search(
        &loaded.outcome.snapshot.graph,
        &args.pattern,
        &args.kind,
        args.limit,
    );
    let result = FindResult { matches, truncated };
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &result))?;
    } else {
        let mut lines: Vec<String> = result
            .matches
            .iter()
            .map(|n| {
                let kind = match &n.subkind {
                    Some(sub) => format!("{}/{}", token(&n.kind), token(sub)),
                    None => token(&n.kind),
                };
                let at = n
                    .loc
                    .as_ref()
                    .map(|l| format!("  {}:{}:{}", l.file, l.line, l.col))
                    .unwrap_or_default();
                printable(&format!("{}  {kind}{at}", n.id))
            })
            .collect();
        if result.truncated {
            lines.push(format!("(more than {}; use --limit 0 for all)", args.limit));
        }
        print_lines(lines.iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}
