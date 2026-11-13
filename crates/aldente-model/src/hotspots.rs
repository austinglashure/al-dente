//! The `data` shape of `aldente hotspots`: ranked findings with evidence
//! and a reason, and the risk score per symbol. See "Findings, not just
//! numbers" in `docs/plan/09-network-and-coverage.md`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::coverage::CoverageStatus;
use crate::id::NodeId;

/// What a finding says.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum FindingKind {
    GodObject,
    MustBeBulletproof,
    Bottleneck,
    Cycle,
    HiddenCoupling,
    RiskyUntested,
    MisplacedCode,
    DeadCode,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Finding {
    pub kind: FindingKind,
    /// A node ID, or two file paths joined by ` <-> ` for hidden coupling.
    pub subject: String,
    /// The nodes involved (one, or a cycle's members), sorted.
    pub nodes: Vec<NodeId>,
    /// 0..1, for ranking.
    pub score: f64,
    /// The measured values behind the finding.
    pub evidence: BTreeMap<String, f64>,
    /// One line a person can act on.
    pub reason: String,
}

/// Risk of one symbol: importance × exposure (× churn when read).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct RiskRow {
    pub id: NodeId,
    pub risk: f64,
    /// Blend of PageRank, fan-in and dominator reach, 0..1.
    pub importance: f64,
    /// 0..1, from the coverage status or measured coverage.
    pub exposure: f64,
    pub status: CoverageStatus,
    pub measured: Option<f64>,
    /// Commits in the symbol window, when history was read.
    pub churn: Option<u32>,
}

/// `aldente hotspots`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct HotspotsReport {
    /// `builtin` or `file` (`--rules`).
    pub rules: String,
    /// Whether history was read (`--with-churn`).
    pub churn: bool,
    /// Highest score first, then by kind and subject.
    pub findings: Vec<Finding>,
    /// Highest risk first, then by ID.
    pub risk: Vec<RiskRow>,
}
