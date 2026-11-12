//! The Clojure adapter: clj-kondo plus the form index, resolved by R1–R4.

use aldente_adapter::{Adapter, AdapterError, AnalyzeCtx, FactBatch};
use aldente_model::{Lang, UnitInfo};

use crate::forms::index_files;
use crate::kondo::{Kondo, KondoError};
use crate::resolve::resolve;

/// The adapter's version, part of the snapshot key. Bump it whenever the
/// resolver's output for the same input changes.
pub const VERSION: &str = "5";

/// The `clj` adapter for Clojure, ClojureScript and `.cljc`.
#[derive(Debug, Clone)]
pub struct CljAdapter {
    kondo: Kondo,
}

impl CljAdapter {
    /// An adapter using this clj-kondo.
    ///
    /// Example: `CljAdapter::new(Kondo::new(path, empty_dir))`.
    pub fn new(kondo: Kondo) -> CljAdapter {
        CljAdapter { kondo }
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
        let run = self
            .kondo
            .analyze(ctx.root, ctx.files)
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
        let indexes = index_files(ctx.root, ctx.files);
        Ok(resolve(unit, &run, &indexes).batch)
    }
}
