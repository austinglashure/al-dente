//! `/api/v1` handlers: thin wrappers over the functions the CLI uses, with
//! the CLI's envelope and `data` shapes.

use std::sync::Arc;

use aldente_graph::query::{self, Direction, Filter, Resolution};
use aldente_model::analyze::SnapshotOverview;
use aldente_model::query::{FindResult, ShowResult};
use aldente_model::{
    AggregationLevel, Certainty, EdgeKind, Envelope, ErrorCode, NodeId, NodeKind, Warning,
    WarningCode,
};
use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::AppState;
use crate::params::Params;

type St = State<Arc<AppState>>;

/// The most children `/expand` returns before marking the result truncated.
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

/// `GET /snapshot`: summary and units.
///
/// Example: for clj-mini, `data.summary.units` is 1.
pub async fn snapshot(State(s): St) -> Response {
    let overview = SnapshotOverview {
        summary: s.summary.clone(),
        units: s.snapshot.units.clone(),
    };
    ok(&s, "snapshot", overview)
}

/// `GET /graph?level=&unit=&min_certainty=&kinds=&scope=&top=`: a graph
/// within the payload cap. Default level: unit for several units, else
/// namespace. With `top=N` the level is kept and only the N nodes with the
/// most calls in and out are returned (`truncated` when some were dropped);
/// without it a too-large level falls back to a coarser one.
///
/// Example: `?level=namespace` for clj-mini gives 13 namespaces and externals.
pub async fn graph(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let default = if s.snapshot.units.len() > 1 {
        AggregationLevel::Unit
    } else {
        AggregationLevel::Namespace
    };
    let level = p
        .get("level")
        .and_then(token::<AggregationLevel>)
        .unwrap_or(default);
    let top: Option<usize> = p.get("top").and_then(|t| t.parse().ok());
    let (payload, coarser) = match top {
        // Node cap: stay at this level, keep the busiest nodes.
        Some(n) => {
            let g = &s.snapshot.graph;
            let all = query::filter(g, s.cache.payload(g, level).into_owned(), &filter_from(&p));
            (query::top_by_calls(all, n), false)
        }
        None => query::fit(&s.snapshot.graph, &s.cache, level, &filter_from(&p)),
    };
    let mut env = Envelope::success("graph", Some(s.info.clone()), payload);
    if coarser {
        env.warnings.push(Warning {
            code: WarningCode::Coarser,
            message: "shown at a coarser level to stay within the size limit".to_owned(),
        });
    }
    (
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_string(&env).unwrap_or_default(),
    )
        .into_response()
}

/// `GET /neighborhood?id=&depth=&direction=&min_certainty=`: symbol-level
/// nodes and edges around one node, capped.
///
/// Example: `?id=checkout&depth=1&direction=out` gives checkout and its callees.
pub async fn neighborhood(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let id = match resolve_id(&s, "graph", p.get("id")) {
        Ok(id) => id,
        Err(res) => return *res,
    };
    let depth = p
        .get("depth")
        .and_then(|d| d.parse().ok())
        .unwrap_or(1usize)
        .min(10);
    let direction = match p.get("direction") {
        Some("out") => Direction::Out,
        Some("in") => Direction::In,
        _ => Direction::Both,
    };
    let hood = query::neighborhood_in(&s.snapshot.graph, &s.cache, &id, depth, direction);
    let hood = query::filter(&s.snapshot.graph, hood, &filter_from(&p));
    ok(&s, "graph", query::cap(hood))
}

/// `GET /expand?id=&visible=…`: children of an aggregate and their edges to
/// what is on screen.
///
/// Example: `?id=clj:.//shop.pricing&visible=clj:.//shop.util`.
pub async fn expand(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let id = match resolve_id(&s, "graph", p.get("id")) {
        Ok(id) => id,
        Err(res) => return *res,
    };
    let visible: Vec<NodeId> = p
        .all("visible")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    ok(
        &s,
        "graph",
        query::filter(
            &s.snapshot.graph,
            query::expand_in(&s.snapshot.graph, &s.cache, &id, &visible, EXPAND_LIMIT),
            &filter_from(&p),
        ),
    )
}

