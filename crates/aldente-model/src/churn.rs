//! The `data` shape of `aldente churn`: change frequency and co-change
//! from git history. See "Churn and co-change" in
//! `docs/plan/09-network-and-coverage.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::NodeId;

/// How much history was available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryDepth {
    /// Every commit up to the limits was read.
    Full,
    /// A shallow clone: older commits are missing.
    Shallow,
    /// No git history at all.
    None,
}

/// Changes to one file (renames followed to its current path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct FileChurn {
    pub path: String,
    pub commits: u32,
    pub added: u64,
    pub deleted: u64,
    /// Committer date of the newest change, `YYYY-MM-DD` (UTC).
    pub last_changed: String,
}

/// Changes to one symbol in the recent window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SymbolChurn {
    pub id: NodeId,
    /// Commits in the window whose changed lines overlap the symbol's
    /// current source range.
    pub commits: u32,
}

/// Two files that change together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CoChange {
    /// `a < b`.
    pub a: String,
    pub b: String,
    /// Commits changing both.
    pub support: u32,
    /// support / commits changing `a`.
    pub confidence_a_b: f64,
    /// support / commits changing `b`.
    pub confidence_b_a: f64,
    /// Fewest hops between the files' code in the static graph (calls,
    /// references, requires, …, either direction), if within the limit.
    pub static_hops: Option<u32>,
    /// Both are analyzed source files and no static path joins them within
    /// the limit: coupling the code does not show.
    pub hidden: bool,
}

/// `aldente churn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ChurnReport {
    pub history: HistoryDepth,
    /// Commits read for file churn and co-change.
    pub commits: u32,
    /// The newest commits used for symbol attribution.
    pub symbol_commits: u32,
    pub since: Option<String>,
    /// Most changed first, then by path.
    pub files: Vec<FileChurn>,
    /// Most changed first, then by ID.
    pub symbols: Vec<SymbolChurn>,
    /// Highest support first, then by paths.
    pub co_changes: Vec<CoChange>,
}
