//! git on a local target. Only read-only plumbing runs, through the same
//! hardened command builder as remote clones, because a local repository's
//! own `.git/config` may be hostile (`07-safety.md`, "Acquisition").

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use thiserror::Error;

use crate::process::{run_command, run_command_with_input};
use crate::remote::Git;

/// How long one read-only git call may take.
const TIMEOUT: Duration = Duration::from_secs(120);

/// Why reading from the local repository's history failed.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HistoryError {
    #[error("not a usable revision: {0:?}")]
    BadRev(String),
    #[error("unknown revision: {0}")]
    UnknownRev(String),
    #[error("git {step} failed: {message}")]
    Git { step: &'static str, message: String },
}

/// The checked-out commit of the repository at `root`, or `None` (no commits,
/// not a repository, or git failed).
///
/// Example: in a repository whose HEAD is `1d2fc67…`, gives `Some("1d2fc67…")`.
pub fn head_rev(git: &Git, root: &Path) -> Option<String> {
    let cmd = git.command(
        root,
        &["--no-pager", "rev-parse", "--verify", "--quiet", "HEAD"],
    );
    let out = run_command(cmd, Duration::from_secs(30)).ok()?;
    let rev = out.stdout.trim();
    (out.status.success() && !rev.is_empty()).then(|| rev.to_owned())
}

/// Rejects revision text that could be read as an option or is not a single
/// plain token. `~`, `^` and `@{…}` are allowed (unlike `--ref`), since
/// `HEAD~1` and `main@{u}` are what people type here.
///
/// Example: `check_rev("HEAD~1")` is ok; `check_rev("--output=x")` is an error.
pub fn check_rev(rev: &str) -> Result<(), HistoryError> {
    let bad = rev.is_empty()
        || rev.starts_with('-')
        || rev.contains("..")
        || rev
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == ':');
    if bad {
        Err(HistoryError::BadRev(rev.to_owned()))
    } else {
        Ok(())
    }
}

/// The full commit ID `rev` names.
///
/// Example: `commit_of(git, root, "HEAD~1")` gives `Ok("1d2fc67…")`.
pub fn commit_of(git: &Git, root: &Path, rev: &str) -> Result<String, HistoryError> {
    check_rev(rev)?;
    let spec = format!("{rev}^{{commit}}");
    let cmd = git.command(
        root,
        &[
            "--no-pager",
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &spec,
        ],
    );
    let out = run_command(cmd, TIMEOUT).map_err(|e| HistoryError::Git {
        step: "rev-parse",
        message: e.to_string(),
    })?;
    let id = out.stdout.trim();
    if out.status.success() && !id.is_empty() {
        Ok(id.to_owned())
    } else {
        Err(HistoryError::UnknownRev(rev.to_owned()))
    }
}

/// Every file in `commit`'s tree as `(path, blob id)`, sorted by path.
/// Reads objects only (`ls-tree`), so no filter, hook or driver can run.
///
/// Example: for a commit holding `src/a.clj` gives `[("src/a.clj", "e69d…")]`.
pub fn tree_files(
    git: &Git,
    root: &Path,
    commit: &str,
) -> Result<Vec<(String, String)>, HistoryError> {
    check_rev(commit)?;
    let cmd = git.command(
        root,
        &[
            "--no-pager",
            "ls-tree",
            "-r",
            "-z",
            "--full-tree",
            "--end-of-options",
            commit,
        ],
    );
    let out = run_command(cmd, TIMEOUT).map_err(|e| HistoryError::Git {
        step: "ls-tree",
        message: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(HistoryError::Git {
            step: "ls-tree",
            message: out.stderr.trim().chars().take(2000).collect(),
        });
    }
    let mut files: Vec<(String, String)> = out
        .stdout
        .split('\0')
        .filter_map(|entry| {
            // `<mode> <type> <id>\t<path>`
            let (meta, path) = entry.split_once('\t')?;
            let mut parts = meta.split(' ');
            let (_mode, kind, id) = (parts.next()?, parts.next()?, parts.next()?);
            (kind == "blob").then(|| (path.to_owned(), id.to_owned()))
        })
        .collect();
    files.sort();
    Ok(files)
}

/// The contents of the blobs `ids`, read in one `cat-file --batch` call
/// (objects only: no filters or text conversion). Missing objects are left
/// out of the map.
///
/// Example: `read_blobs(git, root, &["e69d…"])` gives `{"e69d…": b"(ns a)\n"}`.
pub fn read_blobs(
    git: &Git,
    root: &Path,
    ids: &[String],
) -> Result<HashMap<String, Vec<u8>>, HistoryError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut input = Vec::new();
    for id in ids {
        if !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(HistoryError::BadRev(id.clone()));
        }
        input.extend_from_slice(id.as_bytes());
        input.push(b'\n');
    }
    let cmd = git.command(root, &["--no-pager", "cat-file", "--batch"]);
    let (status, out, err) =
        run_command_with_input(cmd, input, TIMEOUT).map_err(|e| HistoryError::Git {
            step: "cat-file",
            message: e.to_string(),
        })?;
    if !status.success() {
        return Err(HistoryError::Git {
            step: "cat-file",
            message: err.trim().chars().take(2000).collect(),
        });
    }
    Ok(parse_batch(&out))
}

