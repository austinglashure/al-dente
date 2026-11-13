//! The repository's directory tree, as the GUI shows it (`/tree`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{NodeId, RepoPath};

/// A row of the tree: a directory, or a chain of directories that each hold
/// nothing but the next one (`libs/common/src/common`), shown as one row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct TreeDir {
    /// `dir:<path>` of the deepest directory of the row; `None` for the root.
    pub id: Option<NodeId>,
    /// The deepest directory of the row (`.` for the root).
    pub path: RepoPath,
    /// The row's text: its directories relative to the parent row,
    /// e.g. `libs/common/src/common`; `.` for the root.
    pub label: String,
    /// Sorted by label.
    pub dirs: Vec<TreeDir>,
    /// Sorted by name.
    pub files: Vec<TreeFile>,
}

/// A source file in the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct TreeFile {
    pub path: RepoPath,
    /// The file name, e.g. `core.clj`.
    pub name: String,
    /// The namespaces it defines (usually one), sorted.
    pub namespaces: Vec<NodeId>,
    pub status: FileStatus,
}

/// Whether a file's code is in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FileStatus {
    Analyzed,
    /// Analyzed, but the parser reported an error (its facts may be partial).
    ParseError,
    /// Not under any declared source root of its unit.
    NotAnalyzed,
}
