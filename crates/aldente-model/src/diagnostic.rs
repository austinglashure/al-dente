//! Things the analyzer skipped or could not read, reported instead of dropped.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::path::RepoPath;

/// One skipped or unreadable thing. Sorted by `(path, code, message)` in output.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Diagnostic {
    /// Null when the path cannot be represented (e.g. not UTF-8) or the
    /// diagnostic is about the whole run.
    pub path: Option<RepoPath>,
    pub code: DiagnosticCode,
    pub message: String,
}

/// Why something was skipped.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCode {
    /// A symbolic link; never followed.
    Symlink,
    /// A source file over the size cap.
    TooLarge,
    /// A source-named file that looks binary.
    Binary,
    /// A file whose real location is outside the repository root.
    OutsideRepo,
    /// A file name that is not valid UTF-8.
    NonUtf8Path,
    /// A file or directory that could not be read.
    Unreadable,
    /// A deny-listed directory such as `node_modules` or `target`.
    SkippedDir,
    /// The per-run file or byte cap was reached; later files were not read.
    LimitReached,
    /// A build manifest that could not be parsed.
    ManifestError,
    /// A source root declared in a manifest that points outside the repository.
    RootOutsideRepo,
    /// A declared source root that does not exist.
    MissingRoot,
    /// A source file inside a unit but under none of its source roots.
    OutsideRoots,
    /// A git submodule; never fetched, so its contents are not analyzed.
    Submodule,
    /// A source file with syntax errors; only what could be read is analyzed.
    ParseError,
    /// The Clojure front end failed for a unit; its files are not analyzed.
    AnalyzerFailed,
}

impl Diagnostic {
    /// Builds a diagnostic.
    ///
    /// Example: `Diagnostic::new(Some(p), DiagnosticCode::Symlink, "symlink, not followed")`.
    pub fn new(path: Option<RepoPath>, code: DiagnosticCode, message: impl Into<String>) -> Self {
        Diagnostic {
            path,
            code,
            message: message.into(),
        }
    }
}
