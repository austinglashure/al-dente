//! `aldente units [TARGET]`: lists build units, their source roots and what
//! the file walk skipped.

use std::path::PathBuf;

use aldente_acquire::target::resolve;
use aldente_acquire::units::discover;
use aldente_acquire::walk::{Limits, walk};
use aldente_model::{Envelope, ErrorCode, ExitCode, RootKind, UnitsReport};
use clap::Args;

use crate::output::{print_json, print_lines, printable};

/// Arguments for `aldente units`.
#[derive(Debug, Args)]
pub struct UnitsArgs {
    /// Directory to analyze; its git repository root is used when there is one.
    #[arg(default_value = ".")]
    target: PathBuf,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "units";

/// Resolves the target, walks it and prints its units.
///
/// Example: `aldente units fixtures-copy/clj-mini` prints `unit:.` with roots
/// `src` and `test` and returns `ExitCode::Success`; a missing path returns
/// `ExitCode::Usage`.
pub fn run(args: &UnitsArgs, json: bool) -> anyhow::Result<ExitCode> {
    let target = match resolve(&args.target) {
        Ok(target) => target,
        Err(err) => return usage_error(&err.to_string(), json),
    };
    if !target.git {
        eprintln!("aldente: no git repository found; using the target directory as the root");
    }
    let found = discover(
        &target.root,
        target.git,
        walk(&target.root, &Limits::default()),
    );
    if json {
        print_json(&Envelope::success(TAG, None, &found.report))?;
    } else {
        let lines = render(&found.report);
        print_lines(lines.iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// Reports a bad target as a usage error.
///
/// Example: `usage_error("/x: No such file", false)` prints
/// `aldente: /x: No such file` on stderr and returns `ExitCode::Usage`.
fn usage_error(message: &str, json: bool) -> anyhow::Result<ExitCode> {
    if json {
        print_json(&Envelope::<()>::failure(TAG, ErrorCode::Usage, message))?;
    } else {
        eprintln!("aldente: {message}");
    }
    Ok(ErrorCode::Usage.exit_code())
}

/// The text form: a line per unit, an indented line per root, then the
/// diagnostics if any.
///
/// Example: clj-mini gives
/// `unit:.  clj cljs  13 files  deps.edn`, `  source  src  deps.edn`,
/// `  test    test  deps.edn :test`.
fn render(report: &UnitsReport) -> Vec<String> {
    let mut lines = Vec::new();
    for unit in &report.units {
        let langs: Vec<&str> = unit.langs.iter().map(|l| l.as_str()).collect();
        let mut head = format!("{}  {}  {} files", unit.id, langs.join(" "), unit.files);
        if !unit.manifests.is_empty() {
            head.push_str(&format!("  {}", unit.manifests.join(" ")));
        }
        if unit.service {
            head.push_str("  service");
        }
        lines.push(printable(&head));
        for root in &unit.roots {
            let kind = match root.kind {
                RootKind::Source => "source",
                RootKind::Test => "test",
            };
            let origin = match (&root.manifest, &root.alias) {
                (Some(m), Some(a)) => format!("{m} :{a}"),
                (Some(m), None) => m.clone(),
                (None, _) => "fallback".to_owned(),
            };
            lines.push(printable(&format!("  {kind:6}  {}  {origin}", root.path)));
        }
    }
    if !report.diagnostics.is_empty() {
        lines.push(String::new());
        lines.push(format!("diagnostics ({}):", report.diagnostics.len()));
        for d in &report.diagnostics {
            let path = d.path.as_ref().map_or("-", |p| p.as_str());
            let code = serde_json::to_value(d.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            lines.push(printable(&format!("  {path}  {code}  {}", d.message)));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{Diagnostic, DiagnosticCode, Lang, NodeId, RepoPath, SourceRoot, UnitInfo};

    #[test]
    fn renders_units_roots_and_diagnostics() {
        let report = UnitsReport {
            git: true,
            units: vec![UnitInfo {
                id: NodeId::Unit(RepoPath::root()),
                dir: RepoPath::root(),
                manifests: vec!["deps.edn".into()],
                langs: [Lang::Clj, Lang::Cljs].into(),
                roots: vec![
                    SourceRoot {
                        path: RepoPath::parse("src").unwrap(),
                        kind: RootKind::Source,
                        manifest: Some("deps.edn".into()),
                        alias: None,
                    },
                    SourceRoot {
                        path: RepoPath::parse("test").unwrap(),
                        kind: RootKind::Test,
                        manifest: Some("deps.edn".into()),
                        alias: Some("test".into()),
                    },
                ],
                service: true,
                files: 13,
            }],
            diagnostics: vec![Diagnostic::new(
                Some(RepoPath::parse("evil\u{1b}[2J.clj").unwrap()),
                DiagnosticCode::Symlink,
                "symbolic link, not followed",
            )],
        };
        assert_eq!(
            render(&report),
            [
                "unit:.  clj cljs  13 files  deps.edn  service",
                "  source  src  deps.edn",
                "  test    test  deps.edn :test",
                "",
                "diagnostics (1):",
                "  evil?[2J.clj  symlink  symbolic link, not followed",
            ]
        );
    }
}
