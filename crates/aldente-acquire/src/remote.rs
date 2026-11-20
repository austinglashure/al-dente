//! Remote targets: URL validation and the hardened clone from
//! `docs/plan/07-safety.md` ("Acquisition").

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use aldente_model::{Diagnostic, DiagnosticCode, PathStyle, RepoPath};
use thiserror::Error;

use crate::process::{RunError, run_command};

/// A validated remote repository URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUrl {
    /// The URL exactly as git will receive it.
    pub url: String,
    /// Host, with `_<port>` appended when a port is given.
    pub host: String,
    /// Path segments after the host, `.git` stripped from the last.
    pub segments: Vec<String>,
}

/// Why a URL or ref was refused. Raised before any process starts.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UrlError {
    #[error("URL must not start with `-`")]
    LeadingDash,
    #[error("unsupported URL scheme (use https://, ssh:// or git@host:path)")]
    Scheme,
    #[error("URL must not contain a password")]
    Credentials,
    #[error("URL contains whitespace or control characters")]
    BadCharacter,
    #[error("URL needs a host and at least an owner and a name")]
    Shape,
    #[error("URL path segment {0:?} is not allowed")]
    Segment(String),
    #[error("ref {0:?} is not allowed")]
    Ref(String),
}

/// Whether `target` should be treated as a URL rather than a local path.
///
/// Example: `looks_like_url("https://github.com/a/b")` and
/// `looks_like_url("git@github.com:a/b")` are `true`; `looks_like_url("./src")` is `false`.
pub fn looks_like_url(target: &str) -> bool {
    if target.contains("://") || target.contains("::") {
        return true;
    }
    // scp-like `user@host:path`: an `@` before the first `:`, no slash before the `@`.
    match (target.find('@'), target.find(':'), target.find(['/', '\\'])) {
        (Some(at), Some(colon), slash) => at < colon && slash.is_none_or(|s| s > at),
        _ => false,
    }
}

/// Validates a URL: `https://host/owner/name`, `ssh://[user@]host[:port]/owner/name`
/// or `git@host:owner/name`. Everything else, including `file://`, `ext::`,
/// anything starting with `-` and embedded passwords, is refused.
///
/// Example: `parse_url("https://github.com/metosin/reitit.git")` gives host
/// `github.com`, segments `["metosin", "reitit"]`.
pub fn parse_url(url: &str) -> Result<RemoteUrl, UrlError> {
    if url.starts_with('-') {
        return Err(UrlError::LeadingDash);
    }
    let scp_like = !url.contains("://") && !url.contains("::") && looks_like_url(url);
    if !(url.starts_with("https://") || url.starts_with("ssh://") || scp_like) {
        return Err(UrlError::Scheme);
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(UrlError::BadCharacter);
    }
    let (authority, path, allow_user) = if let Some(rest) = url.strip_prefix("https://") {
        let (authority, path) = rest.split_once('/').ok_or(UrlError::Shape)?;
        (authority, path, false)
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/').ok_or(UrlError::Shape)?;
        (authority, path, true)
    } else if scp_like {
        let (authority, path) = url.split_once(':').ok_or(UrlError::Shape)?;
        // scp-like form: no port, so the authority is `user@host`.
        (authority, path, true)
    } else {
        return Err(UrlError::Scheme);
    };
    let host_port = match authority.rsplit_once('@') {
        Some((user, _)) if user.contains(':') || !allow_user => return Err(UrlError::Credentials),
        Some((user, host)) => {
            check_segment(user)?;
            host
        }
        None => authority,
    };
    let host = match host_port.split_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            check_segment(host)?;
            format!("{host}_{port}")
        }
        Some(_) => return Err(UrlError::Segment(host_port.to_owned())),
        None => {
            check_segment(host_port)?;
            host_port.to_owned()
        }
    };
    let mut segments: Vec<String> = Vec::new();
    for segment in path.trim_end_matches('/').split('/') {
        if segment.is_empty() {
            return Err(UrlError::Shape);
        }
        check_segment(segment)?;
        segments.push(segment.to_owned());
    }
    if let Some(last) = segments.last_mut()
        && let Some(stripped) = last.strip_suffix(".git")
    {
        check_segment(stripped)?;
        *last = stripped.to_owned();
    }
    if segments.len() < 2 {
        return Err(UrlError::Shape);
    }
    Ok(RemoteUrl {
        url: url.to_owned(),
        host,
        segments,
    })
}

/// A host, user or path segment: ASCII letters, digits, `.`, `_`, `-`; not
/// empty, `.`, `..`, or starting with `-`. These become directory names.
///
/// Example: `check_segment("reitit")` is ok; `check_segment("..")` and
/// `check_segment("%2e")` are errors.
fn check_segment(segment: &str) -> Result<(), UrlError> {
    let ok = !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.starts_with('-')
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(UrlError::Segment(segment.to_owned()))
    }
}

