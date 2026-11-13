//! The full static pipeline for a local root: walk, discover units, run the
//! adapters that claim each unit, build the graph.

use std::path::Path;

use aldente_acquire::key::{KeyInputs, snapshot_key};
use aldente_acquire::units::{Discovery, discover};
use aldente_acquire::walk::{Limits, walk};
use aldente_adapter::{Adapter, AdapterError, AnalyzeCtx, FactBatch};
use aldente_model::timing::Timings;
use aldente_model::{BlindSpot, BlindSpotCategory, Diagnostic, DiagnosticCode, RepoPath, Site};
use rayon::prelude::*;

use crate::build::{Graph, build_timed};
use crate::snapshot::{self, Snapshot};

/// A point in the pipeline worth telling the user about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<'a> {
    /// Walking the tree and hashing files.
    Walking,
    /// The walk is done.
    Discovered { files: usize, units: usize },
    /// An unchanged tree: the stored snapshot is used.
    CacheHit,
    /// Starting the adapters: `jobs` (unit, adapter) pairs.
    Analyzing { jobs: usize },
    /// An adapter started a stage on a unit (`clj-kondo`, `resolve`, …).
    UnitStage {
        unit: &'a str,
        files: usize,
        stage: &'a str,
    },
    /// A unit finished; `done` of `total`.
    UnitDone {
        unit: &'a str,
        done: usize,
        total: usize,
    },
    /// Merging facts into the graph.
    Building,
    /// Writing the snapshot.
    Saving,
}

/// Receives pipeline steps. Called from worker threads.
pub trait Progress: Sync {
    /// Reports one step.
    ///
    /// Example: `progress.step(Step::Walking)`.
    fn step(&self, step: Step<'_>);
}

/// Progress that reports nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Silent;

impl Progress for Silent {
    fn step(&self, _: Step<'_>) {}
}

/// Discovery plus the graph.
#[derive(Debug, Clone)]
pub struct Analyzed {
    pub discovery: Discovery,
    pub graph: Graph,
}

/// Runs the pipeline on `root` (canonical). A missing tool stops it
/// (`AdapterError::ToolMissing`); any other adapter failure is recorded as an
/// `analyzer-failed` diagnostic plus an `unparsed-file` blind spot per file
/// of that unit, and the other units continue.
///
/// Example: `analyze(clj_mini, false, &[&clj])` gives a graph with
/// `clj:.//shop.core/checkout`.
pub fn analyze(
    root: &Path,
    git: bool,
    adapters: &[&dyn Adapter],
) -> Result<Analyzed, AdapterError> {
    let discovery = discover(root, git, walk(root, &Limits::default()));
    analyze_discovered(root, discovery, adapters, &mut Timings::default(), &Silent)
}

/// Runs the adapters over an existing discovery and builds the graph,
/// recording each adapter's stages (per unit) and the build in `timings`.
///
/// Example: see [`analyze`].
fn analyze_discovered(
    root: &Path,
    discovery: Discovery,
    adapters: &[&dyn Adapter],
    timings: &mut Timings,
    progress: &dyn Progress,
) -> Result<Analyzed, AdapterError> {
    // One job per (unit, claiming adapter), run in parallel; results are
    // collected in unit order, so the graph does not depend on scheduling.
    let mut jobs: Vec<(&aldente_model::UnitInfo, &dyn Adapter, Vec<RepoPath>)> = Vec::new();
    for unit in &discovery.report.units {
        let files: Vec<RepoPath> = discovery
            .analyzed
            .iter()
            .filter(|f| f.unit == unit.dir)
            .map(|f| f.path.clone())
            .collect();
        if files.is_empty() {
            continue;
        }
        for adapter in adapters.iter().filter(|a| a.claims(unit)) {
            jobs.push((unit, *adapter, files.clone()));
        }
    }
    let started = std::time::Instant::now();
    progress.step(Step::Analyzing { jobs: jobs.len() });
    let done = std::sync::atomic::AtomicUsize::new(0);
    let total = jobs.len();
    let results: Vec<Result<FactBatch, AdapterError>> = jobs
        .par_iter()
        .map(|(unit, adapter, files)| {
            let name = unit.dir.as_str();
            let stage = |stage: &str| {
                progress.step(Step::UnitStage {
                    unit: name,
                    files: files.len(),
                    stage,
                })
            };
            let result = adapter.analyze(
                unit,
                AnalyzeCtx {
                    root,
                    files,
                    stage: &stage,
                },
            );
            let done = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            progress.step(Step::UnitDone {
                unit: name,
                done,
                total,
            });
            result
        })
        .collect();
    timings.detail("units (in parallel, wall clock)", started.elapsed());
    let mut batches = Vec::new();
    for ((unit, adapter, files), result) in jobs.iter().zip(results) {
        match result {
            Ok(mut batch) => {
                timings.merge(
                    &format!("unit:{}", unit.dir),
                    std::mem::take(&mut batch.timings),
                );
                batches.push(batch);
            }
            Err(e @ AdapterError::ToolMissing { .. }) => return Err(e),
            Err(AdapterError::Failed(message)) => {
                batches.push(failed(&unit.dir, files, adapter.id(), &message))
            }
        }
    }
    let started = std::time::Instant::now();
    progress.step(Step::Building);
    let mut details = Timings::default();
    let mut graph = build_timed(&discovery.report.units, batches, &mut details);
    timings.record("build", started.elapsed());
    for (label, took) in details.entries() {
        timings.record(label, *took);
    }
    graph
        .diagnostics
        .extend(discovery.report.diagnostics.iter().cloned());
    graph.diagnostics.sort();
    graph.diagnostics.dedup();
    Ok(Analyzed { discovery, graph })
}

/// What the cache needs besides the files: where snapshots live, the
/// non-file key inputs, and the snapshot's identity.
#[derive(Debug, Clone)]
pub struct CacheContext<'a> {
    /// The `snapshots` directory.
    pub snapshots: &'a Path,
    pub inputs: KeyInputs,
    /// The URL or canonical local path.
    pub repo: String,
    pub rev: Option<String>,
    /// Analyze even when a snapshot exists (`--no-cache`).
    pub no_cache: bool,
}

/// A snapshot and whether it came from the cache.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub snapshot: Snapshot,
    pub cached: bool,
    /// Set when the fresh snapshot could not be saved; the result is still good.
    pub store_error: Option<String>,
    /// Wall time per stage (`--timings`).
    pub timings: Timings,
}

