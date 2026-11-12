//! Resolving a local path to the repository root that will be analyzed.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// The directory to analyze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Canonical absolute path of the root.
    pub root: PathBuf,
    /// Whether a `.git` was found; false means `root` is the given directory itself.
    pub git: bool,
}

/// Why a path cannot be analyzed.
#[derive(Debug, Error)]
pub enum TargetError {
    #[error("{path}: {source}")]
    Unreadable {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}: not a directory")]
    NotADirectory(String),
}

/// Resolves `path` to its repository root: the nearest ancestor (or itself)
/// containing `.git` (a directory, or a file for worktrees and submodules).
/// Without one, the directory itself is the root, with `git: false`.
///
/// Example: `resolve("~/proj/src/app")` with `~/proj/.git` present gives
/// `Target { root: "/home/u/proj", git: true }`.
pub fn resolve(path: &Path) -> Result<Target, TargetError> {
    let canonical = path
        .canonicalize()
        .map_err(|source| TargetError::Unreadable {
            path: path.display().to_string(),
            source,
        })?;
    if !canonical.is_dir() {
        return Err(TargetError::NotADirectory(path.display().to_string()));
    }
    let git_root = canonical
        .ancestors()
        .find(|dir| dir.join(".git").symlink_metadata().is_ok());
    Ok(match git_root {
        Some(root) => Target {
            root: root.to_path_buf(),
            git: true,
        },
        None => Target {
            root: canonical,
            git: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aldente-target-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn finds_the_nearest_git_directory() {
        let repo = scratch("git");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src/app")).unwrap();
        let target = resolve(&repo.join("src/app")).unwrap();
        assert_eq!(
            target,
            Target {
                root: repo.clone(),
                git: true
            }
        );
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn a_git_file_counts_too() {
        let repo = scratch("worktree");
        fs::write(repo.join(".git"), "gitdir: elsewhere").unwrap();
        assert!(resolve(&repo).unwrap().git);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn without_git_the_directory_itself_is_the_root() {
        // The system temp dir is assumed not to be inside a git repository.
        let dir = scratch("plain");
        let target = resolve(&dir).unwrap();
        assert_eq!(
            target,
            Target {
                root: dir.clone(),
                git: false
            }
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn files_and_missing_paths_are_errors() {
        let dir = scratch("errors");
        fs::write(dir.join("f"), "").unwrap();
        assert!(matches!(
            resolve(&dir.join("f")),
            Err(TargetError::NotADirectory(_))
        ));
        assert!(matches!(
            resolve(&dir.join("nope")),
            Err(TargetError::Unreadable { .. })
        ));
        fs::remove_dir_all(dir).unwrap();
    }
}