/// Validates a branch, tag or commit name given with `--ref`.
///
/// Example: `check_ref("v1.2.0")` is ok; `check_ref("--upload-pack=x")` is an error.
pub fn check_ref(r: &str) -> Result<(), UrlError> {
    let bad = r.is_empty()
        || r.starts_with('-')
        || r.ends_with('/')
        || r.ends_with(".lock")
        || r.contains("..")
        || r.chars()
            .any(|c| c.is_whitespace() || c.is_control() || ":~^?*[\\".contains(c));
    if bad {
        Err(UrlError::Ref(r.to_owned()))
    } else {
        Ok(())
    }
}

/// Empty files and directories git is pointed at so that no user or system
/// configuration, template or hook applies.
#[derive(Debug, Clone)]
pub struct EmptyDirs {
    /// An empty file used as global and system config.
    pub config: PathBuf,
    /// An empty `--template` directory.
    pub template: PathBuf,
    /// An empty `core.hooksPath`.
    pub hooks: PathBuf,
}

impl EmptyDirs {
    /// Creates (or re-empties) them under `tmp`.
    ///
    /// Example: `EmptyDirs::prepare(cache.join("tmp"))` creates
    /// `tmp/git-empty/{config,template/,hooks/}`.
    pub fn prepare(tmp: &Path) -> std::io::Result<EmptyDirs> {
        let base = tmp.join("git-empty");
        let dirs = EmptyDirs {
            config: base.join("config"),
            template: base.join("template"),
            hooks: base.join("hooks"),
        };
        for dir in [&dirs.template, &dirs.hooks] {
            if dir.exists() {
                fs::remove_dir_all(dir)?;
            }
            fs::create_dir_all(dir)?;
        }
        fs::write(&dirs.config, "")?;
        Ok(dirs)
    }
}

/// Runs git with the hardening from `07-safety.md`. Every git process Al Dente
/// starts on a remote target goes through [`Git::command`].
#[derive(Debug, Clone)]
pub struct Git {
    program: PathBuf,
    empty: EmptyDirs,
    /// Tests only: extra environment applied before the hardening (to show it
    /// is overridden) and whether local `file` transport is allowed.
    test_env: Vec<(OsString, OsString)>,
    allow_file: bool,
    /// How long a fetch may take.
    fetch_timeout: Duration,
}

/// Config passed with `-c` on every call.
const HARDENING: &[&str] = &[
    "submodule.recurse=false",
    "filter.lfs.smudge=",
    "filter.lfs.process=",
    "filter.lfs.required=false",
    "core.symlinks=false",
    "protocol.allow=never",
    "protocol.https.allow=always",
    "protocol.ssh.allow=always",
    "core.fsmonitor=false",
    "core.autocrlf=false",
    "core.longpaths=true",
    "advice.detachedHead=false",
];

/// How long a fetch may take unless [`Git::with_fetch_timeout`] says
/// otherwise; other git calls get [`STEP_TIMEOUT`].
pub const DEFAULT_FETCH_TIMEOUT: Duration = Duration::from_secs(600);
const STEP_TIMEOUT: Duration = Duration::from_secs(60);

impl Git {
    /// A hardened git at `program`.
    ///
    /// Example: `Git::new(which("git"), EmptyDirs::prepare(tmp)?)`.
    pub fn new(program: PathBuf, empty: EmptyDirs) -> Git {
        Git {
            program,
            empty,
            test_env: Vec::new(),
            allow_file: false,
            fetch_timeout: DEFAULT_FETCH_TIMEOUT,
        }
    }

    /// The same git with another fetch time limit.
    ///
    /// Example: `Git::new(path, empty).with_fetch_timeout(Duration::from_secs(3600))`
    /// lets a fetch run for an hour.
    pub fn with_fetch_timeout(mut self, timeout: Duration) -> Git {
        self.fetch_timeout = timeout;
        self
    }

