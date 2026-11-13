//! Commit history for churn (`09-network-and-coverage.md`, "Churn and
//! co-change"): a hardened `git log` with per-file line counts and renames,
//! and the changed line ranges of recent commits. Read-only: `log` with
//! `--no-ext-diff --no-textconv` through the hardened command builder, so a
//! hostile `.git/config` cannot run a diff driver, text conversion or pager
//! (`07-safety.md`, "Acquisition").

use std::path::Path;
use std::time::Duration;

use crate::local_git::HistoryError;
use crate::process::run_command;
use crate::remote::Git;

/// How long one `git log` may take.
const TIMEOUT: Duration = Duration::from_secs(300);

/// Marks the start of a commit in our `--format`.
const COMMIT: char = '\u{1}';

/// One file changed in one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Path after the commit.
    pub path: String,
    /// Path before, when the commit renamed the file.
    pub old_path: Option<String>,
    /// Lines added and deleted; `None` for binary files.
    pub added: Option<u64>,
    pub deleted: Option<u64>,
}

/// One commit with its file changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitStat {
    pub id: String,
    /// Committer time, seconds since the epoch.
    pub time: i64,
    pub files: Vec<FileChange>,
}

/// Lines a commit touched in one file, as `(start, count)` ranges on the
/// new side (`count` 0: lines were only removed, after `start`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHunks {
    pub path: String,
    pub ranges: Vec<(u32, u32)>,
}

/// One commit's changed line ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitHunks {
    pub id: String,
    pub files: Vec<FileHunks>,
}

/// Which commits to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogOptions {
    pub max_commits: usize,
    /// `YYYY-MM-DD`; commits before it are skipped.
    pub since: Option<String>,
}

/// Rejects a `--since` value that is not a plain `YYYY-MM-DD` date.
///
/// Example: `check_date("2026-01-31")` is ok; `check_date("--all")` is an error.
pub fn check_date(d: &str) -> Result<(), HistoryError> {
    let b = d.as_bytes();
    let ok = b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(HistoryError::BadRev(d.to_owned()))
    }
}

/// Whether the repository at `root` is a shallow clone (only part of its
/// history is present).
///
/// Example: a `--depth 1` clone is shallow.
pub fn is_shallow(root: &Path) -> bool {
    root.join(".git").join("shallow").is_file()
}

/// The common `git log` arguments: no pager, no external diff or text
/// conversion, no colour, renames detected, merges skipped.
///
/// Example: `log_args(&opts, &["--numstat", "-z"])`.
fn log_args(opts: &LogOptions, extra: &[&str]) -> Result<Vec<String>, HistoryError> {
    let mut args: Vec<String> = [
        "--no-pager",
        "log",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--no-merges",
        "-M",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.push(format!("--max-count={}", opts.max_commits));
    if let Some(since) = &opts.since {
        check_date(since)?;
        args.push(format!("--since={since}"));
    }
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    args.push("HEAD".to_owned());
    args.push("--".to_owned());
    Ok(args)
}

/// Runs `git log` with `args` and returns its output.
///
/// Example: see [`numstat`].
fn run_log(git: &Git, root: &Path, args: &[String]) -> Result<String, HistoryError> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_command(git.command(root, &refs), TIMEOUT).map_err(|e| HistoryError::Git {
        step: "log",
        message: e.to_string(),
    })?;
    if !out.status.success() {
        return Err(HistoryError::Git {
            step: "log",
            message: out.stderr.trim().chars().take(2000).collect(),
        });
    }
    Ok(out.stdout)
}

/// Commits reachable from HEAD, newest first, with per-file counts.
///
/// Example: in a repository with two commits gives two `CommitStat`s.
pub fn numstat(git: &Git, root: &Path, opts: &LogOptions) -> Result<Vec<CommitStat>, HistoryError> {
    let args = log_args(opts, &["--numstat", "-z", "--format=%x01%H %ct"])?;
    Ok(parse_numstat(&run_log(git, root, &args)?))
}

/// The changed line ranges of the newest `opts.max_commits` commits.
///
/// Example: a commit that adds line 3 of `a.clj` gives `ranges: [(3, 1)]`.
pub fn hunks(git: &Git, root: &Path, opts: &LogOptions) -> Result<Vec<CommitHunks>, HistoryError> {
    let args = log_args(opts, &["-p", "-U0", "--no-prefix", "--format=%x01%H"])?;
    Ok(parse_hunks(&run_log(git, root, &args)?))
}

