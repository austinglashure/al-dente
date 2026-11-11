//! Finding `git` and `clj-kondo` on `PATH` and checking their versions.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use aldente_model::doctor::{ToolCheck, ToolStatus};

use crate::process::run_captured;

/// A dotted version, compared numerically part by part. Missing parts are 0.
pub type Version = [u64; 3];

/// A version as parsed from a tool's output: numbers for comparing, and the
/// text the tool printed for display (e.g. `2026.10.26` keeps its zero padding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub version: Version,
    pub text: String,
}

/// An external tool Al Dente needs.
#[derive(Debug)]
pub struct Tool {
    /// Executable name without extension.
    pub name: &'static str,
    /// Arguments that make it print its version.
    pub version_args: &'static [&'static str],
    pub minimum: Version,
    pub minimum_text: &'static str,
    parse: fn(&str) -> Option<Parsed>,
}

/// git: the plan's floor (`phase-00.md`, WP0.4).
pub const GIT: Tool = Tool {
    name: "git",
    version_args: &["--version"],
    minimum: [2, 30, 0],
    minimum_text: "2.30.0",
    parse: parse_git_version,
};

/// clj-kondo: the release current when WP0.4 ran (2026-11-11). CI pins the same.
pub const CLJ_KONDO: Tool = Tool {
    name: "clj-kondo",
    version_args: &["--version"],
    minimum: [2026, 10, 26],
    minimum_text: "2026.10.26",
    parse: parse_clj_kondo_version,
};

/// Every required tool, sorted by name.
pub const REQUIRED: [&Tool; 2] = [&CLJ_KONDO, &GIT];

/// How long `--version` may take before the tool counts as broken.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// Parses `git --version` output.
///
/// Example: `"git version 2.43.0.windows.1\n"` gives
/// `Parsed { version: [2, 43, 0], text: "2.43.0.windows.1" }`; `"hello"` gives `None`.
pub fn parse_git_version(output: &str) -> Option<Parsed> {
    let token = first_line(output)
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    Some(Parsed {
        version: numeric_prefix(token)?,
        text: token.to_owned(),
    })
}

/// Parses `clj-kondo --version` output.
///
/// Example: `"clj-kondo v2026.10.26\n"` gives
/// `Parsed { version: [2026, 10, 26], text: "2026.10.26" }`.
pub fn parse_clj_kondo_version(output: &str) -> Option<Parsed> {
    let token = first_line(output)
        .strip_prefix("clj-kondo v")?
        .split_whitespace()
        .next()?;
    Some(Parsed {
        version: numeric_prefix(token)?,
        text: token.to_owned(),
    })
}

/// The first line of `s`, trimmed.
///
/// Example: `"  a b \r\nc"` gives `"a b"`.
fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

/// Reads up to three leading dot-separated numbers; needs at least two.
/// A part's trailing non-digits (`27-SNAPSHOT`) end the version.
///
/// Example: `"2.43.0.windows.1"` gives `[2, 43, 0]`; `"2.39"` gives
/// `[2, 39, 0]`; `"x.1"` gives `None`.
fn numeric_prefix(token: &str) -> Option<Version> {
    let mut version = [0u64; 3];
    let mut count = 0;
    for part in token.split('.').take(3) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            break;
        }
        version[count] = digits.parse().ok()?;
        count += 1;
        if digits.len() != part.len() {
            break;
        }
    }
    (count >= 2).then_some(version)
}

/// Finds `name` in the directories of `path_var` only, never in the current
/// directory (on Windows the default search would try it first, and it may be
/// an analyzed repository). Honors `PATHEXT` on Windows.
///
/// Example: `locate("git", Some("/usr/bin".as_ref()))` gives
/// `Some("/usr/bin/git")`; `locate("git", None)` gives `None`.
pub fn locate(name: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    let path_var = path_var?;
    which::which_in(name, Some(path_var), std::env::temp_dir()).ok()
}

/// Locates `tool`, runs its version command and compares with the minimum.
///
/// Example: with a `git` printing `git version 2.20.0` on `path_var`, gives
/// `ToolCheck { status: TooOld, version: Some("2.20.0"), hint: Some(..), .. }`.
pub fn check(tool: &Tool, path_var: Option<&OsStr>, os: &str) -> ToolCheck {
    let mut result = ToolCheck {
        name: tool.name.to_owned(),
        status: ToolStatus::Missing,
        path: None,
        version: None,
        minimum: tool.minimum_text.to_owned(),
        detail: None,
        hint: None,
    };
    let Some(path) = locate(tool.name, path_var) else {
        result.hint = Some(install_hint(tool, os));
        return result;
    };
    result.path = Some(path.to_string_lossy().into_owned());
    let (status, version, detail) = probe_version(tool, &path);
    result.status = status;
    result.version = version;
    result.detail = detail;
    if status != ToolStatus::Ok {
        result.hint = Some(install_hint(tool, os));
    }
    result
}

