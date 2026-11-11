//! The `data` shape of `aldente doctor`: whether the external tools and the
//! cache directory are usable on this machine.
//!
//! Unlike snapshot data, this describes the machine, so it carries absolute
//! paths (where each tool and the cache were found).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The full doctor report. `tools` is sorted by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct DoctorReport {
    pub tools: Vec<ToolCheck>,
    pub cache: CacheCheck,
    pub platform: Platform,
}

impl DoctorReport {
    /// Whether every check passed.
    ///
    /// Example: a report whose `git` check is `Missing` gives `false`.
    pub fn all_ok(&self) -> bool {
        self.tools.iter().all(|t| t.status == ToolStatus::Ok) && self.cache.writable
    }
}

/// One external tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ToolCheck {
    /// `git` or `clj-kondo`.
    pub name: String,
    pub status: ToolStatus,
    /// Absolute path of the executable found on `PATH`.
    pub path: Option<String>,
    /// Normalized version, e.g. `2.34.1` or `2026.10.26`.
    pub version: Option<String>,
    /// The oldest supported version.
    pub minimum: String,
    /// What went wrong, e.g. the unrecognized `--version` output.
    pub detail: Option<String>,
    /// How to install or upgrade on this platform; null when `status` is `ok`.
    pub hint: Option<String>,
}

/// The outcome of one tool check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ToolStatus {
    Ok,
    Missing,
    TooOld,
    /// Found, but `--version` failed or printed something unrecognizable.
    UnknownVersion,
}

/// The cache directory check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct CacheCheck {
    /// `<user cache dir>/aldente`, or null when the platform has none.
    pub path: Option<String>,
    pub writable: bool,
    pub detail: Option<String>,
}

/// The platform this binary runs on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Platform {
    /// `linux`, `macos` or `windows` (Rust's `std::env::consts::OS`).
    pub os: String,
    /// e.g. `x86_64` or `aarch64`.
    pub arch: String,
}
