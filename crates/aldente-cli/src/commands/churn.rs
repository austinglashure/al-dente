//! `aldente churn`: change frequency and co-change coupling from git
//! history. Reads history only (hardened `git log`); for a shallow remote
//! clone, fetches more history first, since the user asked for it.

use std::path::Path;

use aldente_acquire::history::{self, LogOptions};
use aldente_acquire::local_git::head_rev;
use aldente_acquire::remote::{deepen, looks_like_url, parse_url};
use aldente_analysis::churn::{ChurnOptions, History, churn};
use aldente_model::churn::{ChurnReport, HistoryDepth};
use aldente_model::{Envelope, ErrorCode, ExitCode, RepoPath, Warning, WarningCode};
use clap::Args;

use crate::commands::crater_diff::hardened_git;
use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json, print_lines, printable};

/// Arguments for `aldente churn`.
#[derive(Debug, Args)]
pub struct ChurnArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Only commits on or after this date (YYYY-MM-DD).
    #[arg(long)]
    since: Option<String>,
    /// Most commits read for file churn and co-change.
    #[arg(long, default_value_t = 1000)]
    max_commits: usize,
    /// The newest commits whose changed lines are matched to symbols.
    #[arg(long, default_value_t = 200)]
    symbol_commits: usize,
    /// Least commits a pair of files must share.
    #[arg(long, default_value_t = 3)]
    min_support: u32,
    /// Least confidence (either direction) of a pair.
    #[arg(long, default_value_t = 0.5)]
    min_confidence: f64,
    /// Commits touching more files are left out of co-change.
    #[arg(long, default_value_t = 50)]
    max_files_per_commit: usize,
    /// Rows per table in text output (JSON is always complete).
    #[arg(long, default_value_t = 20)]
    top: usize,
}

