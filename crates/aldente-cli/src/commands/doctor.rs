//! `aldente doctor`: checks that `git` and `clj-kondo` are installed and new
//! enough and that the cache directory is writable.

use std::ffi::OsString;
use std::path::PathBuf;

use aldente_acquire::{cache, tools};
use aldente_model::doctor::{DoctorReport, Platform, ToolCheck, ToolStatus};
use aldente_model::{Envelope, ErrorCode, ExitCode};
use clap::Args;

use crate::output::{print_json, print_lines};

/// Arguments for `aldente doctor` (none yet besides the global `--json`).
#[derive(Debug, Args)]
pub struct DoctorArgs {}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "doctor";

/// Runs every check and prints the report. Exits 5 when any check fails.
///
/// Example: with `git` 2.34.1 and `clj-kondo` 2026.10.26 on `PATH`, prints
/// one `ok` line per tool and returns `ExitCode::Success`.
pub fn run(_args: &DoctorArgs, json: bool) -> anyhow::Result<ExitCode> {
    let report = diagnose(std::env::var_os("PATH"), cache::default_dir());
    let code = if report.all_ok() {
        ExitCode::Success
    } else {
        ErrorCode::ToolMissing.exit_code()
    };
    if json {
        print_json(&envelope(report))?;
    } else {
        let lines = render(&report);
        print_lines(lines.iter().map(String::as_str))?;
    }
    Ok(code)
}

/// Builds the report from the given `PATH` value and cache directory.
///
/// Example: `diagnose(None, None)` reports both tools `missing` and the cache
/// not writable.
fn diagnose(path_var: Option<OsString>, cache_dir: Option<PathBuf>) -> DoctorReport {
    let os = std::env::consts::OS;
    DoctorReport {
        tools: tools::REQUIRED
            .iter()
            .map(|tool| tools::check(tool, path_var.as_deref(), os))
            .collect(),
        cache: cache::check(cache_dir.as_deref()),
        platform: Platform {
            os: os.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        },
    }
}

/// Wraps the report. A failed check makes `ok` false with a `tool-missing`
/// error, and `data` still carries the full report.
///
/// Example: a report with `git` missing gives `{ "ok": false, "error":
/// { "code": "tool-missing", "message": "failed: git" }, "data": {…} }`.
fn envelope(report: DoctorReport) -> Envelope<DoctorReport> {
    if report.all_ok() {
        return Envelope::success(TAG, None, report);
    }
    let mut failed: Vec<&str> = report
        .tools
        .iter()
        .filter(|t| t.status != ToolStatus::Ok)
        .map(|t| t.name.as_str())
        .collect();
    if !report.cache.writable {
        failed.push("cache");
    }
    let message = format!("failed: {}", failed.join(", "));
    let mut envelope = Envelope::failure(TAG, ErrorCode::ToolMissing, &message);
    envelope.data = Some(report);
    envelope
}

/// The text form: one line per check, with indented detail and hint lines.
///
/// Example: a passing `git` gives
/// `git        ok               2.34.1 (minimum 2.30.0)  /usr/bin/git`.
fn render(report: &DoctorReport) -> Vec<String> {
    let mut lines = Vec::new();
    for tool in &report.tools {
        lines.push(row(
            &tool.name,
            status_word(tool.status),
            &tool_summary(tool),
        ));
        for extra in [&tool.detail, &tool.hint].into_iter().flatten() {
            lines.push(format!("{:11}{}", "", printable(extra)));
        }
    }
    let cache = &report.cache;
    let where_ = cache.path.as_deref().unwrap_or("(none)");
    let word = if cache.writable {
        "writable"
    } else {
        "not writable"
    };
    lines.push(row("cache", word, where_));
    if let Some(detail) = &cache.detail {
        lines.push(format!("{:11}{}", "", printable(detail)));
    }
    let platform = format!("{} {}", report.platform.os, report.platform.arch);
    lines.push(row("platform", &platform, ""));
    lines
}

/// One aligned line: label, status, then the rest.
///
/// Example: `row("git", "ok", "2.34.1")` gives `"git        ok               2.34.1"`.
fn row(label: &str, status: &str, rest: &str) -> String {
    format!("{label:10} {status:16} {}", printable(rest))
        .trim_end()
        .to_owned()
}

/// Version, minimum and path of a tool, as far as known.
///
/// Example: a too-old git gives `"2.20.0 (minimum 2.30.0)  /usr/bin/git"`;
/// a missing one gives `"(minimum 2.30.0)"`.
fn tool_summary(tool: &ToolCheck) -> String {
    let version = tool
        .version
        .as_deref()
        .map(|v| format!("{v} "))
        .unwrap_or_default();
    let path = tool
        .path
        .as_deref()
        .map(|p| format!("  {p}"))
        .unwrap_or_default();
    format!("{version}(minimum {}){path}", tool.minimum)
}

/// The status as printed, matching the JSON spelling.
///
/// Example: `status_word(ToolStatus::TooOld)` is `"too-old"`.
fn status_word(status: ToolStatus) -> &'static str {
    match status {
        ToolStatus::Ok => "ok",
        ToolStatus::Missing => "missing",
        ToolStatus::TooOld => "too-old",
        ToolStatus::UnknownVersion => "unknown-version",
    }
}

/// Replaces control characters (which could come from a tool's output or a
/// path) so they cannot move the cursor or recolor the terminal.
///
/// Example: `printable("a\u{1b}[31mb")` is `"a?[31mb"`.
fn printable(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::doctor::CacheCheck;

    fn report(git: ToolStatus) -> DoctorReport {
        DoctorReport {
            tools: vec![ToolCheck {
                name: "git".into(),
                status: git,
                path: Some("/usr/bin/git".into()),
                version: Some("2.20.0".into()),
                minimum: "2.30.0".into(),
                detail: None,
                hint: (git != ToolStatus::Ok).then(|| "needs git 2.30.0 or newer: x".into()),
            }],
            cache: CacheCheck {
                path: Some("/c/aldente".into()),
                writable: true,
                detail: None,
            },
            platform: Platform {
                os: "linux".into(),
                arch: "x86_64".into(),
            },
        }
    }

    #[test]
    fn status_words_match_the_json_spelling() {
        for status in [
            ToolStatus::Ok,
            ToolStatus::Missing,
            ToolStatus::TooOld,
            ToolStatus::UnknownVersion,
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), status_word(status),);
        }
    }

    #[test]
    fn renders_one_line_per_check_with_the_hint_indented() {
        let lines = render(&report(ToolStatus::TooOld));
        assert_eq!(
            lines,
            [
                "git        too-old          2.20.0 (minimum 2.30.0)  /usr/bin/git",
                "           needs git 2.30.0 or newer: x",
                "cache      writable         /c/aldente",
                "platform   linux x86_64",
            ]
        );
    }

    #[test]
    fn failing_envelope_keeps_the_report() {
        let env = envelope(report(ToolStatus::Missing));
        assert!(!env.ok);
        assert_eq!(env.error.unwrap().message, "failed: git");
        assert!(env.data.is_some());
        assert!(envelope(report(ToolStatus::Ok)).ok);
    }

    #[test]
    fn control_characters_are_neutralized() {
        assert_eq!(printable("a\u{1b}[31mb\n"), "a?[31mb?");
    }

    #[test]
    fn no_path_and_no_cache_fail_everything() {
        let report = diagnose(None, None);
        assert!(report.tools.iter().all(|t| t.status == ToolStatus::Missing));
        assert!(!report.cache.writable);
    }
}