/// Parses `cat-file --batch` output: per object a header
/// `<id> <type> <size>\n`, then `size` bytes and `\n`; or `<id> missing\n`.
///
/// Example: `b"ab blob 2\nhi\n"` gives `{"ab": b"hi"}`.
fn parse_batch(mut out: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut blobs = HashMap::new();
    while let Some(nl) = out.iter().position(|&b| b == b'\n') {
        let header = String::from_utf8_lossy(&out[..nl]).into_owned();
        out = &out[nl + 1..];
        let parts: Vec<&str> = header.split(' ').collect();
        let [id, _kind, size] = parts.as_slice() else {
            continue;
        };
        let Ok(size) = size.parse::<usize>() else {
            break;
        };
        if out.len() < size {
            break;
        }
        blobs.insert((*id).to_owned(), out[..size].to_vec());
        out = &out[(size + 1).min(out.len())..];
    }
    blobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::EmptyDirs;
    use std::fs;
    use std::process::Command;

    fn plain_git(cwd: &Path, args: &[&str]) {
        // `parent()`, not `join("..")`: on Windows `canonicalize` gives `\\?\` paths, where `..` is not resolved and git cannot open the file.
        let empty = cwd.parent().unwrap().join("empty-gitconfig");
        fs::write(&empty, "").unwrap();
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", &empty)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Hostile case 6a (the part WP1.6 needs): a repository whose own
    /// `.git/config` points `diff.external`, a textconv driver, `core.pager`,
    /// `core.fsmonitor` and a clean filter at a marker-writing command.
    /// Reading HEAD, the base commit, its tree and its blobs runs none of them.
    /// (`crater --diff` reads history this way instead of with `git diff`, which
    /// would run the clean filter on working-tree files.)
    #[test]
    fn hostile_6a_reading_head_runs_nothing_from_the_repo_config() {
        let base = std::env::temp_dir().join(format!("aldente-local-git-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let base = base.canonicalize().unwrap();
        let repo = repo.canonicalize().unwrap();
        plain_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(".gitattributes"), "* diff=evil filter=evil\n").unwrap();
        fs::write(repo.join("a.clj"), "(ns a)\n").unwrap();
        plain_git(&repo, &["add", "."]);
        plain_git(&repo, &["commit", "-q", "-m", "one"]);
        let marker = base.join("ran").display().to_string().replace('\\', "/");
        let touch = format!("touch '{marker}'");
        let mut config = fs::read_to_string(repo.join(".git/config")).unwrap();
        config.push_str(&format!(
            "[core]\n\tpager = {touch}\n\tfsmonitor = {touch}\n[diff]\n\texternal = {touch}\n\
             [diff \"evil\"]\n\ttextconv = {touch}\n[filter \"evil\"]\n\tclean = {touch}\n\tsmudge = {touch}\n"
        ));
        fs::write(repo.join(".git/config"), config).unwrap();
        fs::write(repo.join("a.clj"), "(ns a) changed\n").unwrap();

        let git = Git::new("git".into(), EmptyDirs::prepare(&base.join("tmp")).unwrap());
        let rev = head_rev(&git, &repo).expect("HEAD resolves");
        assert_eq!(rev.len(), 40);
        // What `crater --diff` reads: the base commit, its tree and blobs.
        assert_eq!(commit_of(&git, &repo, "HEAD").unwrap(), rev);
        let files = tree_files(&git, &repo, &rev).unwrap();
        let ids: Vec<String> = files.iter().map(|(_, id)| id.clone()).collect();
        let blobs = read_blobs(&git, &repo, &ids).unwrap();
        assert_eq!(blobs.len(), files.len());
        assert!(
            !base.join("ran").exists(),
            "a command from the repository's config ran"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn history_reads_commits_trees_and_blobs() {
        let base =
            std::env::temp_dir().join(format!("aldente-local-git-hist-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(repo.join("src")).unwrap();
        plain_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("src/a.clj"), "(ns a)\n").unwrap();
        plain_git(&repo, &["add", "."]);
        plain_git(&repo, &["commit", "-q", "-m", "one"]);
        fs::write(repo.join("src/a.clj"), "(ns a)\n(defn f [])\n").unwrap();
        fs::write(repo.join("b c.clj"), "(ns b)\n").unwrap();
        plain_git(&repo, &["add", "."]);
        plain_git(&repo, &["commit", "-q", "-m", "two"]);
        let git = Git::new("git".into(), EmptyDirs::prepare(&base.join("tmp")).unwrap());

        let first = commit_of(&git, &repo, "HEAD~1").unwrap();
        assert_eq!(first.len(), 40);
        assert!(matches!(
            commit_of(&git, &repo, "nope"),
            Err(HistoryError::UnknownRev(_))
        ));
        assert!(matches!(
            commit_of(&git, &repo, "--all"),
            Err(HistoryError::BadRev(_))
        ));
        assert!(check_rev("main@{u}").is_ok());
        assert!(check_rev("HEAD:src/a.clj").is_err());

        let old = tree_files(&git, &repo, &first).unwrap();
        assert_eq!(
            old.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["src/a.clj"]
        );
        let now = tree_files(&git, &repo, "HEAD").unwrap();
        assert_eq!(
            now.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
            ["b c.clj", "src/a.clj"]
        );
        let ids = vec![old[0].1.clone(), now[1].1.clone(), "0".repeat(40)];
        let blobs = read_blobs(&git, &repo, &ids).unwrap();
        assert_eq!(blobs[&ids[0]], b"(ns a)\n");
        assert_eq!(blobs[&ids[1]], b"(ns a)\n(defn f [])\n");
        assert!(!blobs.contains_key(&ids[2]), "missing objects are left out");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn no_repository_gives_none() {
        let dir =
            std::env::temp_dir().join(format!("aldente-local-git-none-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let git = Git::new("git".into(), EmptyDirs::prepare(&dir.join("tmp")).unwrap());
        assert_eq!(head_rev(&git, &dir), None);
        fs::remove_dir_all(&dir).unwrap();
    }
}