/// Runs `aldente churn`.
///
/// Example: `aldente churn --since 2026-01-01` prints the most changed files,
/// symbols and co-changing pairs.
pub fn run(args: &ChurnArgs, json: bool) -> anyhow::Result<ExitCode> {
    const TAG: &str = "churn";
    if let Some(d) = &args.since
        && history::check_date(d).is_err()
    {
        let f = Failure::new(
            ErrorCode::Usage,
            format!("--since takes YYYY-MM-DD, not {d:?}"),
        );
        return fail(TAG, &f, json);
    }
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let req = HistoryRequest {
        since: args.since.clone(),
        max_commits: args.max_commits,
        symbol_commits: args.symbol_commits,
    };
    let (hist, warnings) = match read_history(
        &req,
        &loaded.target.root,
        loaded.target.git,
        &loaded.target.origin,
    ) {
        Ok(v) => v,
        Err(f) => return fail(TAG, &f, json),
    };
    let root = loaded.target.root.clone();
    let exists = move |p: &str| {
        RepoPath::parse(p).is_ok()
            && std::fs::symlink_metadata(root.join(p)).is_ok_and(|m| m.is_file())
    };
    let opts = ChurnOptions {
        min_support: args.min_support,
        min_confidence: args.min_confidence,
        max_files_per_commit: args.max_files_per_commit,
        ..ChurnOptions::default()
    };
    let report = churn(&loaded.outcome.snapshot.graph, &hist, &exists, &opts);
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

/// What history to read.
#[derive(Debug, Clone)]
pub(crate) struct HistoryRequest {
    pub since: Option<String>,
    pub max_commits: usize,
    pub symbol_commits: usize,
}

impl Default for HistoryRequest {
    fn default() -> Self {
        HistoryRequest {
            since: None,
            max_commits: 1000,
            symbol_commits: 200,
        }
    }
}

/// Reads the commits, deepening a shallow remote clone first. No git
/// history is an empty result with a warning, not an error.
///
/// Example: a directory without `.git` gives `HistoryDepth::None` and a
/// `no-history` warning.
pub(crate) fn read_history(
    args: &HistoryRequest,
    root: &Path,
    git_repo: bool,
    origin: &str,
) -> Result<(History, Vec<Warning>), Failure> {
    let mut warnings = Vec::new();
    let empty = |depth| History {
        depth,
        commits: vec![],
        hunks: vec![],
        since: args.since.clone(),
    };
    let no_history = |message: &str| Warning {
        code: WarningCode::NoHistory,
        message: message.to_owned(),
    };
    if !git_repo {
        warnings.push(no_history(
            "the target is not a git repository; there is no history to read",
        ));
        return Ok((empty(HistoryDepth::None), warnings));
    }
    let git = hardened_git()?;
    if head_rev(&git, root).is_none() {
        warnings.push(no_history("the repository has no commits"));
        return Ok((empty(HistoryDepth::None), warnings));
    }
    if history::is_shallow(root) && looks_like_url(origin) {
        eprintln!(
            "aldente: fetching up to {} commits of history…",
            args.max_commits
        );
        let fetched = parse_url(origin)
            .map_err(|e| e.to_string())
            .and_then(|u| deepen(&git, root, &u, args.max_commits).map_err(|e| e.to_string()));
        if let Err(e) = fetched {
            warnings.push(Warning {
                code: WarningCode::ShallowHistory,
                message: format!("could not fetch more history: {e}"),
            });
        }
    }
    let opts = LogOptions {
        max_commits: args.max_commits,
        since: args.since.clone(),
    };
    let git_err = |e: aldente_acquire::local_git::HistoryError| {
        Failure::new(ErrorCode::Internal, e.to_string())
    };
    let commits = history::numstat(&git, root, &opts).map_err(git_err)?;
    let window = LogOptions {
        max_commits: args.symbol_commits.min(args.max_commits),
        ..opts
    };
    let hunks = history::hunks(&git, root, &window).map_err(git_err)?;
    // Still shallow and the boundary was reached: older commits are missing.
    let depth = if history::is_shallow(root) && commits.len() < args.max_commits {
        warnings.push(Warning {
            code: WarningCode::ShallowHistory,
            message: format!(
                "a shallow clone: only {} commits of history are present",
                commits.len()
            ),
        });
        HistoryDepth::Shallow
    } else {
        HistoryDepth::Full
    };
    Ok((
        History {
            depth,
            commits,
            hunks,
            since: args.since.clone(),
        },
        warnings,
    ))
}

/// The text report: most changed files, symbols, and co-changing pairs.
///
/// Example: a line `   12  +340 -120  2026-11-01  src/app/core.clj`.
fn render(r: &ChurnReport, top: usize) -> Vec<String> {
    let mut out = vec![format!(
        "{} commits read{}; symbols matched over the newest {}",
        r.commits,
        r.since
            .as_ref()
            .map(|s| format!(" since {s}"))
            .unwrap_or_default(),
        r.symbol_commits
    )];
    out.push(String::new());
    out.push(format!("Most changed files ({} in all):", r.files.len()));
    for f in r.files.iter().take(top) {
        out.push(format!(
            "{:>6}  +{:<6} -{:<6} {}  {}",
            f.commits, f.added, f.deleted, f.last_changed, f.path
        ));
    }
    out.push(String::new());
    out.push(format!(
        "Most changed symbols ({} in all, estimated from current line ranges):",
        r.symbols.len()
    ));
    for s in r.symbols.iter().take(top) {
        out.push(format!("{:>6}  {}", s.commits, s.id));
    }
    out.push(String::new());
    out.push(format!(
        "Files that change together ({} pairs; * = no static path within two hops):",
        r.co_changes.len()
    ));
    for c in r.co_changes.iter().take(top) {
        out.push(format!(
            "{:>6}  {:.0}%/{:.0}%  {} {} {}",
            c.support,
            c.confidence_a_b * 100.0,
            c.confidence_b_a * 100.0,
            if c.hidden { "*" } else { " " },
            c.a,
            c.b
        ));
    }
    out.iter().map(|l| printable(l)).collect()
}