/// `GET /node?id=`: details for the side panel (the CLI's `show`).
///
/// Example: `?id=checkout` gives its node, callers and callees.
pub async fn node(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let id = match resolve_id(&s, "show", p.get("id")) {
        Ok(id) => id,
        Err(res) => return *res,
    };
    let Some(node) = s.snapshot.graph.nodes.get(&id).cloned() else {
        return err(
            StatusCode::NOT_FOUND,
            "show",
            ErrorCode::UnknownSymbol,
            "no such node",
            None,
        );
    };
    let (mut callers, mut callees) = query::neighbors(&s.snapshot.graph, &s.cache, &id);
    if let Some(min) = filter_from(&p).min_certainty {
        callers.retain(|n| n.certainty.is_none_or(|c| c >= min));
        callees.retain(|n| n.certainty.is_none_or(|c| c >= min));
    }
    ok(
        &s,
        "show",
        ShowResult {
            node,
            callers,
            callees,
            children: query::children(&s.snapshot.graph, &id),
        },
    )
}

/// `GET /source?id=&context=`: the definition text of a node as plain text,
/// with `context` lines around it (default 3, at most 50). Only files the
/// snapshot analyzed are served, and only if unchanged since; IDs are never
/// treated as paths.
///
/// Example: `?id=clj:.//shop.core/checkout` gives lines 9–23 of `src/shop/core.clj`.
pub async fn source(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let id = match resolve_id(&s, "source", p.get("id")) {
        Ok(id) => id,
        Err(res) => return *res,
    };
    let Some(loc) = s.snapshot.graph.nodes.get(&id).and_then(|n| n.loc.clone()) else {
        return err(
            StatusCode::NOT_FOUND,
            "source",
            ErrorCode::UnknownSymbol,
            "this node has no source location",
            None,
        );
    };
    let Some((_, hash)) = s.snapshot.files.iter().find(|(f, _)| f == &loc.file) else {
        return err(
            StatusCode::NOT_FOUND,
            "source",
            ErrorCode::UnknownSymbol,
            "the file is not part of the snapshot",
            None,
        );
    };
    let max = aldente_acquire::walk::Limits::default().max_file_bytes;
    let Ok(bytes) = aldente_acquire::walk::read_inside(&s.root, &loc.file, max) else {
        return err(
            StatusCode::NOT_FOUND,
            "source",
            ErrorCode::UnknownSymbol,
            "the file cannot be read",
            None,
        );
    };
    if blake3::hash(&bytes).to_hex().as_str() != hash {
        return err(
            StatusCode::CONFLICT,
            "source",
            ErrorCode::Usage,
            "the file changed since the snapshot; run analyze again",
            None,
        );
    }
    let context = p
        .get("context")
        .and_then(|c| c.parse().ok())
        .unwrap_or(3usize)
        .min(50);
    let text = String::from_utf8_lossy(&bytes);
    let first = (loc.line as usize).saturating_sub(context + 1);
    let last = loc.end_line as usize + context;
    let body: String = text
        .lines()
        .skip(first)
        .take(last.saturating_sub(first))
        .map(|l| format!("{l}\n"))
        .collect();
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

/// `GET /search?q=&kind=&limit=`: matches by name or ID (the CLI's `find`).
///
/// Example: `?q=charge&limit=5`.
pub async fn search(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let kinds: Vec<NodeKind> = p
        .all("kind")
        .into_iter()
        .filter_map(token::<NodeKind>)
        .collect();
    let limit = p
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(20usize)
        .min(500);
    let (matches, truncated) =
        query::search(&s.snapshot.graph, p.get("q").unwrap_or(""), &kinds, limit);
    ok(&s, "find", FindResult { matches, truncated })
}

/// `GET /blindspots`: blind spots and diagnostics (the CLI's `blindspots`).
///
/// Example: for clj-mini, two holes and one `macro-generated` entry.
pub async fn blindspots(State(s): St) -> Response {
    let mut diagnostics = s.snapshot.graph.diagnostics.clone();
    diagnostics.extend(s.summary.diagnostics.iter().cloned());
    diagnostics.sort();
    diagnostics.dedup();
    let report = aldente_model::query::BlindspotReport {
        blindspots: s.snapshot.graph.blindspots.clone(),
        diagnostics,
    };
    ok(&s, "blindspots", report)
}

/// `GET /tree`: the repository's directory tree, including source files the
/// analyzer skipped.
///
/// Example: for clj-mini, `data.dirs[0].label` is `src/shop`.
pub async fn tree(State(s): St) -> Response {
    ok(&s, "tree", s.tree())
}

/// `GET /tree/graph?collapsed=dir:…&top=`: the repository's code at the
/// tree's resolution, each collapsed directory one node; with `top=N` only
/// the N nodes with the most calls in and out.
///
/// Example: `?collapsed=dir:services` gives `dir:services` and the other
/// namespaces.
pub async fn tree_graph(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let mut collapsed = std::collections::BTreeSet::new();
    for raw in p.all("collapsed") {
        match raw.parse::<NodeId>() {
            Ok(NodeId::Dir(path)) => {
                collapsed.insert(path);
            }
            _ => {
                return err(
                    StatusCode::BAD_REQUEST,
                    "graph",
                    ErrorCode::Usage,
                    "collapsed takes dir: IDs",
                    None,
                );
            }
        }
    }
    let payload = aldente_graph::tree::tree_graph(&s.snapshot.graph, s.tree(), &collapsed);
    let payload = match p.get("top").and_then(|t| t.parse().ok()) {
        Some(n) => query::top_by_calls(payload, n),
        None => query::cap(payload),
    };
    ok(&s, "graph", payload)
}

/// `GET /libs`: libraries in use with their counts (the CLI's `lib ls`).
///
/// Example: for clj-mini, `data.libraries` includes `lib:npm:react`.
pub async fn libs(State(s): St) -> Response {
    let report = aldente_analysis::libraries::summaries(&s.snapshot.graph, &s.snapshot.units);
    ok(&s, "libs", report)
}

/// `GET /lib?id=&by=namespace|symbol`: every use of one library (the CLI's
/// `lib usage`). `id` takes a full ID, a coordinate or an artifact name.
///
/// Example: `?id=react` gives `lib:npm:react`'s calls and requires.
pub async fn lib(State(s): St, RawQuery(q): RawQuery) -> Response {
    use aldente_analysis::libraries::{LibraryLookup, find_library, usage};
    use aldente_model::libraries::UsageGrouping;
    const TAG: &str = "lib-usage";
    let p = Params::parse(q.as_deref());
    let Some(query) = p.get("id") else {
        return err(
            StatusCode::BAD_REQUEST,
            TAG,
            ErrorCode::Usage,
            "id is required",
            None,
        );
    };
    let by = match p.get("by") {
        None | Some("namespace") => UsageGrouping::Namespace,
        Some("symbol") => UsageGrouping::Symbol,
        Some(_) => {
            return err(
                StatusCode::BAD_REQUEST,
                TAG,
                ErrorCode::Usage,
                "by is namespace or symbol",
                None,
            );
        }
    };
    match find_library(&s.snapshot.graph, query) {
        LibraryLookup::Found(id) => ok(&s, TAG, usage(&s.snapshot.graph, &id, by)),
        LibraryLookup::NotFound => err(
            StatusCode::NOT_FOUND,
            TAG,
            ErrorCode::UnknownSymbol,
            "no such library",
            None,
        ),
        LibraryLookup::Ambiguous(ids) => err(
            StatusCode::BAD_REQUEST,
            TAG,
            ErrorCode::AmbiguousSymbol,
            "several libraries match",
            Some(ids.iter().map(ToString::to_string).collect()),
        ),
    }
}

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
