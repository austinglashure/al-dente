//! Opening a target and getting its snapshot (from the cache or by analyzing
//! it). Every command that needs the graph goes through [`load`].

use aldente_acquire::cache::{Layout, default_dir};
use aldente_acquire::key::KeyInputs;
use aldente_acquire::local_git::head_rev;
use aldente_acquire::remote::{EmptyDirs, Git};
use aldente_acquire::tools::{self, CLJ_KONDO};
use aldente_adapter::AdapterError;
use aldente_clj::adapter::{CljAdapter, VERSION as CLJ_VERSION};
use aldente_clj::kondo::Kondo;
use aldente_graph::pipeline::{CacheContext, Outcome, analyze_cached};
use aldente_graph::snapshot::SCHEMA_VERSION;
use aldente_model::doctor::ToolStatus;
use aldente_model::timing::Timings;
use aldente_model::{ErrorCode, SnapshotInfo, Warning, WarningCode};

use crate::commands::target::{Failure, Opened, TargetArgs, open};

/// A target with its snapshot.
#[derive(Debug)]
pub struct Loaded {
    pub target: Opened,
    pub outcome: Outcome,
}

impl Loaded {
    /// The envelope's `snapshot` section.
    ///
    /// Example: `SnapshotInfo { key: "b3:…", repo: "/home/u/proj", rev: Some("1d2f…"), dirty: false, analyzer: "0.1.0" }`.
    pub fn info(&self) -> SnapshotInfo {
        let s = &self.outcome.snapshot;
        SnapshotInfo {
            key: s.key.clone(),
            repo: s.repo.clone(),
            rev: s.rev.clone(),
            dirty: false,
            analyzer: s.analyzer.clone(),
        }
    }

    /// Warnings for the envelope (a snapshot that could not be saved).
    ///
    /// Example: an unwritable cache gives one warning; normally none.
    pub fn warnings(&self) -> Vec<Warning> {
        self.outcome
            .store_error
            .iter()
            .map(|e| Warning {
                code: WarningCode::NotCached,
                message: format!("the snapshot could not be saved: {e}"),
            })
            .collect()
    }
}

/// The local Maven repository for jar listings: `ALDENTE_MAVEN_REPO` when
/// set (empty turns jar listing off), else `~/.m2/repository`.
///
/// Example: with `ALDENTE_MAVEN_REPO=""` gives `None`.
fn maven_repo() -> Option<std::path::PathBuf> {
    match std::env::var_os("ALDENTE_MAVEN_REPO") {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v.into()),
        None => aldente_acquire::maven::default_repository(),
    }
}

/// Opens the target, checks clj-kondo (missing or too old: exit 5 with the
/// doctor hint), and returns the cached snapshot or a fresh analysis.
///
/// Example: `load(&args, false)` on clj-mini analyzes once, then hits the cache.
pub fn load(args: &TargetArgs, no_cache: bool) -> Result<Loaded, Failure> {
    let started = std::time::Instant::now();
    let loaded = load_timed(args, no_cache, started);
    if args.timings
        && let Ok(l) = &loaded
    {
        eprintln!("timings:\n{}", l.outcome.timings.report());
        eprintln!(
            "  (wall clock including target and tool checks: {:.3}s)",
            started.elapsed().as_secs_f64()
        );
    }
    loaded
}

/// [`load`] without the report; `open` and the tool checks are timed as
/// `open-target` and `tool-checks`.
///
/// Example: see [`load`].
fn load_timed(
    args: &TargetArgs,
    no_cache: bool,
    started: std::time::Instant,
) -> Result<Loaded, Failure> {
    let target = open(args)?;
    let opened = started.elapsed();
    let os = std::env::consts::OS;
    let path_var = std::env::var_os("PATH");
    let kondo = tools::check(&CLJ_KONDO, path_var.as_deref(), os);
    let (Some(kondo_path), ToolStatus::Ok) = (kondo.path.clone(), kondo.status) else {
        let hint = kondo
            .hint
            .unwrap_or_else(|| tools::install_hint(&CLJ_KONDO, os));
        return Err(Failure::new(
            ErrorCode::ToolMissing,
            format!("clj-kondo is not usable ({:?}); {hint}", kondo.status),
        ));
    };
    let internal = |m: String| Failure::new(ErrorCode::Internal, m);
    let cache = default_dir()
        .ok_or_else(|| internal("no user cache directory; set ALDENTE_CACHE_DIR".into()))?;
    let layout =
        Layout::open(&cache).map_err(|e| internal(format!("cannot create the cache: {e}")))?;
    let rev = if target.git {
        tools::locate("git", path_var.as_deref()).and_then(|git_path| {
            let empty = EmptyDirs::prepare(&layout.tmp).ok()?;
            head_rev(&Git::new(git_path, empty), &target.root)
        })
    } else {
        None
    };
    let maven = maven_repo();
    let ctx = CacheContext {
        snapshots: &layout.snapshots,
        inputs: KeyInputs {
            analyzer: env!("CARGO_PKG_VERSION").to_owned(),
            schema: SCHEMA_VERSION.to_owned(),
            adapters: vec![("clj".to_owned(), CLJ_VERSION.to_owned())],
            clj_kondo: kondo.version,
            // Which Maven cache library attribution read (R10). Its contents
            // are not in the key: see PROGRESS.md, WP2.9.
            config: maven
                .as_ref()
                .map(|p| format!("maven={}", p.display()))
                .unwrap_or_default(),
        },
        repo: target.origin.clone(),
        rev,
        no_cache,
    };
    let adapter = CljAdapter::new(Kondo::new(
        kondo_path.into(),
        layout.tmp.join("kondo-empty"),
    ))
    .with_maven_repo(maven);
    let checks = started.elapsed().saturating_sub(opened);
    let progress = crate::progress::StderrProgress::start();
    let mut outcome = analyze_cached(&target.root, target.git, &[&adapter], &ctx, &progress)
        .map_err(|e| match e {
            AdapterError::ToolMissing { tool, hint } => {
                Failure::new(ErrorCode::ToolMissing, format!("{tool} is missing; {hint}"))
            }
            AdapterError::Failed(m) => internal(m),
        })?;
    let mut timings = Timings::default();
    timings.record("open-target", opened);
    timings.record("tool-checks", checks);
    for (label, took) in outcome.timings.entries() {
        timings.record(label, *took);
    }
    outcome.timings = timings;
    if outcome.cached {
        eprintln!("aldente: using cached snapshot {}", outcome.snapshot.key);
    } else {
        progress.finish();
    }
    if let Some(e) = &outcome.store_error {
        eprintln!("aldente: warning: the snapshot could not be saved: {e}");
    }
    Ok(Loaded { target, outcome })
}
