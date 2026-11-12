//! `aldente cache ls|gc|clear`: manages clones and snapshots.

use aldente_acquire::cache::{Layout, default_dir};
use aldente_model::cache::{CacheRemoval, CacheReport};
use aldente_model::{Envelope, ErrorCode, ExitCode};
use clap::{Args, Subcommand};

use crate::commands::target::Failure;
use crate::output::{fail, print_json, print_lines, printable};

/// Arguments for `aldente cache`.
#[derive(Debug, Args)]
pub struct CacheArgs {
    #[command(subcommand)]
    action: CacheAction,
}

#[derive(Debug, Subcommand)]
enum CacheAction {
    /// List remote clones and snapshots with their sizes.
    Ls,
    /// Remove temporary files.
    Gc,
    /// Remove every clone, snapshot and temporary file.
    Clear,
}

/// Runs the cache action.
///
/// Example: `aldente cache ls` prints one line per clone and snapshot.
pub fn run(args: &CacheArgs, json: bool) -> anyhow::Result<ExitCode> {
    let layout = match default_dir().map(|d| Layout::open(&d)) {
        Some(Ok(layout)) => layout,
        Some(Err(e)) => {
            return fail(
                "cache",
                &Failure::new(ErrorCode::Internal, format!("cannot open the cache: {e}")),
                json,
            );
        }
        None => {
            return fail(
                "cache",
                &Failure::new(
                    ErrorCode::Internal,
                    "no user cache directory; set ALDENTE_CACHE_DIR",
                ),
                json,
            );
        }
    };
    match args.action {
        CacheAction::Ls => {
            let report = layout.list();
            if json {
                print_json(&Envelope::success("cache", None, &report))?;
            } else {
                print_lines(render_ls(&report).iter().map(String::as_str))?;
            }
        }
        CacheAction::Gc | CacheAction::Clear => {
            let result = if matches!(args.action, CacheAction::Gc) {
                layout.gc()
            } else {
                layout.clear()
            };
            let removal = match result {
                Ok(removal) => removal,
                Err(e) => {
                    return fail(
                        "cache-removal",
                        &Failure::new(ErrorCode::Internal, format!("cannot remove: {e}")),
                        json,
                    );
                }
            };
            if json {
                print_json(&Envelope::success("cache-removal", None, &removal))?;
            } else {
                print_lines(render_removal(&removal).iter().map(String::as_str))?;
            }
        }
    }
    Ok(ExitCode::Success)
}

/// Text form of `cache ls`.
///
/// Example: a cache with one clone gives
/// `["cache /c/aldente  1.2 MB", "repo      github.com/a/b  1.2 MB"]`.
fn render_ls(report: &CacheReport) -> Vec<String> {
    let mut lines = vec![printable(&format!(
        "cache {}  {}",
        report.root,
        human_bytes(report.bytes)
    ))];
    for repo in &report.repos {
        lines.push(printable(&format!(
            "repo      {}  {}",
            repo.path,
            human_bytes(repo.bytes)
        )));
    }
    for snap in &report.snapshots {
        lines.push(format!(
            "snapshot  {}  {}",
            snap.key,
            human_bytes(snap.bytes)
        ));
    }
    lines
}

/// Text form of `gc` and `clear`.
///
/// Example: two removed entries give `["removed tmp/a", "removed tmp/b", "2 entries, 10 B"]`.
fn render_removal(removal: &CacheRemoval) -> Vec<String> {
    let mut lines: Vec<String> = removal
        .removed
        .iter()
        .map(|r| printable(&format!("removed {r}")))
        .collect();
    lines.push(format!(
        "{} entries, {}",
        removal.removed.len(),
        human_bytes(removal.bytes)
    ));
    lines
}

/// A size with a binary unit, one decimal.
///
/// Example: `human_bytes(1536)` is `"1.5 KiB"`; `human_bytes(10)` is `"10 B"`.
fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(human_bytes(10), "10 B");
        assert_eq!(human_bytes(1536), "1.5 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
    }
}
