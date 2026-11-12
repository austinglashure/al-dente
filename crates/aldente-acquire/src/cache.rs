//! The Al Dente cache directory: `<user cache dir>/aldente/`.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use aldente_model::cache::{CacheRemoval, CacheReport, CachedRepo, CachedSnapshot};
use aldente_model::doctor::CacheCheck;

/// Environment variable that overrides the cache location (used by tests and
/// by anyone who wants the cache elsewhere).
pub const CACHE_ENV: &str = "ALDENTE_CACHE_DIR";

/// The cache directory: `$ALDENTE_CACHE_DIR` if set and non-empty, else
/// `<user cache dir>/aldente`, or `None` if the platform reports no cache
/// directory (e.g. `HOME` unset on Linux).
///
/// Example: on Linux with `XDG_CACHE_HOME=/tmp/c`, gives `Some("/tmp/c/aldente")`.
pub fn default_dir() -> Option<PathBuf> {
    match std::env::var_os(CACHE_ENV) {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => dirs::cache_dir().map(|d| d.join("aldente")),
    }
}

/// The cache's subdirectories (`02-architecture.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub root: PathBuf,
    /// Remote clones: `repos/<host>/<owner>/<name>`.
    pub repos: PathBuf,
    /// `snapshots/<hex>.db`, where the key is `b3:<hex>` (no `:` in file
    /// names, which Windows forbids).
    pub snapshots: PathBuf,
    /// Scratch space, including git's empty config and template.
    pub tmp: PathBuf,
}

impl Layout {
    /// Creates the layout under `root` if needed.
    ///
    /// Example: `Layout::open("/c/aldente")` creates `/c/aldente/{repos,snapshots,tmp}`.
    pub fn open(root: &Path) -> std::io::Result<Layout> {
        let layout = Layout {
            root: root.to_path_buf(),
            repos: root.join("repos"),
            snapshots: root.join("snapshots"),
            tmp: root.join("tmp"),
        };
        for dir in [&layout.repos, &layout.snapshots, &layout.tmp] {
            fs::create_dir_all(dir)?;
        }
        Ok(layout)
    }

    /// Lists clones and snapshots with their sizes.
    ///
    /// Example: after cloning reitit, `repos` holds
    /// `CachedRepo { path: "github.com/metosin/reitit", .. }`.
    pub fn list(&self) -> CacheReport {
        let mut repos = Vec::new();
        find_clones(&self.repos, &self.repos, &mut repos);
        repos.sort();
        let mut snapshots: Vec<CachedSnapshot> = fs::read_dir(&self.snapshots)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let key = format!("b3:{}", name.strip_suffix(".db")?);
                Some(CachedSnapshot {
                    key,
                    bytes: size(&e.path()),
                })
            })
            .collect();
        snapshots.sort();
        CacheReport {
            root: self.root.display().to_string(),
            repos,
            snapshots,
            bytes: size(&self.root),
        }
    }

    /// Housekeeping: empties `tmp`. (Snapshot pruning arrives with snapshots.)
    ///
    /// Example: a leftover `tmp/x` is removed and listed as `tmp/x`.
    pub fn gc(&self) -> std::io::Result<CacheRemoval> {
        remove_children(&self.root, &[&self.tmp])
    }

    /// Removes every clone, snapshot and temporary file. The cache directory
    /// itself stays. Symlinks inside are removed, never followed.
    ///
    /// Example: with one clone and one snapshot, `removed` lists
    /// `repos/github.com` and `snapshots/<hex>.db`.
    pub fn clear(&self) -> std::io::Result<CacheRemoval> {
        remove_children(&self.root, &[&self.repos, &self.snapshots, &self.tmp])
    }
}

/// Collects directories under `dir` that contain `.git`, without descending
/// into them or following symlinks.
///
/// Example: `repos/github.com/a/b/.git` gives `CachedRepo { path: "github.com/a/b", .. }`.
fn find_clones(base: &Path, dir: &Path, out: &mut Vec<CachedRepo>) {
    if dir.join(".git").symlink_metadata().is_ok() {
        let path = dir
            .strip_prefix(base)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        out.push(CachedRepo {
            path,
            bytes: size(dir),
        });
        return;
    }
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            find_clones(base, &entry.path(), out);
        }
    }
}