/// Resolves the target to its key first (walk and hashes only); a hit loads
/// the snapshot without running any adapter, a miss analyzes and saves.
///
/// Example: the second call on an unchanged tree returns `cached: true` and
/// runs no adapter.
pub fn analyze_cached(
    root: &Path,
    git: bool,
    adapters: &[&dyn Adapter],
    ctx: &CacheContext<'_>,
    progress: &dyn Progress,
) -> Result<Outcome, AdapterError> {
    let mut timings = Timings::default();
    progress.step(Step::Walking);
    let walked = timings.time("walk", || walk(root, &Limits::default()));
    let discovery = timings.time("discover", || discover(root, git, walked));
    progress.step(Step::Discovered {
        files: discovery.analyzed.len(),
        units: discovery.report.units.len(),
    });
    let key = timings.time("key", || snapshot_key(&ctx.inputs, &discovery.key_files()));
    let path = snapshot::path_for(ctx.snapshots, &key);
    if !ctx.no_cache {
        let loaded = timings.time("snapshot-lookup", || snapshot::load(&path));
        if let Ok(Some(snap)) = loaded
            && snap.key == key
        {
            progress.step(Step::CacheHit);
            return Ok(Outcome {
                snapshot: snap,
                cached: true,
                store_error: None,
                timings,
            });
        }
    }
    let units = discovery.report.units.clone();
    let files = discovery
        .analyzed
        .iter()
        .map(|f| (f.path.clone(), f.hash.clone()))
        .collect();
    let analyzed = analyze_discovered(root, discovery, adapters, &mut timings, progress)?;
    let snap = Snapshot {
        key,
        repo: ctx.repo.clone(),
        rev: ctx.rev.clone(),
        git,
        analyzer: ctx.inputs.analyzer.clone(),
        units,
        files,
        graph: analyzed.graph,
    };
    progress.step(Step::Saving);
    let store_error = timings
        .time("snapshot-save", || snapshot::write(&path, &snap))
        .err()
        .map(|e| e.to_string());
    Ok(Outcome {
        snapshot: snap,
        cached: false,
        store_error,
        timings,
    })
}

