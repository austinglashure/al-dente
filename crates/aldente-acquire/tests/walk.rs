//! The file walk on the clj-mini fixture and on hostile trees built at test
//! time (symlinks, odd names and huge files cannot all be committed to git
//! portably). Hostile cases 2, 3 and 8 from `docs/plan/07-safety.md`.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use aldente_acquire::walk::{FileKind, Limits, Walk, native_path, walk};
use aldente_model::{DiagnosticCode, RepoPath};

/// A fresh directory under the system temp dir (outside any git repository).
///
/// Example: `scratch("odd")` gives `/tmp/aldente-walk-<pid>-odd`, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aldente-walk-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Writes `contents` to `root/rel`, creating parent directories.
///
/// Example: `put(root, "src/a.clj", "(ns a)")`.
fn put(root: &Path, rel: &str, contents: &[u8]) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// Paths of walked files, as strings.
///
/// Example: `["deps.edn", "src/a.clj"]`.
fn paths(w: &Walk) -> Vec<&str> {
    w.files.iter().map(|f| f.path.as_str()).collect()
}

/// Codes of diagnostics about `path`.
///
/// Example: `codes(&w, "big.clj")` gives `[DiagnosticCode::TooLarge]`.
fn codes(w: &Walk, path: &str) -> Vec<DiagnosticCode> {
    w.diagnostics
        .iter()
        .filter(|d| d.path.as_ref().map(RepoPath::as_str) == Some(path))
        .map(|d| d.code)
        .collect()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn clj_mini_walk_finds_sources_and_the_manifest() {
    let w = walk(&fixture("clj-mini"), &Limits::default());
    let p = paths(&w);
    assert_eq!(p.len(), 14, "{p:?}");
    assert!(p.contains(&"deps.edn"));
    assert!(p.contains(&"src/shop/model.cljc"));
    assert!(p.contains(&"test/shop/pricing_test.clj"));
    assert!(!p.iter().any(|f| f.ends_with(".md") || f.ends_with(".snap")));
    let mut sorted = p.clone();
    sorted.sort_unstable();
    assert_eq!(p, sorted, "files are sorted");
    assert!(w.diagnostics.is_empty(), "{:?}", w.diagnostics);
    let deps = w
        .files
        .iter()
        .find(|f| f.path.as_str() == "deps.edn")
        .unwrap();
    assert!(matches!(deps.kind, FileKind::Manifest(_)));
    assert_eq!(deps.hash.len(), 64);
}

#[test]
fn walks_are_deterministic() {
    let a = walk(&fixture("clj-mini"), &Limits::default());
    let b = walk(&fixture("clj-mini"), &Limits::default());
    assert_eq!(a, b);
}

#[test]
fn deny_listed_dirs_gitignored_and_binary_files_are_skipped() {
    let root = scratch("skips");
    put(&root, "src/a.clj", b"(ns a)");
    put(&root, "node_modules/x/y.cljs", b"(ns y)");
    put(&root, "target/classes/z.clj", b"(ns z)");
    put(&root, ".git/hooks/h.clj", b"(ns h)");
    put(&root, ".gitignore", b"ignored.clj\n");
    put(&root, "ignored.clj", b"(ns ignored)");
    put(&root, "bin.clj", b"(ns bin)\0\x01\x02");
    put(&root, "README.md", b"# readme");
    let w = walk(&root, &Limits::default());
    assert_eq!(paths(&w), ["src/a.clj"]);
    assert_eq!(codes(&w, "node_modules"), [DiagnosticCode::SkippedDir]);
    assert_eq!(codes(&w, "target"), [DiagnosticCode::SkippedDir]);
    assert!(codes(&w, ".git").is_empty(), ".git is skipped silently");
    assert_eq!(codes(&w, "bin.clj"), [DiagnosticCode::Binary]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn global_gitignore_is_not_consulted() {
    // Results must not depend on the user's machine. A `.ignore` file (ripgrep's
    // convention) is not git's and is also ignored.
    let root = scratch("dotignore");
    put(&root, ".ignore", b"a.clj\n");
    put(&root, "a.clj", b"(ns a)");
    assert_eq!(paths(&walk(&root, &Limits::default())), ["a.clj"]);
    fs::remove_dir_all(root).unwrap();
}

/// Hostile case 8 (file part): a 50 MB single-line file is skipped, not read.
#[test]
fn hostile_8_huge_file_is_skipped_with_a_diagnostic() {
    let root = scratch("huge");
    put(&root, "big.clj", &vec![b'a'; 50 * 1024 * 1024]);
    put(&root, "small.clj", b"(ns small)");
    let w = walk(&root, &Limits::default());
    assert_eq!(paths(&w), ["small.clj"]);
    assert_eq!(codes(&w, "big.clj"), [DiagnosticCode::TooLarge]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_caps_stop_the_walk_with_a_diagnostic() {
    let root = scratch("caps");
    for i in 0..5 {
        put(&root, &format!("f{i}.clj"), b"(ns f)");
    }
    let limits = Limits {
        max_files: 3,
        ..Limits::default()
    };
    let w = walk(&root, &limits);
    assert_eq!(w.files.len(), 3);
    assert!(
        w.diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::LimitReached && d.path.is_none())
    );
    fs::remove_dir_all(root).unwrap();
}

/// Hostile case 3: names with shell metacharacters, newlines and terminal
/// escapes survive the walk, JSON and reopening, and nothing runs.
#[test]
fn hostile_3_odd_file_names_round_trip() {
    let root = scratch("odd");
    let mut names = vec![
        "a b;$(touch pwned)&x.clj",
        "quote'%`.clj",
        "dollar$HOME.cljs",
    ];
    if cfg!(unix) {
        names.extend([
            "new\nline.clj",
            "esc\u{1b}[31mred.clj",
            "back\\slash.clj",
            "tab\there|pipe\"q.clj",
        ]);
    }
    for (i, name) in names.iter().enumerate() {
        put(
            &root,
            &format!("src/{name}"),
            format!("(ns n{i})").as_bytes(),
        );
    }
    let w = walk(&root, &Limits::default());
    assert_eq!(
        w.files.len(),
        names.len(),
        "{:?} {:?}",
        paths(&w),
        w.diagnostics
    );
    for file in &w.files {
        let json = serde_json::to_string(&file.path).unwrap();
        let back: RepoPath = serde_json::from_str(&json).unwrap();
        assert_eq!(back, file.path);
        let bytes = fs::read(native_path(&root, &back)).unwrap();
        assert_eq!(blake3::hash(&bytes).to_hex().as_str(), file.hash);
    }
    assert!(!root.join("pwned").exists());
    assert!(!root.join("src/pwned").exists());
    fs::remove_dir_all(root).unwrap();
}

/// Hostile case 2: a symlink to a file (and to a directory) outside the
/// repository is recorded and skipped, never followed.
#[cfg(unix)]
#[test]
fn hostile_2_symlinks_are_skipped_and_reported() {
    use std::os::unix::fs::symlink;
    let outside = scratch("outside");
    put(&outside, "secret.clj", b"(ns secret)");
    let root = scratch("symlinks");
    put(&root, "src/ok.clj", b"(ns ok)");
    symlink(outside.join("secret.clj"), root.join("src/evil.clj")).unwrap();
    symlink(&outside, root.join("src/linked-dir")).unwrap();
    let w = walk(&root, &Limits::default());
    assert_eq!(paths(&w), ["src/ok.clj"]);
    assert_eq!(codes(&w, "src/evil.clj"), [DiagnosticCode::Symlink]);
    assert_eq!(codes(&w, "src/linked-dir"), [DiagnosticCode::Symlink]);
    for d in &w.diagnostics {
        assert!(
            !d.message.contains(outside.to_str().unwrap()),
            "no absolute paths: {d:?}"
        );
    }
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(outside).unwrap();
}

/// A file name that is not UTF-8 cannot be a `RepoPath`; it is reported.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_names_are_reported() {
    use std::os::unix::ffi::OsStrExt;
    let root = scratch("nonutf8");
    let name = std::ffi::OsStr::from_bytes(b"bad\xff.clj");
    fs::write(root.join(name), b"(ns bad)").unwrap();
    let w = walk(&root, &Limits::default());
    assert!(w.files.is_empty());
    assert!(
        w.diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::NonUtf8Path)
    );
    fs::remove_dir_all(root).unwrap();
}