    /// A git command in `cwd`: inherited `GIT_*` variables removed, config
    /// and templates pointed at empty files, prompts off, and the `-c`
    /// hardening before `args`.
    ///
    /// Example: `command(dir, &["fetch", "--depth", "1", "--", url, "HEAD"])`.
    pub fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(&key);
            }
        }
        for (key, value) in &self.test_env {
            cmd.env(key, value);
        }
        cmd.env("GIT_CONFIG_GLOBAL", &self.empty.config)
            .env("GIT_CONFIG_SYSTEM", &self.empty.config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_LFS_SKIP_SMUDGE", "1")
            .env("GIT_PAGER", "cat")
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS")
            .current_dir(cwd);
        let hooks = format!("core.hooksPath={}", self.empty.hooks.display());
        for setting in HARDENING.iter().copied().chain([hooks.as_str()]) {
            cmd.args(["-c", setting]);
        }
        if self.allow_file {
            cmd.args(["-c", "protocol.file.allow=always"]);
        }
        cmd.args(args);
        cmd
    }

    /// Runs one step, mapping failures to [`AcquireError::Git`].
    ///
    /// Example: `run(dir, &["init", "-q"], "init", STEP_TIMEOUT)`.
    fn run(
        &self,
        cwd: &Path,
        args: &[&str],
        step: &str,
        timeout: Duration,
    ) -> Result<(), AcquireError> {
        let result = run_command(self.command(cwd, args), timeout);
        finish(step, result, |t| AcquireError::Git {
            step: step.to_owned(),
            message: format!("timed out after {t:?}"),
        })
    }

    /// Runs a fetch from `url` into the clone at `cwd` within the fetch time
    /// limit; running out of time is [`AcquireError::FetchTimeout`], and what
    /// the killed git left behind is removed at once rather than at the next
    /// fetch. Call only while holding the clone's [`EntryLock`].
    ///
    /// Example: `fetch(dir, &["fetch", "-q", "--depth", "1", "--", url, "HEAD"], url)`.
    fn fetch(&self, cwd: &Path, args: &[&str], url: &str) -> Result<(), AcquireError> {
        let result = run_command(self.command(cwd, args), self.fetch_timeout);
        if matches!(result, Err(RunError::Timeout(_))) {
            // Best effort: the timeout is the error to report, and the next
            // fetch clears anything left here anyway.
            let _ = clear_leftovers(&cwd.join(".git"));
        }
        finish("fetch", result, |after| AcquireError::FetchTimeout {
            url: url.to_owned(),
            after,
        })
    }
}

/// Turns a finished (or failed) git step into a result: a non-zero exit is
/// [`AcquireError::Git`] with git's stderr, a timeout is `on_timeout`.
///
/// Example: a `fetch` that exited 128 with "fatal: …" gives
/// `AcquireError::Git { step: "fetch", message: "fatal: …" }`.
fn finish(
    step: &str,
    result: Result<crate::process::Captured, RunError>,
    on_timeout: impl FnOnce(Duration) -> AcquireError,
) -> Result<(), AcquireError> {
    let out = result.map_err(|e| match e {
        RunError::Timeout(t) => on_timeout(t),
        other => AcquireError::Git {
            step: step.to_owned(),
            message: other.to_string(),
        },
    })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(AcquireError::Git {
            step: step.to_owned(),
            message: out.stderr.trim().chars().take(2000).collect(),
        })
    }
}

/// Why a remote target could not be acquired.
#[derive(Debug, Error)]
pub enum AcquireError {
    #[error(transparent)]
    Url(#[from] UrlError),
    #[error("cannot prepare {path}: {message}")]
    Io { path: String, message: String },
    #[error("git {step} failed: {message}")]
    Git { step: String, message: String },
    #[error(
        "fetching {url} did not finish in {} s: the repository is too big to fetch remotely \
         over this connection; clone it yourself and run Al Dente on the local clone",
        after.as_secs()
    )]
    FetchTimeout { url: String, after: Duration },
}

/// A checked-out remote target.
#[derive(Debug)]
pub struct Acquired {
    /// The clone's working tree.
    pub dir: PathBuf,
    /// Submodules, which are never fetched.
    pub diagnostics: Vec<Diagnostic>,
}

/// Where a URL is cloned: `<repos>/<host>/<segments…>`.
///
/// Example: `https://github.com/metosin/reitit` under `/c/repos` gives
/// `/c/repos/github.com/metosin/reitit`.
pub fn clone_dir(repos: &Path, url: &RemoteUrl) -> PathBuf {
    let mut dir = repos.join(&url.host);
    dir.extend(&url.segments);
    dir
}

/// Clones `url` (or updates an existing clone) at `ref` (default: the remote
/// `HEAD`) into its directory under `repos`, depth 1, with full hardening.
///
/// Example: `acquire(&git, &parse_url("https://github.com/metosin/reitit")?,
/// Some("0.7.0"), &repos)` checks out tag 0.7.0 into
/// `repos/github.com/metosin/reitit`.
pub fn acquire(
    git: &Git,
    url: &RemoteUrl,
    r#ref: Option<&str>,
    repos: &Path,
) -> Result<Acquired, AcquireError> {
    if let Some(r) = r#ref {
        check_ref(r)?;
    }
    fetch_into(git, &url.url, &clone_dir(repos, url), r#ref)
}