/// Parses `git log --numstat -z --format=%x01%H %ct`. Each commit starts
/// with `\x01<id> <time>`; each file is `added\tdeleted\tpath` or, for a
/// rename, `added\tdeleted\t` followed by the old and new paths as their
/// own fields. Binary files have `-` counts.
///
/// Example: `"\x01abc 10\0\n1\t0\ta.clj\0"` gives one commit changing `a.clj`.
pub fn parse_numstat(text: &str) -> Vec<CommitStat> {
    let mut commits: Vec<CommitStat> = Vec::new();
    let mut fields = text.split('\0');
    while let Some(raw) = fields.next() {
        let field = raw.trim_start_matches('\n');
        if let Some(header) = field.strip_prefix(COMMIT) {
            let (id, time) = header.split_once(' ').unwrap_or((header, "0"));
            commits.push(CommitStat {
                id: id.to_owned(),
                time: time.trim().parse().unwrap_or(0),
                files: Vec::new(),
            });
            continue;
        }
        let mut parts = field.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (path, old_path) = if path.is_empty() {
            let old = fields.next().unwrap_or_default().to_owned();
            let new = fields.next().unwrap_or_default().to_owned();
            (new, Some(old))
        } else {
            (path.to_owned(), None)
        };
        if let Some(c) = commits.last_mut() {
            c.files.push(FileChange {
                path,
                old_path,
                added: a.parse().ok(),
                deleted: d.parse().ok(),
            });
        }
    }
    commits
}

/// Where the hunk parser is in a commit's patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Between `diff --git` and the first `@@`.
    Header,
    /// Inside a hunk: `+`, `-` and `\` lines are content.
    Content,
}

/// Parses `git log -p -U0 --no-prefix --format=%x01%H`, keeping only file
/// names (from `+++` lines in a file header) and the new-side ranges of
/// `@@` hunk headers. Content lines are never read as headers, so an added
/// line that starts with `+++` cannot fake a file name.
///
/// Example: a patch adding line 3 of `b.clj` gives `[("b.clj", [(3, 1)])]`.
pub fn parse_hunks(text: &str) -> Vec<CommitHunks> {
    let mut commits: Vec<CommitHunks> = Vec::new();
    let mut state = State::Header;
    // Whether hunks belong to the last file pushed (not to a deleted file).
    let mut current = false;
    for line in text.lines() {
        if let Some(id) = line.strip_prefix(COMMIT) {
            commits.push(CommitHunks {
                id: id.trim().to_owned(),
                files: Vec::new(),
            });
            state = State::Header;
            continue;
        }
        let Some(commit) = commits.last_mut() else {
            continue;
        };
        if state == State::Content && matches!(line.as_bytes().first(), Some(b'+' | b'-' | b'\\')) {
            continue;
        }
        if line.starts_with("diff --git ") {
            state = State::Header;
            current = false;
            continue;
        }
        if let Some(range) = line.strip_prefix("@@ ") {
            state = State::Content;
            if let (true, Some(r), Some(file)) =
                (current, new_range(range), commit.files.last_mut())
            {
                file.ranges.push(r);
            }
            continue;
        }
        if state == State::Header
            && let Some(name) = line.strip_prefix("+++ ")
        {
            let name = name.strip_suffix('\t').unwrap_or(name);
            current = name != "/dev/null";
            if current {
                commit.files.push(FileHunks {
                    path: unquote(name),
                    ranges: Vec::new(),
                });
            }
        }
    }
    commits
}

/// The new-side `(start, count)` of a hunk header's text after `@@ `.
///
/// Example: `new_range("-2,0 +3 @@ b")` is `Some((3, 1))`; `"-5 +4,0 @@"` gives `(4, 0)`.
fn new_range(rest: &str) -> Option<(u32, u32)> {
    let plus = rest.split(' ').find_map(|t| t.strip_prefix('+'))?;
    let (start, count) = match plus.split_once(',') {
        Some((s, c)) => (s.parse().ok()?, c.parse().ok()?),
        None => (plus.parse().ok()?, 1),
    };
    Some((start, count))
}

