//! The `data` shapes of `aldente cache ls`, `gc` and `clear`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the cache holds. Lists are sorted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CacheReport {
    /// Absolute path of the cache directory (this describes the machine).
    pub root: String,
    /// Remote clones.
    pub repos: Vec<CachedRepo>,
    /// Analysis snapshots.
    pub snapshots: Vec<CachedSnapshot>,
    /// Total size of everything in the cache.
    pub bytes: u64,
}

/// One remote clone.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CachedRepo {
    /// `<host>/<owner>/<name>`, relative to the `repos` directory.
    pub path: String,
    pub bytes: u64,
}

/// One snapshot file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CachedSnapshot {
    /// The snapshot key, `b3:…`.
    pub key: String,
    pub bytes: u64,
}

/// What `gc` or `clear` removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CacheRemoval {
    /// Removed entries, relative to the cache directory, sorted.
    pub removed: Vec<String>,
    pub bytes: u64,
}