/// The clone steps for an already validated URL: `init` with an empty
/// template (first time only), `fetch --depth 1` of the ref, detached forced
/// checkout of `FETCH_HEAD`.
///
/// Example: `fetch_into(&git, "https://…", dir, None)` leaves the remote
/// `HEAD` checked out in `dir`.
fn fetch_into(
    git: &Git,
    url: &str,
    dir: &Path,
    r#ref: Option<&str>,
) -> Result<Acquired, AcquireError> {
    let io = |e: std::io::Error| AcquireError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    };
    let _held = EntryLock::acquire(dir).map_err(io)?;
    clear_leftovers(&dir.join(".git")).map_err(io)?;
    if !dir.join(".git").is_dir() {
        fs::create_dir_all(dir).map_err(io)?;
        let template = format!("--template={}", git.empty.template.display());
        git.run(dir, &["init", "-q", &template], "init", STEP_TIMEOUT)?;
    }
    let target = r#ref.unwrap_or("HEAD");
    git.fetch(
        dir,
        &[
            "fetch",
            "-q",
            "--depth",
            "1",
            "--no-tags",
            "--no-recurse-submodules",
            "--",
            url,
            target,
        ],
        url,
    )?;
    git.run(
        dir,
        &["checkout", "-q", "--detach", "--force", "FETCH_HEAD"],
        "checkout",
        STEP_TIMEOUT,
    )?;
    Ok(Acquired {
        dir: dir.to_path_buf(),
        diagnostics: submodules(dir),
    })
}

/// Fetches up to `commits` commits of history behind the checked-out
/// commit of a shallow clone (for `churn`, only when asked), with the same
/// hardening as the clone. Nothing is checked out.
///
/// Example: `deepen(&git, &dir, &url, 1000)` turns a `--depth 1` clone into
/// one with up to 1,000 commits.
pub fn deepen(git: &Git, dir: &Path, url: &RemoteUrl, commits: usize) -> Result<(), AcquireError> {
    deepen_from(git, dir, &url.url, commits)
}

/// [`deepen`] for an already validated URL.
///
/// Example: see [`deepen`].
fn deepen_from(git: &Git, dir: &Path, url: &str, commits: usize) -> Result<(), AcquireError> {
    let io = |e: std::io::Error| AcquireError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    };
    let _held = EntryLock::acquire(dir).map_err(io)?;
    clear_leftovers(&dir.join(".git")).map_err(io)?;
    let head = crate::local_git::head_rev(git, dir).ok_or_else(|| AcquireError::Git {
        step: "rev-parse".into(),
        message: "the clone has no HEAD".into(),
    })?;
    let depth = format!("--depth={}", commits.max(1));
    git.fetch(
        dir,
        &[
            "fetch",
            "-q",
            &depth,
            "--no-tags",
            "--no-recurse-submodules",
            "--",
            url,
            &head,
        ],
        url,
    )
}

/// Submodule paths from `.gitmodules`, as diagnostics. Read as plain text;
/// git's config parser is not run on it.
///
/// Example: a `.gitmodules` with `path = vendor/lib` gives one `submodule`
/// diagnostic at `vendor/lib`.
fn submodules(dir: &Path) -> Vec<Diagnostic> {
    let Ok(text) = fs::read_to_string(dir.join(".gitmodules")) else {
        return Vec::new();
    };
    let mut out: Vec<Diagnostic> = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "path").then(|| value.trim())
        })
        .map(|path| {
            let repo_path = RepoPath::from_native_str(path, PathStyle::Unix).ok();
            Diagnostic::new(
                repo_path,
                DiagnosticCode::Submodule,
                format!("submodule {path:?} is not fetched; its code is not analyzed"),
            )
        })
        .collect();
    out.sort();
    out
}

/// Exclusive hold on one clone in the cache, across Al Dente processes.
/// Released when dropped (the OS also releases it if the process dies, so a
/// crash cannot leave it stuck).
#[derive(Debug)]
pub struct EntryLock {
    _file: fs::File,
}

impl EntryLock {
    /// Blocks until this process holds the clone at `dir`. The lock is a file
    /// beside the clone, `<name>@aldente.lock`: outside the working tree, and
    /// `@` cannot appear in a URL segment, so no repository's directory can
    /// collide with it.
    ///
    /// Example: `EntryLock::acquire(Path::new("/c/repos/github.com/penpot/penpot"))`
    /// locks `/c/repos/github.com/penpot/penpot@aldente.lock`.
    pub fn acquire(dir: &Path) -> std::io::Result<EntryLock> {
        let name = dir.file_name().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "clone dir has no name")
        })?;
        let mut lock_name = name.to_os_string();
        lock_name.push("@aldente.lock");
        let parent = dir.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(parent.join(lock_name))?;
        file.lock()?;
        Ok(EntryLock { _file: file })
    }
}

