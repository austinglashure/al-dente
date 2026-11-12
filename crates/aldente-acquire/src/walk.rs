//! Walking a repository: which files exist, which are skipped and why.
//! Follows the "Reading files" rules in `docs/plan/07-safety.md`.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aldente_model::{Diagnostic, DiagnosticCode, Lang, RepoPath};
use ignore::{DirEntry, WalkBuilder};

/// Per-file and per-run caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Larger source files are skipped (default 2 MB, `07-safety.md`).
    pub max_file_bytes: u64,
    /// The walk stops after this many kept files.
    pub max_files: usize,
    /// The walk stops once kept files total this many bytes.
    pub max_total_bytes: u64,
}

impl Default for Limits {
    /// 2 MB per file, 200,000 files, 1 GiB in total.
    ///
    /// Example: `Limits::default().max_file_bytes` is `2_097_152`.
    fn default() -> Self {
        Limits {
            max_file_bytes: 2 * 1024 * 1024,
            max_files: 200_000,
            max_total_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// Directories never entered (`07-safety.md`). `.git` is skipped without a diagnostic.
pub const DENY_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cpcache",
    ".shadow-cljs",
    "dist",
    "out",
];

/// What a kept file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Source(SourceExt),
    Manifest(Manifest),
    /// A `Dockerfile`: marks its unit as a service.
    Dockerfile,
}

/// Source file extensions the Clojure adapter reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceExt {
    Clj,
    Cljs,
    Cljc,
    /// Babashka scripts, analyzed as Clojure.
    Bb,
}

impl SourceExt {
    /// The languages a file with this extension is analyzed as.
    ///
    /// Example: `SourceExt::Cljc.langs()` is `[Clj, Cljs]`.
    pub fn langs(self) -> &'static [Lang] {
        match self {
            SourceExt::Clj | SourceExt::Bb => &[Lang::Clj],
            SourceExt::Cljs => &[Lang::Cljs],
            SourceExt::Cljc => &[Lang::Clj, Lang::Cljs],
        }
    }
}

/// Build manifests that make a directory a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Manifest {
    BbEdn,
    DepsEdn,
    ProjectClj,
    ShadowCljs,
}

impl Manifest {
    /// Every manifest, sorted by file name.
    pub const ALL: [Manifest; 4] = [
        Manifest::BbEdn,
        Manifest::DepsEdn,
        Manifest::ProjectClj,
        Manifest::ShadowCljs,
    ];

    /// The file name.
    ///
    /// Example: `Manifest::ShadowCljs.file_name()` is `"shadow-cljs.edn"`.
    pub fn file_name(self) -> &'static str {
        match self {
            Manifest::BbEdn => "bb.edn",
            Manifest::DepsEdn => "deps.edn",
            Manifest::ProjectClj => "project.clj",
            Manifest::ShadowCljs => "shadow-cljs.edn",
        }
    }
}

/// Classifies a file by name. Other files are not read at all.
///
/// Example: `classify("project.clj")` is `Some(Manifest(ProjectClj))`;
/// `classify("core.cljc")` is `Some(Source(Cljc))`; `classify("a.md")` is `None`.
pub fn classify(name: &str) -> Option<FileKind> {
    if let Some(m) = Manifest::ALL.into_iter().find(|m| m.file_name() == name) {
        return Some(FileKind::Manifest(m));
    }
    if name == "Dockerfile" {
        return Some(FileKind::Dockerfile);
    }
    let ext = match name.rsplit_once('.')?.1 {
        "clj" => SourceExt::Clj,
        "cljs" => SourceExt::Cljs,
        "cljc" => SourceExt::Cljc,
        "bb" => SourceExt::Bb,
        _ => return None,
    };
    Some(FileKind::Source(ext))
}

/// A kept file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFile {
    pub path: RepoPath,
    pub kind: FileKind,
    /// blake3 of the bytes as stored, lowercase hex.
    pub hash: String,
    pub bytes: u64,
}

/// The result of a walk: kept files sorted by path, diagnostics sorted.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Walk {
    pub files: Vec<WalkedFile>,
    pub diagnostics: Vec<Diagnostic>,
}

/// How many leading bytes are checked for NUL when sniffing binaries.
const SNIFF_BYTES: usize = 8000;

