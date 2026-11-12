//! The Clojure adapter: clj-kondo plus the form index, resolved by R1–R4.

use aldente_adapter::{Adapter, AdapterError, AnalyzeCtx, FactBatch};
use aldente_model::timing::Timings;
use aldente_model::{Lang, UnitInfo};

use crate::forms::index_files;
use crate::kondo::{Kondo, KondoError};
use crate::resolve::{Context, resolve};

/// The adapter's version, part of the snapshot key. Bump it whenever the
/// resolver's output for the same input changes.
pub const VERSION: &str = "12";

/// The `clj` adapter for Clojure, ClojureScript and `.cljc`.
#[derive(Debug, Clone)]
pub struct CljAdapter {
    kondo: Kondo,
    /// The local Maven repository to read jar listings from (R10); `None`
    /// reads none, which keeps tests independent of the machine.
    maven_repo: Option<std::path::PathBuf>,
}

impl CljAdapter {
    /// An adapter using this clj-kondo.
    ///
    /// Example: `CljAdapter::new(Kondo::new(path, empty_dir))`.
    pub fn new(kondo: Kondo) -> CljAdapter {
        CljAdapter {
            kondo,
            maven_repo: None,
        }
    }

    /// The same adapter reading jar listings from `repo`.
    ///
    /// Example: `CljAdapter::new(k).with_maven_repo(maven::default_repository())`.
    pub fn with_maven_repo(self, repo: Option<std::path::PathBuf>) -> CljAdapter {
        CljAdapter {
            maven_repo: repo,
            ..self
        }
    }
}

impl Adapter for CljAdapter {
    /// `"clj"`.
    fn id(&self) -> &'static str {
        "clj"
    }

    /// Units whose files include Clojure or ClojureScript.
    ///
    /// Example: clj-mini's unit (langs clj, cljs) is claimed.
    fn claims(&self, unit: &UnitInfo) -> bool {
        unit.langs
            .iter()
            .any(|l| matches!(l, Lang::Clj | Lang::Cljs))
    }

    /// Runs clj-kondo and the form index over the unit's files and resolves them.
    ///
    /// Example: on clj-mini gives about 90 nodes; a missing clj-kondo gives
    /// `AdapterError::ToolMissing`.
    fn analyze(&self, unit: &UnitInfo, ctx: AnalyzeCtx<'_>) -> Result<FactBatch, AdapterError> {
        let mut timings = Timings::default();
        let run = timings
            .time("clj-kondo", || self.kondo.analyze(ctx.root, ctx.files))
            .map_err(|e| match e {
                KondoError::Missing(_) => AdapterError::ToolMissing {
                    tool: "clj-kondo".into(),
                    hint: aldente_acquire::tools::install_hint(
                        &aldente_acquire::tools::CLJ_KONDO,
                        std::env::consts::OS,
                    ),
                },
                other => AdapterError::Failed(other.to_string()),
            })?;
        let indexes = timings.time("form-index", || index_files(ctx.root, ctx.files));
        let mut ctx = Context::builtin().map_err(|e| AdapterError::Failed(e.to_string()))?;
        if let Some(repo) = &self.maven_repo {
            ctx.jars = timings.time("jar-listing", || {
                aldente_acquire::maven::jar_namespaces(repo, &unit.deps)
            });
        }
        let mut batch = timings.time("resolve", || resolve(unit, &run, &indexes, &ctx).batch);
        batch.timings = timings;
        Ok(batch)
    }
}
