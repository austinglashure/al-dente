//! The `[TARGET] [--ref]` arguments shared by analysis commands: a local
//! path, or a URL cloned with the hardened git into the cache.

use std::path::PathBuf;

use aldente_acquire::cache::{Layout, default_dir};
use aldente_acquire::remote::{AcquireError, EmptyDirs, Git, acquire, looks_like_url, parse_url};
use aldente_acquire::target::resolve;
use aldente_acquire::tools;
use aldente_model::{Diagnostic, ErrorCode};
use clap::Args;

/// Where to analyze.
#[derive(Debug, Args)]
pub struct TargetArgs {
    /// A local directory (its git repository root is used) or a git URL
    /// (https://, ssh:// or git@host:path), cloned into the cache.
    #[arg(default_value = ".")]
    pub target: String,
    /// Branch, tag or commit to check out (URL targets only).
    #[arg(long = "ref", value_name = "REV")]
    pub git_ref: Option<String>,
}

/// An opened target.
#[derive(Debug)]
pub struct Opened {
    /// Canonical root directory.
    pub root: PathBuf,
    pub git: bool,
    /// Problems found while acquiring (e.g. submodules not fetched).
    pub diagnostics: Vec<Diagnostic>,
}

/// Why a target could not be opened.
#[derive(Debug)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

impl Failure {
    /// Builds a failure.
    ///
    /// Example: `Failure::new(ErrorCode::Usage, "bad")`.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Failure {
        Failure {
            code,
            message: message.into(),
        }
    }
}

/// Opens the target: clones or updates a URL in the cache, or resolves a
/// local path to its repository root.
///
/// Example: `open(&TargetArgs { target: ".".into(), git_ref: None })` gives
/// the current repository's root; `target: "file:///x"` gives a usage failure.
pub fn open(args: &TargetArgs) -> Result<Opened, Failure> {
    if looks_like_url(&args.target) || args.target.starts_with('-') {
        return open_remote(&args.target, args.git_ref.as_deref());
    }
    if args.git_ref.is_some() {
        return Err(Failure::new(
            ErrorCode::Usage,
            "--ref applies to URL targets only",
        ));
    }
    let target =
        resolve(args.target.as_ref()).map_err(|e| Failure::new(ErrorCode::Usage, e.to_string()))?;
    if !target.git {
        eprintln!("aldente: no git repository found; using the target directory as the root");
    }
    Ok(Opened {
        root: target.root,
        git: target.git,
        diagnostics: Vec::new(),
    })
}

/// Validates the URL (before anything runs), then clones it.
///
/// Example: `open_remote("https://github.com/metosin/reitit", None)` clones into
/// `<cache>/repos/github.com/metosin/reitit` and opens it.
fn open_remote(url: &str, git_ref: Option<&str>) -> Result<Opened, Failure> {
    let url = parse_url(url).map_err(|e| Failure::new(ErrorCode::Usage, e.to_string()))?;
    if let Some(r) = git_ref {
        aldente_acquire::remote::check_ref(r)
            .map_err(|e| Failure::new(ErrorCode::Usage, e.to_string()))?;
    }
    let os = std::env::consts::OS;
    let git_path = tools::locate("git", std::env::var_os("PATH").as_deref()).ok_or_else(|| {
        Failure::new(ErrorCode::ToolMissing, tools::install_hint(&tools::GIT, os))
    })?;
    let internal = |m: String| Failure::new(ErrorCode::Internal, m);
    let cache = default_dir()
        .ok_or_else(|| internal("no user cache directory; set ALDENTE_CACHE_DIR".into()))?;
    let layout =
        Layout::open(&cache).map_err(|e| internal(format!("cannot create the cache: {e}")))?;
    let empty = EmptyDirs::prepare(&layout.tmp)
        .map_err(|e| internal(format!("cannot prepare git: {e}")))?;
    eprintln!("aldente: fetching {}", url.url);
    let acquired =
        acquire(&Git::new(git_path, empty), &url, git_ref, &layout.repos).map_err(|e| match e {
            AcquireError::Url(u) => Failure::new(ErrorCode::Usage, u.to_string()),
            other => internal(other.to_string()),
        })?;
    let root = acquired
        .dir
        .canonicalize()
        .map_err(|e| internal(format!("cannot open the clone: {e}")))?;
    Ok(Opened {
        root,
        git: true,
        diagnostics: acquired.diagnostics,
    })
}
