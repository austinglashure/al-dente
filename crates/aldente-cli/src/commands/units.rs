//! `aldente units [TARGET]`: lists build units, their source roots and what
//! the file walk skipped.

use aldente_acquire::units::discover;
use aldente_acquire::walk::{Limits, walk};
use aldente_model::{Envelope, ExitCode, RootKind, UnitsReport};
use clap::Args;

use crate::commands::target::{TargetArgs, open};
use crate::output::{fail, print_json, print_lines, printable};

/// Arguments for `aldente units`.
#[derive(Debug, Args)]
pub struct UnitsArgs {
    #[command(flatten)]
    target: TargetArgs,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "units";

/// Opens the target (cloning a URL first), walks it and prints its units.
///
/// Example: `aldente units fixtures-copy/clj-mini` prints `unit:.` with roots
/// `src` and `test` and returns `ExitCode::Success`; a missing path returns
/// `ExitCode::Usage`.
pub fn run(args: &UnitsArgs, json: bool) -> anyhow::Result<ExitCode> {
    let target = match open(&args.target) {
        Ok(target) => target,
        Err(failure) => return fail(TAG, &failure, json),
    };
    let mut found = discover(
        &target.root,
        target.git,
        walk(&target.root, &Limits::default()),
    );
    found.report.diagnostics.extend(target.diagnostics);
    found.report.diagnostics.sort();
    if json {
        print_json(&Envelope::success(TAG, None, &found.report))?;
    } else {
        let lines = render(&found.report);
        print_lines(lines.iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
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
        let mut head = unit.id.to_string();
        if !langs.is_empty() {
            head.push_str(&format!("  {}", langs.join(" ")));
        }
        head.push_str(&format!("  {} files", unit.files));
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
            let code = crate::output::token(&d.code);
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
    fn a_unit_without_languages_has_no_double_space() {
        let report = UnitsReport {
            git: true,
            units: vec![UnitInfo {
                id: NodeId::Unit(RepoPath::parse("modules/meta").unwrap()),
                dir: RepoPath::parse("modules/meta").unwrap(),
                manifests: vec!["project.clj".into()],
                langs: Default::default(),
                roots: vec![],
                service: false,
                entry_points: Vec::new(),
                files: 0,
            }],
            diagnostics: vec![],
        };
        assert_eq!(render(&report), ["unit:modules/meta  0 files  project.clj"]);
    }

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
                entry_points: Vec::new(),
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