/// Total size of the files under `path`, not following symlinks.
///
/// Example: a directory with two 10-byte files gives 20.
fn size(path: &Path) -> u64 {
    let Ok(meta) = path.symlink_metadata() else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| size(&e.path()))
        .sum()
}

/// Removes the children of each directory in `dirs`, reporting them relative
/// to `root`, sorted.
///
/// Example: `remove_children(root, &[tmp])` with `tmp/a` gives `removed: ["tmp/a"]`.
fn remove_children(root: &Path, dirs: &[&Path]) -> std::io::Result<CacheRemoval> {
    let mut removal = CacheRemoval {
        removed: Vec::new(),
        bytes: 0,
    };
    for dir in dirs {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            removal.bytes += size(&path);
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            removal
                .removed
                .push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    removal.removed.sort();
    Ok(removal)
}

/// Checks that `dir` exists or can be created and that a file can be written
/// in it. Writes and removes one probe file; touches nothing else.
///
/// Example: `check(Some(Path::new("/tmp/c/aldente")))` creates the directory
/// and gives `CacheCheck { writable: true, detail: None, .. }`; `check(None)`
/// gives `writable: false` with a detail.
pub fn check(dir: Option<&Path>) -> CacheCheck {
    let Some(dir) = dir else {
        return CacheCheck {
            path: None,
            writable: false,
            detail: Some("this platform reports no user cache directory".to_owned()),
        };
    };
    let detail = probe(dir).err();
    CacheCheck {
        path: Some(dir.to_string_lossy().into_owned()),
        writable: detail.is_none(),
        detail,
    }
}

/// Creates `dir` if needed, then creates and removes a probe file in it.
///
/// Example: a writable `dir` gives `Ok(())`; a `dir` whose parent is a file
/// gives `Err("cannot create …: …")`.
fn probe(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let file = dir.join(format!(".write-probe-{}", std::process::id()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)
        .map_err(|e| format!("cannot write in {}: {e}", dir.display()))?;
    fs::remove_file(&file).map_err(|e| format!("cannot remove {}: {e}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch directory under the system temp dir, unique per test.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aldente-cache-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_the_directory_and_leaves_it_empty() {
        let root = scratch("ok");
        let dir = root.join("nested").join("aldente");
        let result = check(Some(&dir));
        assert!(result.writable, "{result:?}");
        assert_eq!(result.detail, None);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_file_in_the_way_is_not_writable() {
        let root = scratch("blocked");
        let blocker = root.join("file");
        fs::write(&blocker, "x").unwrap();
        let result = check(Some(&blocker.join("aldente")));
        assert!(!result.writable);
        assert!(result.detail.unwrap().starts_with("cannot create"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn layout_lists_clears_and_collects() {
        let root = scratch("layout");
        let layout = Layout::open(&root).unwrap();
        let clone = layout.repos.join("github.com").join("a").join("b");
        fs::create_dir_all(clone.join(".git")).unwrap();
        fs::write(clone.join("x.clj"), "12345").unwrap();
        fs::write(layout.snapshots.join("00.db"), "123").unwrap();
        fs::write(layout.tmp.join("leftover"), "1").unwrap();

        let report = layout.list();
        assert_eq!(
            report.repos,
            [CachedRepo {
                path: "github.com/a/b".into(),
                bytes: 5
            }]
        );
        assert_eq!(
            report.snapshots,
            [CachedSnapshot {
                key: "b3:00".into(),
                bytes: 3
            }]
        );
        assert_eq!(report.bytes, 9);

        let gc = layout.gc().unwrap();
        assert_eq!(gc.removed, ["tmp/leftover"]);
        assert!(clone.exists());

        let cleared = layout.clear().unwrap();
        assert_eq!(cleared.removed, ["repos/github.com", "snapshots/00.db"]);
        assert_eq!(cleared.bytes, 8);
        assert!(layout.repos.exists() && fs::read_dir(&layout.repos).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn clear_removes_symlinks_without_following_them() {
        let root = scratch("clear-symlink");
        let outside = scratch("clear-outside");
        fs::write(outside.join("keep"), "x").unwrap();
        let layout = Layout::open(&root).unwrap();
        std::os::unix::fs::symlink(&outside, layout.tmp.join("link")).unwrap();
        layout.clear().unwrap();
        assert!(outside.join("keep").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn no_cache_dir_is_reported() {
        let result = check(None);
        assert!(!result.writable);
        assert_eq!(result.path, None);
    }
}