/// Walks `root` (canonicalized first): gitignore-aware (the
/// repository's own `.gitignore` files only), no symlink following,
/// deny-listed directories skipped, oversized and binary source files
/// skipped, every skip recorded as a diagnostic.
///
/// Example: on `fixtures/clj-mini` gives 14 files (`deps.edn` and 13 sources)
/// and no diagnostics.
pub fn walk(root: &Path, limits: &Limits) -> Walk {
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(err) => {
            return Walk {
                files: Vec::new(),
                diagnostics: vec![Diagnostic::new(
                    None,
                    DiagnosticCode::Unreadable,
                    format!("cannot open the root: {}", err.kind()),
                )],
            };
        }
    };
    let root = root.as_path();
    let skipped_dirs = Arc::new(Mutex::new(Vec::new()));
    let mut out = Walk::default();
    let mut total_bytes = 0u64;
    for entry in builder(root, Arc::clone(&skipped_dirs)).build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                out.diagnostics.push(walk_error(root, &err));
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let Some(file) = visit(root, &entry, limits, &mut out.diagnostics) else {
            continue;
        };
        if out.files.len() >= limits.max_files || total_bytes + file.bytes > limits.max_total_bytes
        {
            out.diagnostics.push(Diagnostic::new(
                None,
                DiagnosticCode::LimitReached,
                format!(
                    "stopped after {} files and {} bytes (caps: {} files, {} bytes)",
                    out.files.len(),
                    total_bytes,
                    limits.max_files,
                    limits.max_total_bytes
                ),
            ));
            break;
        }
        total_bytes += file.bytes;
        out.files.push(file);
    }
    if let Ok(mut dirs) = skipped_dirs.lock() {
        out.diagnostics.append(&mut dirs);
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    out.diagnostics.sort();
    out.diagnostics.dedup();
    out
}

/// Configures the walker. Only the repository's own `.gitignore` files apply:
/// no global gitignore, no `.git/info/exclude`, no `.ignore` files, nothing
/// above the root, so results do not depend on the machine.
///
/// Example: `builder(root, dirs).build()` iterates entries under `root`.
fn builder(root: &Path, skipped: Arc<Mutex<Vec<Diagnostic>>>) -> WalkBuilder {
    let mut b = WalkBuilder::new(root);
    b.follow_links(false)
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_exclude(false)
        .git_ignore(true)
        .require_git(false)
        .sort_by_file_name(|a, b| a.cmp(b));
    let root = root.to_path_buf();
    b.filter_entry(move |entry| {
        let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
        let name = entry.file_name().to_string_lossy();
        if !is_dir || !DENY_DIRS.contains(&name.as_ref()) {
            return true;
        }
        if name != ".git" {
            if let (Some(path), Ok(mut list)) = (relative(&root, entry.path()).ok(), skipped.lock())
            {
                list.push(Diagnostic::new(
                    Some(path),
                    DiagnosticCode::SkippedDir,
                    "deny-listed directory, not entered",
                ));
            }
        }
        false
    });
    b
}

/// Handles one entry: returns the kept file, or records why it was skipped.
///
/// Example: a 50 MB `big.clj` gives `None` and a `too-large` diagnostic.
fn visit(
    root: &Path,
    entry: &DirEntry,
    limits: &Limits,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<WalkedFile> {
    let path = match relative(root, entry.path()) {
        Ok(path) => path,
        Err(code) => {
            diagnostics.push(Diagnostic::new(
                None,
                code,
                format!(
                    "file name is not valid UTF-8: {}",
                    entry.file_name().to_string_lossy()
                ),
            ));
            return None;
        }
    };
    if entry.path_is_symlink() {
        diagnostics.push(Diagnostic::new(
            Some(path),
            DiagnosticCode::Symlink,
            "symbolic link, not followed",
        ));
        return None;
    }
    if !entry.file_type().is_some_and(|t| t.is_file()) {
        return None;
    }
    let kind = classify(&entry.file_name().to_string_lossy())?;
    let mut skip = |code, message: String| {
        diagnostics.push(Diagnostic::new(Some(path.clone()), code, message));
        None
    };
    let size = match entry.metadata() {
        Ok(meta) => meta.len(),
        Err(_) => return skip(DiagnosticCode::Unreadable, "cannot read metadata".into()),
    };
    if size > limits.max_file_bytes {
        return skip(
            DiagnosticCode::TooLarge,
            format!("{size} bytes, over the {} byte cap", limits.max_file_bytes),
        );
    }
    let bytes = match read_inside(root, &path, limits.max_file_bytes) {
        Ok(bytes) => bytes,
        Err(ReadError::Outside) => {
            return skip(
                DiagnosticCode::OutsideRepo,
                "resolves outside the repository".into(),
            );
        }
        Err(ReadError::TooLarge) => {
            return skip(
                DiagnosticCode::TooLarge,
                "grew over the size cap while reading".into(),
            );
        }
        Err(ReadError::Io(kind)) => {
            return skip(DiagnosticCode::Unreadable, format!("cannot read: {kind}"));
        }
    };
    if bytes.iter().take(SNIFF_BYTES).any(|&b| b == 0) {
        return skip(DiagnosticCode::Binary, "looks binary (NUL byte)".into());
    }
    Some(WalkedFile {
        path,
        kind,
        hash: blake3::hash(&bytes).to_hex().to_string(),
        bytes: bytes.len() as u64,
    })
}

/// `path` relative to `root` as a [`RepoPath`].
///
/// Example: root `/r`, path `/r/src/a.clj` gives `src/a.clj`; a non-UTF-8
/// name gives `Err(NonUtf8Path)`.
fn relative(root: &Path, path: &Path) -> Result<RepoPath, DiagnosticCode> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| DiagnosticCode::OutsideRepo)?;
    RepoPath::from_native(rel).map_err(|err| match err {
        aldente_model::PathError::NotUtf8 => DiagnosticCode::NonUtf8Path,
        _ => DiagnosticCode::Unreadable,
    })
}

