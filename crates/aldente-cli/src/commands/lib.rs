//! `aldente lib ls` and `aldente lib usage <library>`: libraries in use and
//! every place the repository invokes one.

use aldente_analysis::libraries::{LibraryLookup, find_library, summaries, usage};
use aldente_model::libraries::{
    Attribution, LibrariesReport, LibraryUsage, UsageGroup, UsageGrouping, UsageRow,
};
use aldente_model::{Envelope, ErrorCode, ExitCode};
use clap::{Args, Subcommand, ValueEnum};

use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente lib`.
#[derive(Debug, Args)]
pub struct LibArgs {
    #[command(subcommand)]
    action: LibAction,
}

#[derive(Debug, Subcommand)]
enum LibAction {
    /// Libraries in use, with usage counts and declared versions.
    Ls(LsArgs),
    /// Every place the repository invokes or requires a library.
    Usage(UsageArgs),
}

/// Arguments for `aldente lib ls`.
#[derive(Debug, Args)]
pub struct LsArgs {
    #[command(flatten)]
    target: TargetArgs,
}

/// Grouping on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum By {
    Namespace,
    Symbol,
}

/// Arguments for `aldente lib usage`.
#[derive(Debug, Args)]
pub struct UsageArgs {
    /// Full ID (`lib:npm:react`), coordinate (`reagent/reagent`) or artifact (`reagent`).
    library: String,
    #[command(flatten)]
    target: TargetArgs,
    #[arg(long, value_enum, default_value = "namespace")]
    by: By,
}

/// Runs the lib action.
///
/// Example: `aldente lib usage reagent` lists every reagent call and require.
pub fn run(args: &LibArgs, json: bool) -> anyhow::Result<ExitCode> {
    match &args.action {
        LibAction::Ls(a) => ls(a, json),
        LibAction::Usage(a) => usage_cmd(a, json),
    }
}

/// `lib ls`.
///
/// Example: prints one line per library with its counts.
fn ls(args: &LsArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "libs";
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let snap = &loaded.outcome.snapshot;
    let report = summaries(&snap.graph, &snap.units);
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &report))?;
    } else {
        print_lines(render_ls(&report).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// `lib usage`.
///
/// Example: an unknown library exits 3; an ambiguous one lists candidates.
fn usage_cmd(args: &UsageArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "lib-usage";
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let lib = match find_library(graph, &args.library) {
        LibraryLookup::Found(id) => id,
        LibraryLookup::NotFound => {
            let f = Failure::new(
                ErrorCode::UnknownSymbol,
                format!(
                    "no library matches {:?}; see `aldente lib ls`",
                    args.library
                ),
            );
            return fail(TAG, &f, json);
        }
        LibraryLookup::Ambiguous(ids) => {
            let mut f = Failure::new(
                ErrorCode::AmbiguousSymbol,
                format!("{:?} matches {} libraries", args.library, ids.len()),
            );
            f.candidates = Some(ids.iter().map(ToString::to_string).collect());
            return fail(TAG, &f, json);
        }
    };
    let by = match args.by {
        By::Namespace => UsageGrouping::Namespace,
        By::Symbol => UsageGrouping::Symbol,
    };
    let result = usage(graph, &lib, by);
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &result))?;
    } else {
        print_lines(render_usage(&result).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// `"reagent.core likely name-match"`-style attribution text.
///
/// Example: `"ext:clj//react certain direct"`.
fn attribution(a: &Attribution) -> String {
    let cert = a
        .certainty
        .as_ref()
        .map(token)
        .unwrap_or_else(|| "-".into());
    let reason = a.reason.as_ref().map(token).unwrap_or_else(|| "-".into());
    format!("{} {cert} {reason}", a.provides)
}

/// One line per library, then its attribution and declared versions.
///
/// Example: `"lib:npm:react  2 symbols  1 namespace  1 unit"`.
fn render_ls(r: &LibrariesReport) -> Vec<String> {
    let mut lines = Vec::new();
    for l in &r.libraries {
        lines.push(format!(
            "{}  {}  {}  {}",
            l.id,
            plural(l.symbols, "symbol"),
            plural(l.namespaces, "namespace"),
            plural(l.units, "unit")
        ));
        for a in &l.attribution {
            lines.push(format!("  provides {}", attribution(a)));
        }
        for v in &l.versions {
            lines.push(format!(
                "  declared {} by {}",
                v.version.as_deref().unwrap_or("(no version)"),
                v.unit
            ));
        }
    }
    lines.iter().map(|l| printable(l)).collect()
}

/// `"1 unit"` or `"3 units"`.
///
/// Example: `plural(2, "symbol")` is `"2 symbols"`.
fn plural(n: u64, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The usage report: attribution, then each group's rows by site, then the
/// namespaces that only require the library.
///
/// Example lines: `clj:.//lib.views`, `  src/lib/views.cljs:7:18  calls certain  clj:.//lib.views/counter -> ext:clj//reagent.core/atom`.
fn render_usage(u: &LibraryUsage) -> Vec<String> {
    let mut lines = vec![format!("{}   by {}", u.library, token(&u.by))];
    for a in &u.attribution {
        lines.push(format!("provides {}", attribution(a)));
    }
    group_lines(&u.groups, "", &mut lines);
    if !u.require_only.is_empty() {
        lines.push(format!(
            "requires only, never calls ({})",
            u.require_only.len()
        ));
        group_lines(&u.require_only, "  ", &mut lines);
    }
    lines.iter().map(|l| printable(l)).collect()
}

/// Each group's ID, then its rows indented.
///
/// Example: see [`render_usage`].
fn group_lines(groups: &[UsageGroup], indent: &str, lines: &mut Vec<String>) {
    for g in groups {
        lines.push(format!("{indent}{}", g.id));
        lines.extend(g.rows.iter().map(|r| format!("{indent}  {}", row(r))));
    }
}

/// `site  kind certainty  from -> to`.
///
/// Example: `"src/lib/views.cljs:13:3  calls certain  clj:.//lib.views/mount! -> ext:clj//reagent.dom/render"`.
fn row(r: &UsageRow) -> String {
    let at = r
        .site
        .as_ref()
        .map(|s| format!("{}:{}:{}", s.file, s.line, s.col))
        .unwrap_or_else(|| "-".into());
    let cert = r
        .certainty
        .as_ref()
        .map(|c| format!(" {}", token(c)))
        .unwrap_or_default();
    format!("{at}  {}{cert}  {} -> {}", token(&r.kind), r.from, r.to)
}