/// The facts for a unit whose adapter failed: a diagnostic and one
/// `unparsed-file` blind spot per file.
///
/// Example: a clj-kondo timeout on a 3-file unit gives 1 diagnostic and 3 blind spots.
fn failed(unit: &RepoPath, files: &[RepoPath], adapter: &str, message: &str) -> FactBatch {
    FactBatch {
        diagnostics: vec![Diagnostic::new(
            Some(unit.clone()),
            DiagnosticCode::AnalyzerFailed,
            format!("{adapter} adapter failed: {message}"),
        )],
        blindspots: files
            .iter()
            .map(|f| BlindSpot {
                category: BlindSpotCategory::UnparsedFile,
                node: None,
                site: Some(Site {
                    file: f.clone(),
                    line: 1,
                    col: 1,
                }),
                message: format!("not analyzed: the {adapter} adapter failed for this unit"),
            })
            .collect(),
        ..FactBatch::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::UnitInfo;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts how often it is asked to analyze; returns no facts.
    struct Counting(AtomicUsize);

    impl Adapter for Counting {
        fn id(&self) -> &'static str {
            "count"
        }
        fn claims(&self, _: &UnitInfo) -> bool {
            true
        }
        fn analyze(&self, _: &UnitInfo, _: AnalyzeCtx<'_>) -> Result<FactBatch, AdapterError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let mut batch = FactBatch::default();
            batch
                .timings
                .record("fake-stage", std::time::Duration::from_millis(1));
            Ok(batch)
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aldente-pipeline-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("repo/src")).unwrap();
        std::fs::create_dir_all(dir.join("snapshots")).unwrap();
        std::fs::write(dir.join("repo/deps.edn"), "{}").unwrap();
        std::fs::write(dir.join("repo/src/a.clj"), "(ns a)").unwrap();
        dir.canonicalize().unwrap()
    }

    fn ctx(snapshots: &Path, no_cache: bool) -> CacheContext<'_> {
        CacheContext {
            snapshots,
            inputs: KeyInputs {
                analyzer: "test".into(),
                schema: snapshot::SCHEMA_VERSION.into(),
                adapters: vec![("count".into(), "1".into())],
                clj_kondo: None,
                config: String::new(),
            },
            repo: "/repo".into(),
            rev: None,
            no_cache,
        }
    }

    #[test]
    fn an_unchanged_tree_is_a_cache_hit_with_no_analysis_work() {
        let dir = scratch("hit");
        let (root, snaps) = (dir.join("repo"), dir.join("snapshots"));
        let counter = Counting(AtomicUsize::new(0));
        let first =
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert!(!first.cached);
        assert_eq!(first.store_error, None);
        let started = std::time::Instant::now();
        let second =
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert!(second.cached);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            1,
            "the hit ran no adapter"
        );
        assert_eq!(second.snapshot, first.snapshot);
    }

    #[test]
    fn changing_one_file_changes_the_key() {
        let dir = scratch("change");
        let (root, snaps) = (dir.join("repo"), dir.join("snapshots"));
        let counter = Counting(AtomicUsize::new(0));
        let first =
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        std::fs::write(root.join("src/a.clj"), "(ns a) (defn f [])").unwrap();
        let second =
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert_ne!(first.snapshot.key, second.snapshot.key);
        assert!(!second.cached);
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn no_cache_and_old_schemas_force_analysis() {
        let dir = scratch("force");
        let (root, snaps) = (dir.join("repo"), dir.join("snapshots"));
        let counter = Counting(AtomicUsize::new(0));
        let first =
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert!(
            !analyze_cached(&root, false, &[&counter], &ctx(&snaps, true), &Silent)
                .unwrap()
                .cached
        );
        let path = snapshot::path_for(&snaps, &first.snapshot.key);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE meta SET value = '0' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        let rebuilt =
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert!(!rebuilt.cached, "an older schema is ignored and rebuilt");
        assert_eq!(counter.0.load(Ordering::SeqCst), 3);
        assert!(
            analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent)
                .unwrap()
                .cached,
            "and the rebuild is cached"
        );
    }

    /// Records steps as text.
    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<String>>);

    impl Progress for Recorder {
        fn step(&self, step: Step<'_>) {
            let text = match step {
                Step::Walking => "walking".to_owned(),
                Step::Discovered { files, units } => format!("discovered {files} {units}"),
                Step::CacheHit => "hit".to_owned(),
                Step::Analyzing { jobs } => format!("analyzing {jobs}"),
                Step::UnitStage { .. } => "stage".to_owned(),
                Step::UnitDone { done, total, .. } => format!("done {done}/{total}"),
                Step::Building => "building".to_owned(),
                Step::Saving => "saving".to_owned(),
            };
            self.0.lock().unwrap().push(text);
        }
    }

    #[test]
    fn progress_reports_each_step_and_a_hit() {
        let dir = scratch("progress");
        let (root, snaps) = (dir.join("repo"), dir.join("snapshots"));
        let counter = Counting(AtomicUsize::new(0));
        let rec = Recorder::default();
        analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &rec).unwrap();
        let steps = rec.0.lock().unwrap().clone();
        assert_eq!(
            steps,
            [
                "walking",
                "discovered 1 1",
                "analyzing 1",
                "done 1/1",
                "building",
                "saving"
            ]
        );
        let rec = Recorder::default();
        analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &rec).unwrap();
        assert_eq!(
            rec.0.lock().unwrap().as_slice(),
            ["walking", "discovered 1 1", "hit"]
        );
    }

    #[test]
    fn timings_cover_each_stage_and_a_hit_only_loads() {
        let dir = scratch("timings");
        let (root, snaps) = (dir.join("repo"), dir.join("snapshots"));
        let counter = Counting(AtomicUsize::new(0));
        // Stages exactly; detail lines (`· …`) only checked for presence.
        let labels = |o: &Outcome| -> Vec<String> {
            o.timings
                .entries()
                .iter()
                .map(|(l, _)| l.clone())
                .filter(|l| !l.starts_with(aldente_model::timing::DETAIL))
                .collect()
        };
        let details = |o: &Outcome| -> Vec<String> {
            o.timings
                .entries()
                .iter()
                .map(|(l, _)| l.clone())
                .filter(|l| l.starts_with(aldente_model::timing::DETAIL))
                .collect()
        };
        let cold = analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert_eq!(
            labels(&cold),
            [
                "walk",
                "discover",
                "key",
                "snapshot-lookup",
                "fake-stage unit:.",
                "build",
                "snapshot-save"
            ]
        );
        let d = details(&cold);
        for want in [
            "· units (in parallel, wall clock)",
            "· build:merge",
            "· build:link",
        ] {
            assert!(d.iter().any(|l| l == want), "{want} in {d:?}");
        }
        let hit = analyze_cached(&root, false, &[&counter], &ctx(&snaps, false), &Silent).unwrap();
        assert_eq!(labels(&hit), ["walk", "discover", "key", "snapshot-lookup"]);
    }

    /// Sleeps, then returns a node named after the unit.
    struct Slow;

    impl Adapter for Slow {
        fn id(&self) -> &'static str {
            "slow"
        }
        fn claims(&self, _: &UnitInfo) -> bool {
            true
        }
        fn analyze(&self, unit: &UnitInfo, _: AnalyzeCtx<'_>) -> Result<FactBatch, AdapterError> {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let mut batch = FactBatch::default();
            batch
                .timings
                .record("slow", std::time::Duration::from_millis(300));
            batch.diagnostics.push(Diagnostic::new(
                Some(unit.dir.clone()),
                DiagnosticCode::AnalyzerFailed,
                format!("ran {}", unit.dir),
            ));
            Ok(batch)
        }
    }

    #[test]
    fn units_are_analyzed_in_parallel_and_merged_in_unit_order() {
        let dir = scratch("parallel");
        let root = dir.join("repo");
        for u in ["a", "b", "c", "d"] {
            std::fs::create_dir_all(root.join(u).join("src")).unwrap();
            std::fs::write(root.join(u).join("deps.edn"), "{}").unwrap();
            std::fs::write(root.join(u).join("src/x.clj"), "(ns x)").unwrap();
        }
        let started = std::time::Instant::now();
        let a = analyze(&root, false, &[&Slow]).unwrap();
        let took = started.elapsed();
        assert!(
            took < std::time::Duration::from_millis(1000),
            "took {took:?}"
        );
        let ran: Vec<String> = a
            .graph
            .diagnostics
            .iter()
            .filter(|d| d.message.starts_with("ran "))
            .map(|d| d.message.clone())
            .collect();
        assert_eq!(ran, ["ran .", "ran a", "ran b", "ran c", "ran d"]);
    }
}