/// Runs the version command at `path` and classifies the result.
///
/// Example: a `clj-kondo` printing `clj-kondo v2026.10.26` gives
/// `(Ok, Some("2026.10.26"), None)`; one printing `oops` gives
/// `(UnknownVersion, None, Some("unrecognized version output: oops"))`.
fn probe_version(tool: &Tool, path: &Path) -> (ToolStatus, Option<String>, Option<String>) {
    let output = match run_captured(path, tool.version_args, VERSION_TIMEOUT) {
        Ok(output) => output,
        Err(err) => return (ToolStatus::UnknownVersion, None, Some(err.to_string())),
    };
    match (tool.parse)(&output.stdout) {
        Some(parsed) if parsed.version >= tool.minimum => (ToolStatus::Ok, Some(parsed.text), None),
        Some(parsed) => (ToolStatus::TooOld, Some(parsed.text), None),
        None => {
            let shown = first_line(&output.stdout)
                .chars()
                .take(200)
                .collect::<String>();
            (
                ToolStatus::UnknownVersion,
                None,
                Some(format!("unrecognized version output: {shown}")),
            )
        }
    }
}

/// How to install or upgrade `tool` on `os` (`std::env::consts::OS` values).
///
/// Example: `install_hint(&GIT, "windows")` gives
/// `"needs git 2.30.0 or newer: winget install Git.Git"`.
pub fn install_hint(tool: &Tool, os: &str) -> String {
    let how = match (tool.name, os) {
        ("git", "macos") => "xcode-select --install, or brew install git",
        ("git", "windows") => "winget install Git.Git",
        ("git", _) => "install git with your package manager, e.g. sudo apt install git",
        (_, "windows") => {
            "download clj-kondo-<version>-windows-amd64.zip from https://github.com/clj-kondo/clj-kondo/releases and put clj-kondo.exe on PATH"
        }
        (_, "macos") => "brew install borkdude/brew/clj-kondo",
        _ => "see https://github.com/clj-kondo/clj-kondo/blob/master/doc/install.md",
    };
    format!("needs {} {} or newer: {how}", tool.name, tool.minimum_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_versions_seen_in_the_wild() {
        let v = |s| parse_git_version(s).map(|p| p.version);
        assert_eq!(v("git version 2.34.1\n"), Some([2, 34, 1]));
        assert_eq!(v("git version 2.43.0.windows.1\r\n"), Some([2, 43, 0]));
        assert_eq!(v("git version 2.39.5 (Apple Git-154)\n"), Some([2, 39, 5]));
        assert_eq!(v("git version 2.30\n"), Some([2, 30, 0]));
        assert_eq!(
            parse_git_version("git version 2.43.0.windows.1").map(|p| p.text),
            Some("2.43.0.windows.1".to_owned())
        );
        assert_eq!(v("hub version 2.14.2"), None);
        assert_eq!(v("git version banana"), None);
        assert_eq!(v(""), None);
    }

    #[test]
    fn parses_clj_kondo_versions() {
        let p = parse_clj_kondo_version("clj-kondo v2026.10.26\n").unwrap();
        assert_eq!(p.version, [2026, 10, 26]);
        assert_eq!(p.text, "2026.10.26");
        assert_eq!(
            parse_clj_kondo_version("clj-kondo v2026.10.27-SNAPSHOT").map(|p| p.version),
            Some([2026, 10, 27])
        );
        assert_eq!(parse_clj_kondo_version("v2026.10.26"), None);
    }

    #[test]
    fn minimum_text_matches_minimum() {
        assert_eq!(
            parse_git_version("git version 2.30.0").map(|p| p.version),
            Some(GIT.minimum)
        );
        assert_eq!(
            parse_clj_kondo_version(&format!("clj-kondo v{}", CLJ_KONDO.minimum_text))
                .map(|p| p.version),
            Some(CLJ_KONDO.minimum)
        );
    }

    #[test]
    fn versions_compare_numerically() {
        assert!([2, 9, 0] < [2, 30, 0]);
        assert!([2026, 9, 30] < CLJ_KONDO.minimum);
        assert!([2027, 1, 1] > CLJ_KONDO.minimum);
    }

    #[test]
    fn required_tools_are_sorted() {
        let names: Vec<_> = REQUIRED.iter().map(|t| t.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn no_path_means_missing() {
        assert_eq!(locate("git", None), None);
        let check = check(&GIT, None, "linux");
        assert_eq!(check.status, ToolStatus::Missing);
        assert!(check.hint.unwrap().contains("2.30.0"));
    }

    #[test]
    fn every_platform_gets_a_hint() {
        for tool in REQUIRED {
            for os in ["linux", "macos", "windows", "freebsd"] {
                assert!(install_hint(tool, os).starts_with(&format!("needs {}", tool.name)));
            }
        }
    }
}
