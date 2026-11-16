//! `aldente find <pattern> [TARGET]`: searches symbols, impls and namespaces.

use aldente_graph::query::{Pattern, SearchFilter, search_filtered};
use aldente_model::query::FindResult;
use aldente_model::{Envelope, ErrorCode, ExitCode, NodeKind, SymbolFlag, SymbolSubkind};
use clap::Args;

use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente find`.
#[derive(Debug, Args)]
pub struct FindArgs {
    /// Substring of the name or ID (case-insensitive); `""` matches all.
    /// With `--regex`, a regular expression (unanchored, case-sensitive
    /// unless it starts with `(?i)`).
    pattern: String,
    /// Treat the pattern as a regular expression.
    #[arg(long)]
    regex: bool,
    #[command(flatten)]
    target: TargetArgs,
    /// Only these node kinds or symbol subkinds (repeatable), e.g.
    /// `namespace`, `test`, `fn`. Default: symbol, impl, namespace.
    #[arg(long, value_parser = parse_kind)]
    kind: Vec<Kind>,
    /// Only nodes with all these flags (repeatable), e.g. `entry-point`, `test`.
    #[arg(long, value_parser = parse_flag)]
    flag: Vec<SymbolFlag>,
    /// At most this many results; 0 for all.
    #[arg(long, default_value_t = 50)]
    limit: usize,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "find";

/// A `--kind` value: a node kind or a symbol subkind (their names differ).
#[derive(Debug, Clone, Copy)]
enum Kind {
    Node(NodeKind),
    Sub(SymbolSubkind),
}

/// Parses a node kind or symbol subkind as written in JSON.
///
/// Example: `parse_kind("namespace")` is `Kind::Node(Namespace)`;
/// `parse_kind("test")` is `Kind::Sub(Test)`.
fn parse_kind(s: &str) -> Result<Kind, String> {
    let v = serde_json::Value::String(s.to_owned());
    serde_json::from_value(v.clone())
        .map(Kind::Node)
        .or_else(|_| serde_json::from_value(v).map(Kind::Sub))
        .map_err(|_| format!("unknown kind {s:?}"))
}

/// Parses a flag as written in JSON.
///
/// Example: `parse_flag("entry-point")` is `Ok(SymbolFlag::EntryPoint)`.
fn parse_flag(s: &str) -> Result<SymbolFlag, String> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|_| format!("unknown flag {s:?}"))
}

/// The search filter the arguments ask for.
///
/// Example: `--kind test --flag entry-point` gives subkinds `[test]`, flags `[entry-point]`.
fn filter(args: &FindArgs) -> SearchFilter {
    let mut f = SearchFilter {
        flags: args.flag.clone(),
        ..SearchFilter::default()
    };
    for k in &args.kind {
        match k {
            Kind::Node(n) => f.kinds.push(*n),
            Kind::Sub(s) => f.subkinds.push(*s),
        }
    }
    f
}

/// The pattern the arguments ask for, or a usage failure for a bad regex.
///
/// Example: `find --regex "(" ` gives a usage failure; `find charge` gives
/// `Pattern::Substring("charge")`.
fn pattern(args: &FindArgs) -> Result<Pattern, Failure> {
    if !args.regex {
        return Ok(Pattern::substring(&args.pattern));
    }
    Pattern::regex(&args.pattern)
        .map_err(|e| Failure::new(ErrorCode::Usage, format!("invalid --regex pattern: {e}")))
}

/// Prints matching nodes.
///
/// Example: `aldente find charge` lists `clj:.//shop.payment/charge` and its impls.
pub fn run(args: &FindArgs, json: bool) -> anyhow::Result<ExitCode> {
    let pattern = match pattern(args) {
        Ok(p) => p,
        Err(f) => return fail(TAG, &f, json),
    };
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let (matches, truncated) = search_filtered(
        &loaded.outcome.snapshot.graph,
        &pattern,
        &filter(args),
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
