//! The `data` shapes of `aldente lib ls` and `aldente lib usage`.
//! See "Library impact" in `docs/plan/08-crater-and-traces.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::certainty::{Certainty, Reason};
use crate::graph::Site;
use crate::id::NodeId;
use crate::kinds::EdgeKind;

/// How a library was tied to a namespace it provides (rule R10).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Attribution {
    /// The external or namespace node the library provides.
    pub provides: NodeId,
    pub certainty: Option<Certainty>,
    pub reason: Option<Reason>,
}

/// A version a unit declares for a library.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct DeclaredVersion {
    pub unit: NodeId,
    pub version: Option<String>,
}

/// One library in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct LibrarySummary {
    pub id: NodeId,
    pub ecosystem: String,
    pub coordinate: String,
    pub attribution: Vec<Attribution>,
    /// Repository symbols and impls that use it.
    pub symbols: u64,
    /// Repository namespaces that use or require it.
    pub namespaces: u64,
    /// Units that use or require it.
    pub units: u64,
    pub versions: Vec<DeclaredVersion>,
}

/// `aldente lib ls`: libraries sorted by ID, `lib:unknown` included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct LibrariesReport {
    pub libraries: Vec<LibrarySummary>,
}

/// How `lib usage` groups its rows.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum UsageGrouping {
    Namespace,
    Symbol,
}

/// One edge from repository code into the library, at one site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct UsageRow {
    pub from: NodeId,
    pub kind: EdgeKind,
    pub to: NodeId,
    pub certainty: Option<Certainty>,
    pub reason: Option<Reason>,
    pub site: Option<Site>,
}

/// The rows of one namespace or symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct UsageGroup {
    pub id: NodeId,
    pub rows: Vec<UsageRow>,
}

/// `aldente lib usage <library>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct LibraryUsage {
    pub library: NodeId,
    pub by: UsageGrouping,
    pub attribution: Vec<Attribution>,
    /// Uses, grouped and sorted by group ID; rows sorted.
    pub groups: Vec<UsageGroup>,
    /// Namespaces that require the library and never call into it.
    pub require_only: Vec<UsageGroup>,
}