/// A diagnostic for an error the walker itself hit, without absolute paths.
///
/// Example: a permission error on `/r/secret` gives `unreadable` at `secret`.
fn walk_error(root: &Path, err: &ignore::Error) -> Diagnostic {
    let path = walk_error_path(err).and_then(|p| relative(root, p).ok());
    let reason = err
        .io_error()
        .map_or_else(|| "walk error".to_owned(), |io| io.kind().to_string());
    Diagnostic::new(
        path,
        DiagnosticCode::Unreadable,
        format!("cannot read: {reason}"),
    )
}

/// The path an `ignore` error is about, if it names one.
///
/// Example: `WithPath { path: "/r/x", .. }` gives `Some("/r/x")`.
fn walk_error_path(err: &ignore::Error) -> Option<&Path> {
    match err {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => {
            walk_error_path(err)
        }
        _ => None,
    }
}

/// Why [`read_inside`] refused or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The canonical location is outside the root.
    Outside,
    /// More than the allowed number of bytes.
    TooLarge,
    Io(std::io::ErrorKind),
}

/// Reads `root/path` after canonicalizing it and checking it is still inside
/// `root` (which must itself be canonical), reading at most `max_bytes`.
///
/// Example: `read_inside(root, "deps.edn", 2_097_152)` gives its bytes; a path
/// that resolves through a symlink to `/etc/passwd` gives `Err(Outside)`.
pub fn read_inside(root: &Path, path: &RepoPath, max_bytes: u64) -> Result<Vec<u8>, ReadError> {
    let real = native_path(root, path)
        .canonicalize()
        .map_err(|e| ReadError::Io(e.kind()))?;
    if !real.starts_with(root) {
        return Err(ReadError::Outside);
    }
    let file = File::open(&real).map_err(|e| ReadError::Io(e.kind()))?;
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ReadError::Io(e.kind()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(ReadError::TooLarge);
    }
    Ok(bytes)
}

/// The native path of `path` under `root`.
///
/// Example: root `C:\r`, `src/a.clj` gives `C:\r\src\a.clj` on Windows.
pub fn native_path(root: &Path, path: &RepoPath) -> PathBuf {
    let mut out = root.to_path_buf();
    out.extend(path.components());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_name() {
        assert_eq!(
            classify("deps.edn"),
            Some(FileKind::Manifest(Manifest::DepsEdn))
        );
        assert_eq!(
            classify("project.clj"),
            Some(FileKind::Manifest(Manifest::ProjectClj))
        );
        assert_eq!(classify("a.cljc"), Some(FileKind::Source(SourceExt::Cljc)));
        assert_eq!(classify("build.bb"), Some(FileKind::Source(SourceExt::Bb)));
        assert_eq!(classify("Dockerfile"), Some(FileKind::Dockerfile));
        assert_eq!(classify("config.edn"), None);
        assert_eq!(classify("clj"), None);
        assert_eq!(classify("README.md"), None);
    }

    #[test]
    fn manifests_are_sorted_by_file_name() {
        let names: Vec<_> = Manifest::ALL.iter().map(|m| m.file_name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
    }

    #[test]
    fn native_path_joins_components() {
        let p = RepoPath::parse("src/a.clj").unwrap();
        assert_eq!(
            native_path(Path::new("/r"), &p),
            Path::new("/r").join("src").join("a.clj")
        );
        assert_eq!(
            native_path(Path::new("/r"), &RepoPath::root()),
            Path::new("/r")
        );
    }
}
