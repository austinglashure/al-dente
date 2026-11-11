//! The Al Dente cache directory: `<user cache dir>/aldente/`.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use aldente_model::doctor::CacheCheck;

/// `<user cache dir>/aldente`, or `None` if the platform reports no cache
/// directory (e.g. `HOME` unset on Linux).
///
/// Example: on Linux with `XDG_CACHE_HOME=/tmp/c`, gives `Some("/tmp/c/aldente")`.
pub fn default_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("aldente"))
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
    fn no_cache_dir_is_reported() {
        let result = check(None);
        assert!(!result.writable);
        assert_eq!(result.path, None);
    }
}
