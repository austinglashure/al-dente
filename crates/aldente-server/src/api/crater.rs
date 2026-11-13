//! Crater, checklist and trace handlers.

use aldente_model::{ErrorCode, NodeId};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::{St, err, ok, resolve_id, token};
use crate::AppState;
use crate::params::Params;

/// Crater settings from `direction`, `depth`, `min_certainty`, `limit`,
/// `no_tests` and `through_hubs`; defaults as on the command line.
///
/// Example: `?direction=reverse&depth=3` gives a reverse crater of depth 3.
fn crater_options(
    p: &Params,
    default: aldente_model::crater::Direction,
) -> aldente_analysis::crater::CraterOptions {
    use aldente_analysis::crater::CraterOptions;
    let d = CraterOptions::default();
    CraterOptions {
        direction: p.get("direction").and_then(token).unwrap_or(default),
        depth: p
            .get("depth")
            .and_then(|v| v.parse().ok())
            .unwrap_or(d.depth),
        min_certainty: p
            .get("min_certainty")
            .and_then(token)
            .unwrap_or(d.min_certainty),
        node_cap: p
            .get("limit")
            .and_then(|v| v.parse::<usize>().ok())
            .map_or(d.node_cap, |n| n.min(50_000)),
        exclude_tests: p.get("no_tests") == Some("true"),
        through_hubs: p.get("through_hubs") == Some("true"),
        ..d
    }
}

/// The seeds named by every `id` parameter.
///
/// Example: `?id=checkout&id=half-off` gives both nodes.
fn crater_seeds(s: &AppState, tag: &str, p: &Params) -> Result<Vec<NodeId>, Box<Response>> {
    let ids = p.all("id");
    if ids.is_empty() {
        return Err(Box::new(err(
            StatusCode::BAD_REQUEST,
            tag,
            ErrorCode::Usage,
            "missing id",
            None,
        )));
    }
    ids.into_iter()
        .map(|id| resolve_id(s, tag, Some(id)))
        .collect()
}

/// Runs a crater for the query's seeds and settings.
///
/// Example: see [`crater`].
fn run_crater(
    s: &AppState,
    tag: &str,
    p: &Params,
) -> Result<aldente_model::crater::CraterResult, Box<Response>> {
    let seeds = crater_seeds(s, tag, p)?;
    let opts = crater_options(p, aldente_model::crater::Direction::Forward);
    aldente_analysis::crater::crater(&s.snapshot.graph, s.crater_index(), &seeds, &opts).map_err(
        |e| {
            Box::new(err(
                StatusCode::NOT_FOUND,
                tag,
                ErrorCode::UnknownSymbol,
                &e.to_string(),
                None,
            ))
        },
    )
}

/// `GET /crater?id=…&direction=&depth=&min_certainty=&limit=`: the impact
/// crater (the CLI's `crater`).
///
/// Example: `?id=checkout` gives the forward crater of `shop.core/checkout`.
pub async fn crater(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    match run_crater(&s, "crater", &p) {
        Ok(r) => ok(&s, "crater", r),
        Err(res) => *res,
    }
}

/// `GET /checklist?…`: the crater as places to verify (the CLI's
/// `crater --checklist`); same parameters as `/crater`.
///
/// Example: `?id=half-off&direction=reverse` lists `half-off` and its callers.
pub async fn checklist(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    match run_crater(&s, "checklist", &p) {
        Ok(r) => ok(
            &s,
            "checklist",
            aldente_model::crater::Checklist {
                rows: aldente_analysis::crater::checklist(&s.snapshot.graph, &r),
                seeds: r.seeds,
                direction: r.direction,
                depth: r.depth,
                min_certainty: r.min_certainty,
                hidden_below_min: r.hidden_below_min,
                truncated: r.truncated,
                diff: None,
            },
        ),
        Err(res) => *res,
    }
}

/// `POST /trace/normalize` with the raw trace as the body: canonical frames
/// mapped to nodes (the CLI's `trace normalize`, without source maps).
///
/// Example: a clj-mini trace gives `shop.*` frames mapped `exact`.
pub async fn trace_normalize(State(s): St, body: String) -> Response {
    match aldente_analysis::trace::normalize(&body, &s.snapshot.graph) {
        Ok(r) => ok(&s, "trace", r),
        Err(e) => err(
            StatusCode::BAD_REQUEST,
            "trace",
            ErrorCode::Usage,
            &e.to_string(),
            None,
        ),
    }
}

/// `POST /crater/trace?direction=&depth=&min_certainty=` with the raw trace
/// as the body: the crater pinned to it (the CLI's `crater --trace`).
/// Direction defaults to reverse.
///
/// Example: clj-mini's `:book` trace pins `price@:book`.
pub async fn crater_trace(State(s): St, RawQuery(q): RawQuery, body: String) -> Response {
    let p = Params::parse(q.as_deref());
    let report = match aldente_analysis::trace::normalize(&body, &s.snapshot.graph) {
        Ok(r) => r,
        Err(e) => {
            return err(
                StatusCode::BAD_REQUEST,
                "crater",
                ErrorCode::Usage,
                &e.to_string(),
                None,
            );
        }
    };
    let opts = crater_options(&p, aldente_model::crater::Direction::Reverse);
    match aldente_analysis::crater::pinned::pinned_crater(
        &s.snapshot.graph,
        s.crater_index(),
        &report,
        &opts,
    ) {
        Ok(r) => ok(&s, "crater", r),
        Err(e) => err(
            StatusCode::NOT_FOUND,
            "crater",
            ErrorCode::UnknownSymbol,
            &e.to_string(),
            None,
        ),
    }
}