/// Undoes git's C-style quoting of unusual path names (`"a\303\251.clj"`).
/// Unquoted names come back unchanged.
///
/// Example: `unquote("\"a\\tb\"")` is `"a\tb"`.
pub fn unquote(name: &str) -> String {
    let Some(inner) = name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) else {
        return name.to_owned();
    };
    let mut out: Vec<u8> = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let c = bytes[i + 1];
        let simple = match c {
            b'n' => Some(b'\n'),
            b't' => Some(b'\t'),
            b'r' => Some(b'\r'),
            b'a' => Some(7),
            b'b' => Some(8),
            b'f' => Some(12),
            b'v' => Some(11),
            b'"' => Some(b'"'),
            b'\\' => Some(b'\\'),
            _ => None,
        };
        if let Some(b) = simple {
            out.push(b);
            i += 2;
        } else if i + 3 < bytes.len()
            && bytes[i + 1..i + 4]
                .iter()
                .all(|d| (b'0'..=b'7').contains(d))
        {
            let v = bytes[i + 1..i + 4]
                .iter()
                .fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
            out.push(v as u8);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_parses_counts_renames_and_binaries() {
        let text = "\u{1}c3 30\0\n-\t-\tbin.dat\0\u{1}c2 20\0\n1\t0\t\0a.clj\0b.clj\0\
                    1\t0\tsp ace.clj\0\u{1}c1 10\0\n2\t0\ta.clj\0";
        let c = parse_numstat(text);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].id.as_str(), c[0].time), ("c3", 30));
        assert_eq!(c[0].files[0].added, None);
        assert_eq!(c[1].files[0].path, "b.clj");
        assert_eq!(c[1].files[0].old_path.as_deref(), Some("a.clj"));
        assert_eq!(c[1].files[1].path, "sp ace.clj");
        assert_eq!(
            (c[2].files[0].added, c[2].files[0].deleted),
            (Some(2), Some(0))
        );
    }

    #[test]
    fn hunks_read_headers_only() {
        let text = "\u{1}c2\n\ndiff --git a.clj b.clj\nsimilarity index 66%\nrename from a.clj\n\
                    rename to b.clj\n--- a.clj\n+++ b.clj\n@@ -2,0 +3 @@ b\n+++ fake.clj\n\
                    @@ -9,2 +9,0 @@\n-x\n-y\ndiff --git sp ace.clj sp ace.clj\n--- sp ace.clj\t\n\
                    +++ sp ace.clj\t\n@@ -1,0 +2 @@ x\n+y\ndiff --git gone.clj gone.clj\n\
                    --- gone.clj\n+++ /dev/null\n@@ -1 +0,0 @@\n-z\n\u{1}c1\n";
        let c = parse_hunks(text);
        assert_eq!(c.len(), 2);
        assert_eq!(
            c[0].files,
            vec![
                FileHunks {
                    path: "b.clj".into(),
                    ranges: vec![(3, 1), (9, 0)]
                },
                FileHunks {
                    path: "sp ace.clj".into(),
                    ranges: vec![(2, 1)]
                },
            ]
        );
        assert!(c[1].files.is_empty());
    }

    fn plain_git(cwd: &Path, args: &[&str]) {
        let empty = cwd.join("..").join("empty-gitconfig");
        std::fs::write(&empty, "").unwrap();
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(["-c", "commit.gpgsign=false"])
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

    /// A scripted repository with a rename, read with real git; its own
    /// `.git/config` sets a diff driver, text conversion and pager that
    /// must not run (hostile case 6a for `churn`).
    #[test]
    fn reads_a_scripted_history_without_running_repo_config() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("aldente-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let base = base.canonicalize().unwrap();
        let repo = repo.canonicalize().unwrap();
        plain_git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(".gitattributes"), "* diff=evil\n").unwrap();
        fs::write(repo.join("a.clj"), "(ns a)\n(defn f [])\n").unwrap();
        plain_git(&repo, &["add", "."]);
        plain_git(&repo, &["commit", "-q", "-m", "one"]);
        plain_git(&repo, &["mv", "a.clj", "b.clj"]);
        fs::write(repo.join("b.clj"), "(ns a)\n(defn f [])\n(defn g [])\n").unwrap();
        plain_git(&repo, &["add", "-A"]);
        plain_git(&repo, &["commit", "-q", "-m", "two"]);
        let marker = base.join("ran").display().to_string().replace('\\', "/");
        let touch = format!("touch '{marker}'");
        let mut config = fs::read_to_string(repo.join(".git/config")).unwrap();
        config.push_str(&format!(
            "[core]\n\tpager = {touch}\n[diff]\n\texternal = {touch}\n[diff \"evil\"]\n\ttextconv = {touch}\n"
        ));
        fs::write(repo.join(".git/config"), config).unwrap();

        let git = Git::new(
            "git".into(),
            crate::remote::EmptyDirs::prepare(&base.join("tmp")).unwrap(),
        );
        let opts = LogOptions {
            max_commits: 10,
            since: None,
        };
        let stats = numstat(&git, &repo, &opts).unwrap();
        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].files[0].path, "b.clj");
        assert_eq!(stats[0].files[0].old_path.as_deref(), Some("a.clj"));
        assert!(stats[1].files.iter().any(|f| f.path == "a.clj"));
        let h = hunks(&git, &repo, &opts).unwrap();
        assert_eq!(h[0].files[0].path, "b.clj");
        assert_eq!(h[0].files[0].ranges, vec![(3, 1)]);
        assert!(!is_shallow(&repo));
        let since = LogOptions {
            max_commits: 10,
            since: Some("2099-01-01".into()),
        };
        assert!(numstat(&git, &repo, &since).unwrap().is_empty());
        assert!(
            !base.join("ran").exists(),
            "a command from the repository's config ran"
        );
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn quoting_and_dates() {
        assert_eq!(unquote("plain.clj"), "plain.clj");
        assert_eq!(unquote("\"a\\tb\""), "a\tb");
        assert_eq!(unquote("\"caf\\303\\251.clj\""), "café.clj");
        assert!(check_date("2026-01-31").is_ok());
        for bad in ["--all", "2026-1-31", "2026-01-31x", "yesterday"] {
            assert!(check_date(bad).is_err(), "{bad}");
        }
        assert_eq!(new_range("-5 +4,0 @@"), Some((4, 0)));
    }
}
