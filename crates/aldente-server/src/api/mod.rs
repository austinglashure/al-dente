//! `/api/v1` handlers: thin wrappers over the functions the CLI uses, with
//! the CLI's envelope and `data` shapes.

use std::sync::Arc;

use aldente_graph::query::{self, Filter, Resolution};
use aldente_model::{Certainty, EdgeKind, Envelope, ErrorCode, NodeId};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::AppState;
use crate::params::Params;

mod crater;
mod graph;
mod libs;
mod measures;
mod open;

pub use crater::{checklist, crater, crater_trace, trace_normalize};
pub use graph::{
    blindspots, expand, expand_post, graph, neighborhood, node, search, snapshot, source, tree,
    tree_graph,
};
pub use libs::{lib, libs};
pub use measures::{churn, coverage, cycles, defaults, hotspots, metrics, subgraph};
pub use open::open;

type St = State<Arc<AppState>>;

/// The most children one `/expand` page returns (`limit` is clamped to it).
const EXPAND_LIMIT: usize = 200;

/// A JSON success envelope.
///
/// Example: `ok(&state, "graph", payload)` gives `200` with `{"schema": "aldente/graph/v1", …}`.
fn ok<T: Serialize>(state: &AppState, tag: &str, data: T) -> Response {
    let env = Envelope::success(tag, Some(state.info.clone()), data);
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&env).unwrap_or_default(),
    )
        .into_response()
}

/// A JSON success envelope with warnings.
///
/// Example: `with_warnings(&state, "churn", report, &["no git history".into()])`.
fn with_warnings<T: Serialize>(
    state: &AppState,
    tag: &str,
    data: T,
    warnings: &[String],
) -> Response {
    let mut env = Envelope::success(tag, Some(state.info.clone()), data);
    env.warnings = warnings
        .iter()
        .map(|m| aldente_model::Warning {
            code: aldente_model::WarningCode::NoHistory,
            message: m.clone(),
        })
        .collect();
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&env).unwrap_or_default(),
    )
        .into_response()
}

/// A JSON failure envelope with an HTTP status.
///
/// Example: `err(StatusCode::NOT_FOUND, "node", ErrorCode::UnknownSymbol, "no node", None)`.
fn err(
    status: StatusCode,
    tag: &str,
    code: ErrorCode,
    message: &str,
    candidates: Option<Vec<String>>,
) -> Response {
    let mut env = Envelope::<()>::failure(tag, code, message);
    if let Some(body) = env.error.as_mut() {
        body.candidates = candidates;
    }
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&env).unwrap_or_default(),
    )
        .into_response()
}

/// A 404 for unknown API paths.
///
/// Example: `GET /api/v1/nope` gives `404` with an `unknown-symbol` error.
pub fn not_found(message: &str) -> Response {
    err(
        StatusCode::NOT_FOUND,
        "api",
        ErrorCode::UnknownSymbol,
        message,
        None,
    )
}

/// Parses a model enum from its JSON spelling.
///
/// Example: `token::<Certainty>("likely")` is `Some(Likely)`.
fn token<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

/// Resolves `id` (full or short) or returns the error response.
///
/// Example: `resolve_id(&state, "node", Some("checkout"))` gives the full ID.
fn resolve_id(state: &AppState, tag: &str, id: Option<&str>) -> Result<NodeId, Box<Response>> {
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return Err(Box::new(err(
            StatusCode::BAD_REQUEST,
            tag,
            ErrorCode::Usage,
            "missing id",
            None,
        )));
    };
    match query::resolve(&state.snapshot.graph, id) {
        Resolution::Found(id) => Ok(id),
        Resolution::NotFound => Err(Box::new(err(
            StatusCode::NOT_FOUND,
            tag,
            ErrorCode::UnknownSymbol,
            "no such node",
            None,
        ))),
        Resolution::Ambiguous(ids) => Err(Box::new(err(
            StatusCode::CONFLICT,
            tag,
            ErrorCode::AmbiguousSymbol,
            "ambiguous id",
            Some(ids.iter().map(ToString::to_string).collect()),
        ))),
    }
}

/// The filter from `min_certainty`, `kinds` (comma-separated), `unit` and
/// `scope=repo` (repository code and holes only).
///
/// Example: `?min_certainty=likely&kinds=calls,references`.
fn filter_from(p: &Params) -> Filter {
    Filter {
        min_certainty: p.get("min_certainty").and_then(token::<Certainty>),
        kinds: p
            .get("kinds")
            .map(|k| k.split(',').filter_map(token::<EdgeKind>).collect())
            .unwrap_or_default(),
        unit: p.get("unit").and_then(|u| u.parse().ok()),
        repo_only: p.get("scope") == Some("repo"),
    }
}
