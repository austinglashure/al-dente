//! The `data` shape of `aldente units`: build units, their source roots and
//! what the file walk skipped.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::diagnostic::Diagnostic;
use crate::id::NodeId;
use crate::kinds::Lang;
use crate::path::RepoPath;

/// Units sorted by directory, diagnostics sorted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct UnitsReport {
    /// Whether the target is inside a git repository; false means the target
    /// directory itself was used as the root.
    pub git: bool,
    pub units: Vec<UnitInfo>,
    pub diagnostics: Vec<Diagnostic>,
}

/// One build unit: a directory owning one or more manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct UnitInfo {
    /// `unit:<dir>`.
    pub id: NodeId,
    pub dir: RepoPath,
    /// Manifest file names in this directory, sorted, e.g. `["deps.edn"]`.
    /// Empty for the implicit root unit.
    pub manifests: Vec<String>,
    /// Languages of the source files the unit owns.
    pub langs: BTreeSet<Lang>,
    /// Sorted by path.
    pub roots: Vec<SourceRoot>,
    /// Has an entry point declared in a manifest or a `Dockerfile`.
    pub service: bool,
    /// Number of source files the unit owns that lie under its roots.
    pub files: usize,
}

/// A source or test root of a unit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SourceRoot {
    /// Repository-relative.
    pub path: RepoPath,
    pub kind: RootKind,
    /// The manifest that declared it; null for the fallback roots.
    pub manifest: Option<String>,
    /// The deps.edn alias or Leiningen profile that adds it, without the colon.
    pub alias: Option<String>,
}

/// Whether a root holds tests.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RootKind {
    Source,
    Test,
}
