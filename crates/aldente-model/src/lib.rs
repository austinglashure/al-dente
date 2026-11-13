//! Node, edge, certainty and ID types, the JSON envelope and its schemas.
//! The Rust types here are the source of truth for every JSON shape; see
//! `docs/plan/03-graph-model.md` and `docs/plan/05-cli.md`.

pub mod analyze;
pub mod blindspot;
pub mod cache;
pub mod certainty;
pub mod churn;
pub mod coverage;
pub mod crater;
pub mod diagnostic;
pub mod doctor;
pub mod envelope;
pub mod graph;
pub mod id;
pub mod kinds;
pub mod libraries;
pub mod measures;
pub mod path;
pub mod query;
pub mod schema;
pub mod timing;
pub mod trace;
pub mod tree;
pub mod units;

#[cfg(test)]
mod strategies;

pub use blindspot::BlindSpot;
pub use certainty::{Certainty, Reason};
pub use diagnostic::{Diagnostic, DiagnosticCode};
pub use envelope::{
    Envelope, ErrorBody, ErrorCode, ExitCode, SnapshotInfo, Warning, WarningCode, schema_tag,
};
pub use graph::{
    Aggregation, AggregationLevel, Edge, EdgeError, GraphPayload, Location, Node, Site,
};
pub use id::{
    ExternalId, HoleId, IdError, ImplId, LibraryId, NamespaceId, NodeId, PlatformId, SymbolId,
};
pub use kinds::{BlindSpotCategory, EdgeKind, Lang, NodeKind, SymbolFlag, SymbolSubkind};
pub use path::{PathError, PathStyle, RepoPath};
pub use units::{DeclaredDep, RootKind, SourceRoot, UnitInfo, UnitsReport};