/// Removes what a killed git (our timeout) leaves in `dot_git`: every
/// `*.lock` file, and the half-downloaded `tmp_*` files in `objects/pack`
/// (git ignores them, but each failed fetch of a large repository leaves tens
/// of megabytes). Only call while holding the clone's [`EntryLock`]: then no
/// git of ours is running in it, so these files are all leftovers. Symlinks
/// are neither followed nor removed.
///
/// Example: a `.git` holding `shallow.lock`, `refs/heads/x.lock` and
/// `objects/pack/tmp_pack_Ec99jR` has none of them afterwards; `HEAD`,
/// `config` and `objects/pack/pack-*.pack` are untouched.
fn clear_leftovers(dot_git: &Path) -> std::io::Result<()> {
    clear_leftovers_in(dot_git, &dot_git.join("objects").join("pack"))
}

/// [`clear_leftovers`] for the directory `dir`; `pack_dir` is where `tmp_*`
/// files are leftovers too.
///
/// Example: see [`clear_leftovers`].
fn clear_leftovers_in(dir: &Path, pack_dir: &Path) -> std::io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        // `DirEntry::file_type` does not follow symlinks, so a symlink is
        // neither a dir nor a file here and is left alone.
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() {
            clear_leftovers_in(&path, pack_dir)?;
        } else if kind.is_file() {
            let lock = path.extension().is_some_and(|e| e == "lock");
            let partial_pack =
                dir == pack_dir && entry.file_name().to_string_lossy().starts_with("tmp_");
            if lock || partial_pack {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn accepts_the_three_url_forms() {
        let u = parse_url("https://github.com/metosin/reitit.git").unwrap();
        assert_eq!(u.host, "github.com");
        assert_eq!(u.segments, ["metosin", "reitit"]);
        assert_eq!(u.url, "https://github.com/metosin/reitit.git");
        let u = parse_url("git@github.com:metosin/reitit").unwrap();
        assert_eq!((u.host.as_str(), u.segments.len()), ("github.com", 2));
        let u = parse_url("ssh://git@gitlab.example.com:2222/group/sub/repo.git").unwrap();
        assert_eq!(u.host, "gitlab.example.com_2222");
        assert_eq!(u.segments, ["group", "sub", "repo"]);
        assert!(parse_url("https://github.com/metosin/reitit/").is_ok());
    }

    #[test]
    fn refuses_dangerous_urls_before_running_anything() {
        let cases = [
            ("-oProxyCommand=touch x", UrlError::LeadingDash),
            ("--upload-pack=touch x", UrlError::LeadingDash),
            ("file:///etc/repo", UrlError::Scheme),
            ("ext::sh -c touch% x", UrlError::Scheme),
            ("http://github.com/a/b", UrlError::Scheme),
            ("git://github.com/a/b", UrlError::Scheme),
            ("/local/path", UrlError::Scheme),
            ("https://user:secret@github.com/a/b", UrlError::Credentials),
            ("https://github.com/a/b\nc", UrlError::BadCharacter),
            ("https://github.com/a b/c", UrlError::BadCharacter),
            ("https://github.com/onlyowner", UrlError::Shape),
            ("https://github.com/a/../b", UrlError::Segment("..".into())),
            (
                "https://github.com/a/%2e%2e",
                UrlError::Segment("%2e%2e".into()),
            ),
            ("git@-oProxy:a/b", UrlError::Segment("-oProxy".into())),
        ];
        for (url, expected) in cases {
            assert_eq!(parse_url(url), Err(expected), "{url}");
        }
    }

    #[test]
    fn url_detection() {
        for url in [
            "https://x/a/b",
            "ssh://x/a/b",
            "git@x:a/b",
            "file:///x",
            "ext::sh",
        ] {
            assert!(looks_like_url(url), "{url}");
        }
        for path in [".", "./src", "/home/u/proj", "C:\\proj", "relative/dir"] {
            assert!(!looks_like_url(path), "{path}");
        }
    }

    /// A scratch directory under the system temp dir.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aldente-remote-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    /// Plain git for building test repositories, isolated from the user's config.
    fn plain_git(cwd: &Path, args: &[&str], env: &[(OsString, OsString)]) -> std::process::Output {
        // `parent()`, not `join("..")`: on Windows `canonicalize` gives `\\?\` paths, where `..` is not resolved and git cannot open the file.
        let empty = cwd.parent().unwrap().join("empty-config");
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
            .envs(env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out
    }

    /// A bare repository whose content is hostile: a `.gitattributes` filter,
    /// a `.gitmodules` entry with a `file://` URL and a gitlink.
    fn hostile_bare(base: &Path) -> PathBuf {
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        plain_git(&src, &["init", "-q", "-b", "main"], &[]);
        fs::write(src.join("a.clj"), "(ns a)\n").unwrap();
        fs::write(src.join(".gitattributes"), "*.clj filter=evil\n").unwrap();
        fs::write(
            src.join(".gitmodules"),
            "[submodule \"sub\"]\n\tpath = vendor/sub\n\turl = file:///tmp/aldente-evil\n",
        )
        .unwrap();
        plain_git(&src, &["add", "."], &[]);
        plain_git(
            &src,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,1111111111111111111111111111111111111111,vendor/sub",
            ],
            &[],
        );
        plain_git(&src, &["commit", "-q", "-m", "one"], &[]);
        let bare = base.join("bare.git");
        plain_git(base, &["clone", "-q", "--bare", "src", "bare.git"], &[]);
        bare
    }

    /// Hostile environment: a template with a post-checkout hook and a global
    /// config defining the `evil` filter and a hooks path, each writing a
    /// marker file.
    fn hostile_env(base: &Path, markers: &Path) -> Vec<(OsString, OsString)> {
        let hooks = base.join("evil-template").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-checkout");
        let marker = markers
            .join("hook-ran")
            .display()
            .to_string()
            .replace('\\', "/");
        fs::write(&hook, format!("#!/bin/sh\ntouch '{marker}'\n")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let filter_marker = markers
            .join("filter-ran")
            .display()
            .to_string()
            .replace('\\', "/");
        let config = base.join("evil-gitconfig");
        fs::write(
            &config,
            format!(
                "[filter \"evil\"]\n\tsmudge = touch '{filter_marker}' && cat\n\trequired = true\n[core]\n\thooksPath = {}\n",
                hooks.display().to_string().replace('\\', "/")
            ),
        )
        .unwrap();
        vec![
            (
                "GIT_TEMPLATE_DIR".into(),
                base.join("evil-template").into_os_string(),
            ),
            ("GIT_CONFIG_GLOBAL".into(), config.into_os_string()),
        ]
    }

    fn test_git(base: &Path, env: Vec<(OsString, OsString)>) -> Git {
        let mut git = Git::new("git".into(), EmptyDirs::prepare(&base.join("tmp")).unwrap());
        git.test_env = env;
        git.allow_file = true;
        git
    }

    /// A depth-1 clone gains history on request, still with no hook or filter run.
    #[test]
    fn deepen_fetches_more_history() {
        let base = scratch("deepen");
        let bare = hostile_bare(&base);
        let src = base.join("src");
        for i in 0..3 {
            fs::write(src.join("a.clj"), format!("(ns a) ;; {i}\n")).unwrap();
            plain_git(&src, &["commit", "-q", "-am", "more"], &[]);
        }
        plain_git(&src, &["push", "-q", bare.to_str().unwrap(), "main"], &[]);
        let markers = base.join("markers");
        fs::create_dir_all(&markers).unwrap();
        let git = test_git(&base, hostile_env(&base, &markers));
        let dir = base.join("clone");
        fetch_into(&git, bare.to_str().unwrap(), &dir, None).unwrap();
        assert!(crate::history::is_shallow(&dir));
        let opts = crate::history::LogOptions {
            max_commits: 10,
            since: None,
        };
        assert_eq!(crate::history::numstat(&git, &dir, &opts).unwrap().len(), 1);
        deepen_from(&git, &dir, bare.to_str().unwrap(), 100).unwrap();
        assert_eq!(crate::history::numstat(&git, &dir, &opts).unwrap().len(), 4);
        assert!(!markers.join("hook-ran").exists() && !markers.join("filter-ran").exists());
        fs::remove_dir_all(base).unwrap();
    }

    /// Hostile case 6. The control clone shows the setup is live; the hardened
    /// clone runs neither hook nor filter and does not fetch the submodule.
    #[test]
    fn hostile_6_hardened_clone_runs_nothing() {
        let base = scratch("hostile6");
        let bare = hostile_bare(&base);

        let control_markers = base.join("control-markers");
        fs::create_dir_all(&control_markers).unwrap();
        let env = hostile_env(&base, &control_markers);
        let mut control = Command::new("git");
        control
            .args(["clone", "-q"])
            .arg(&bare)
            .arg(base.join("control"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .envs(env.iter().map(|(k, v)| (k, v)));
        let _ = control.output().unwrap();
        assert!(
            control_markers.join("filter-ran").exists()
                || control_markers.join("hook-ran").exists(),
            "control clone should trip the hostile setup"
        );

        let markers = base.join("markers");
        fs::create_dir_all(&markers).unwrap();
        let git = test_git(&base, hostile_env(&base, &markers));
        let dir = base.join("clone");
        let acquired = fetch_into(&git, bare.to_str().unwrap(), &dir, None).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.clj")).unwrap(), "(ns a)\n");
        assert!(!markers.join("hook-ran").exists(), "hook ran");
        assert!(!markers.join("filter-ran").exists(), "filter ran");
        let sub = dir.join("vendor").join("sub");
        assert!(
            !sub.exists() || fs::read_dir(&sub).unwrap().next().is_none(),
            "submodule fetched"
        );
        assert_eq!(acquired.diagnostics.len(), 1);
        assert_eq!(acquired.diagnostics[0].code, DiagnosticCode::Submodule);
        assert_eq!(
            acquired.diagnostics[0].path.as_ref().unwrap().as_str(),
            "vendor/sub"
        );
        assert!(
            !dir.join(".git")
                .join("hooks")
                .join("post-checkout")
                .exists()
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn an_existing_clone_is_updated_to_the_requested_ref() {
        let base = scratch("reuse");
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        plain_git(&src, &["init", "-q", "-b", "main"], &[]);
        fs::write(src.join("one.clj"), "1").unwrap();
        plain_git(&src, &["add", "."], &[]);
        plain_git(&src, &["commit", "-q", "-m", "one"], &[]);
        plain_git(&src, &["tag", "v1"], &[]);
        let first = String::from_utf8(plain_git(&src, &["rev-parse", "HEAD"], &[]).stdout).unwrap();
        fs::write(src.join("two.clj"), "2").unwrap();
        plain_git(&src, &["add", "."], &[]);
        plain_git(&src, &["commit", "-q", "-m", "two"], &[]);
        let url = src.to_str().unwrap();
        let git = test_git(&base, Vec::new());
        let dir = base.join("clone");

        fetch_into(&git, url, &dir, None).unwrap();
        assert!(dir.join("two.clj").exists());
        fetch_into(&git, url, &dir, Some("v1")).unwrap();
        assert!(!dir.join("two.clj").exists(), "checkout moved back to v1");
        fetch_into(&git, url, &dir, Some("main")).unwrap();
        assert!(dir.join("two.clj").exists());
        fetch_into(&git, url, &dir, Some(first.trim())).unwrap();
        assert!(!dir.join("two.clj").exists(), "a commit id works too");
        fs::remove_dir_all(base).unwrap();
    }

    /// Every file under `dir`, recursively.
    fn walk_files(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_files(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    /// A one-commit repository to fetch from, at `base/src`.
    fn simple_src(base: &Path) -> PathBuf {
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        plain_git(&src, &["init", "-q", "-b", "main"], &[]);
        fs::write(src.join("a.clj"), "1").unwrap();
        plain_git(&src, &["add", "."], &[]);
        plain_git(&src, &["commit", "-q", "-m", "one"], &[]);
        src
    }

    /// A git killed mid-fetch (our timeout) leaves its lock files behind;
    /// the next fetch must not fail on them.
    #[test]
    fn locks_left_by_a_killed_git_do_not_wedge_the_next_fetch() {
        let base = scratch("stale-locks");
        let src = simple_src(&base);
        let url = src.to_str().unwrap();
        let git = test_git(&base, Vec::new());
        let dir = base.join("clone");
        fetch_into(&git, url, &dir, None).unwrap();
        let dot_git = dir.join(".git");
        let stale = [
            dot_git.join("shallow.lock"),
            dot_git.join("index.lock"),
            dot_git.join("HEAD.lock"),
            dot_git.join("refs").join("heads").join("x.lock"),
        ];
        for lock in &stale {
            fs::create_dir_all(lock.parent().unwrap()).unwrap();
            fs::write(lock, "").unwrap();
        }
        fetch_into(&git, url, &dir, None).unwrap();
        for lock in &stale {
            assert!(!lock.exists(), "{} still there", lock.display());
        }
        fs::remove_dir_all(base).unwrap();
    }

    /// A killed fetch also leaves its half-downloaded pack; it is removed,
    /// and the real packs stay.
    #[test]
    fn partial_downloads_left_by_a_killed_git_are_removed() {
        let base = scratch("stale-packs");
        let src = simple_src(&base);
        let url = src.to_str().unwrap();
        let git = test_git(&base, Vec::new());
        let dir = base.join("clone");
        fetch_into(&git, url, &dir, None).unwrap();
        let pack_dir = dir.join(".git").join("objects").join("pack");
        fs::create_dir_all(&pack_dir).unwrap();
        let real = pack_dir.join("pack-0000.keep-me");
        fs::write(&real, "").unwrap();
        let partial = [
            pack_dir.join("tmp_pack_Ec99jR"),
            pack_dir.join("tmp_idx_Ec99jR"),
        ];
        for p in &partial {
            fs::write(p, "half a pack").unwrap();
        }
        fetch_into(&git, url, &dir, None).unwrap();
        for p in &partial {
            assert!(!p.exists(), "{} still there", p.display());
        }
        assert!(real.exists(), "a real pack file was removed");
        fs::remove_dir_all(base).unwrap();
    }

    /// A fetch that runs out of time says what to do instead.
    #[test]
    fn a_fetch_that_times_out_says_to_clone_it_locally() {
        let base = scratch("fetch-timeout");
        let src = simple_src(&base);
        let git = test_git(&base, Vec::new()).with_fetch_timeout(Duration::ZERO);
        let err = fetch_into(&git, src.to_str().unwrap(), &base.join("clone"), None).unwrap_err();
        assert!(matches!(err, AcquireError::FetchTimeout { .. }), "{err:?}");
        let message = err.to_string();
        assert!(message.contains("too big to fetch remotely"), "{message}");
        assert!(message.contains("clone it yourself"), "{message}");
        assert!(
            message.contains("run Al Dente on the local clone"),
            "{message}"
        );
        // Nothing the killed git started is left. (With a zero limit git
        // rarely gets far enough to leave anything; the penpot run on
        // 2026-11-18 is what showed the files cleared after a real timeout.)
        let leftovers: Vec<_> = walk_files(&base.join("clone").join(".git"))
            .into_iter()
            .filter(|p| {
                let name = p.file_name().unwrap_or_default().to_string_lossy();
                name.ends_with(".lock") || name.starts_with("tmp_")
            })
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        fs::remove_dir_all(base).unwrap();
    }

    /// While another Al Dente process holds a clone, a fetch waits for it
    /// instead of clearing locks that may belong to a live git.
    #[test]
    fn a_fetch_waits_for_another_holder_of_the_clone() {
        let base = scratch("entry-lock");
        let src = simple_src(&base);
        let url = src.to_str().unwrap().to_owned();
        let git = test_git(&base, Vec::new());
        let dir = base.join("clone");
        fetch_into(&git, &url, &dir, None).unwrap();
        let live = dir.join(".git").join("shallow.lock");

        let held = EntryLock::acquire(&dir).unwrap();
        fs::write(&live, "").unwrap();
        let (git2, dir2) = (git.clone(), dir.clone());
        let fetch = thread::spawn(move || fetch_into(&git2, &url, &dir2, None).map(|_| ()));
        thread::sleep(Duration::from_millis(300));
        assert!(!fetch.is_finished(), "fetch did not wait for the holder");
        assert!(live.exists(), "a live lock was removed");
        fs::remove_file(&live).unwrap();
        drop(held);
        fetch.join().unwrap().unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn the_file_transport_is_refused_outside_tests() {
        let base = scratch("nofile");
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        plain_git(&src, &["init", "-q", "-b", "main"], &[]);
        fs::write(src.join("a.clj"), "1").unwrap();
        plain_git(&src, &["add", "."], &[]);
        plain_git(&src, &["commit", "-q", "-m", "one"], &[]);
        let git = Git::new("git".into(), EmptyDirs::prepare(&base.join("tmp")).unwrap());
        let err = fetch_into(&git, src.to_str().unwrap(), &base.join("clone"), None).unwrap_err();
        assert!(
            matches!(err, AcquireError::Git { ref step, .. } if step == "fetch"),
            "{err}"
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn invalid_urls_never_start_git() {
        let base = scratch("nogit");
        let git = Git::new(
            "/nonexistent/git".into(),
            EmptyDirs::prepare(&base.join("tmp")).unwrap(),
        );
        let url = RemoteUrl {
            url: "x".into(),
            host: "h".into(),
            segments: vec!["a".into(), "b".into()],
        };
        let err = acquire(&git, &url, Some("--upload-pack=touch x"), &base).unwrap_err();
        assert!(matches!(err, AcquireError::Url(UrlError::Ref(_))), "{err}");
        assert!(!base.join("h").exists(), "nothing created");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn clone_dirs_follow_host_and_path() {
        let url = parse_url("https://github.com/metosin/reitit.git").unwrap();
        assert_eq!(
            clone_dir(Path::new("/c/repos"), &url),
            Path::new("/c/repos")
                .join("github.com")
                .join("metosin")
                .join("reitit")
        );
    }

    #[test]
    fn refs_are_checked() {
        for ok in [
            "main",
            "v1.2.0",
            "release/2024",
            "1d2fc67",
            "a".repeat(40).as_str(),
        ] {
            assert!(check_ref(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-x", "--upload-pack=x", "a b", "a\nb", "a..b", "a:b"] {
            assert!(check_ref(bad).is_err(), "{bad:?}");
        }
    }
}
