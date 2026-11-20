//! Graph, node, source, search, blind-spot and tree handlers.

use aldente_graph::query::{self, Direction};
use aldente_model::analyze::SnapshotOverview;
use aldente_model::query::{FindResult, ShowResult};
use aldente_model::{
    AggregationLevel, Envelope, ErrorCode, NodeId, NodeKind, Warning, WarningCode,
};
use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::{EXPAND_LIMIT, St, err, filter_from, ok, resolve_id, token};
use crate::params::Params;

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
/// namespace; `level=auto` is the finest level that fits, named in
/// `aggregated.level`. With `top=N` the level is kept and only the N nodes with the
/// most calls in and out are returned (`truncated` when some were dropped);
/// without it a too-large level falls back to a coarser one.
///
/// Example: `?level=namespace` for clj-mini gives 13 namespaces and externals.
pub async fn graph(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    if p.get("dir").is_some() && filter_from(&p).dir.is_none() {
        return err(
            StatusCode::BAD_REQUEST,
            "graph",
            ErrorCode::Usage,
            "dir must be a repository path like src/app",
            None,
        );
    }
    let default = if s.snapshot.units.len() > 1 {
        AggregationLevel::Unit
    } else {
        AggregationLevel::Namespace
    };
    if p.get("level") == Some("auto") {
        let (mut payload, _) = query::fit(
            &s.snapshot.graph,
            &s.cache,
            AggregationLevel::Symbol,
            &filter_from(&p),
        );
        payload.aggregated = payload.aggregated.or(Some(aldente_model::Aggregation {
            level: AggregationLevel::Symbol,
        }));
        return ok(&s, "graph", payload);
    }
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

/// `GET /expand?id=&visible=…&offset=&limit=`: one page of an aggregate's
/// children (best-connected first; `limit` at most 200, the default) and
/// their edges to what is on screen. While children remain, the aggregate
/// itself comes back too, standing for them.
///
/// Example: `?id=clj:.//shop.pricing&visible=clj:.//shop.util&limit=2`.
pub async fn expand(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let visible: Vec<NodeId> = p
        .all("visible")
        .iter()
        .filter_map(|v| v.parse().ok())
        .collect();
    expand_page(&s, &p, &visible)
}

/// `POST /expand?id=&offset=&limit=` with the visible IDs as a JSON array
/// of strings in the body: [`expand`] for screens too big for a URL.
///
/// Example: body `["clj:.//shop.util", "clj:.//shop.core"]`.
pub async fn expand_post(State(s): St, RawQuery(q): RawQuery, body: String) -> Response {
    let p = Params::parse(q.as_deref());
    let Ok(ids) = serde_json::from_str::<Vec<String>>(&body) else {
        return err(
            StatusCode::BAD_REQUEST,
            "graph",
            ErrorCode::Usage,
            "the body must be a JSON array of node IDs",
            None,
        );
    };
    let visible: Vec<NodeId> = ids.iter().filter_map(|v| v.parse().ok()).collect();
    expand_page(&s, &p, &visible)
}

/// The shared body of [`expand`] and [`expand_post`].
///
/// Example: `expand_page(&s, &params, &[])`.
fn expand_page(s: &crate::AppState, p: &Params, visible: &[NodeId]) -> Response {
    let id = match resolve_id(s, "graph", p.get("id")) {
        Ok(id) => id,
        Err(res) => return *res,
    };
    let number = |k: &str| p.get(k).and_then(|v| v.parse::<usize>().ok());
    let page = query::Page {
        offset: number("offset").unwrap_or(0),
        limit: number("limit")
            .filter(|l| *l > 0)
            .unwrap_or(EXPAND_LIMIT)
            .min(EXPAND_LIMIT),
    };
    let payload = query::expand_in(&s.snapshot.graph, &s.cache, &id, visible, page);
    ok(
        s,
        "graph",
        query::cap_expanded(query::filter(&s.snapshot.graph, payload, &filter_from(p))),
    )
}

/// `GET /unit-links?id=`: what a unit uses from each other unit and what
/// each uses from it (the service overview's details).
///
/// Example: `?id=unit:services/orders` in mono-mini lists `unit:libs/common`.
pub async fn unit_links(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let id = match resolve_id(&s, "unit-links", p.get("id")) {
        Ok(id) => id,
        Err(res) => return *res,
    };
    if !matches!(id, NodeId::Unit(_)) {
        return err(
            StatusCode::BAD_REQUEST,
            "unit-links",
            ErrorCode::Usage,
            "unit-links takes a unit ID",
            None,
        );
    }
    ok(&s, "unit-links", query::unit_links(&s.snapshot.graph, &id))
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

/// `GET /source?id=&context=` or `GET /source?file=&line=&context=`: the
/// definition text of a node, or the lines around a call site, as plain
/// text with `context` lines around it (default 3, at most 50); the
/// `x-aldente-first-line` header says which line it starts at. Only files
/// the snapshot analyzed are served, and only if unchanged since; IDs are
/// never treated as paths.
///
/// Example: `?id=clj:.//shop.core/checkout` gives lines 9–23 of `src/shop/core.clj`.
pub async fn source(State(s): St, RawQuery(q): RawQuery) -> Response {
    let p = Params::parse(q.as_deref());
    let missing = |m: &str| {
        err(
            StatusCode::NOT_FOUND,
            "source",
            ErrorCode::UnknownSymbol,
            m,
            None,
        )
    };
    let (file, from, to) = if let Some(file) = p.get("file") {
        let (Ok(file), Some(line)) = (
            aldente_model::RepoPath::parse(file),
            p.get("line").and_then(|l| l.parse::<u32>().ok()),
        ) else {
            return err(
                StatusCode::BAD_REQUEST,
                "source",
                ErrorCode::Usage,
                "a site needs file and line",
                None,
            );
        };
        (file, line, line)
    } else {
        let id = match resolve_id(&s, "source", p.get("id")) {
            Ok(id) => id,
            Err(res) => return *res,
        };
        let Some(loc) = s.snapshot.graph.nodes.get(&id).and_then(|n| n.loc.clone()) else {
            return missing("this node has no source location");
        };
        (loc.file, loc.line, loc.end_line)
    };
    let Some((_, hash)) = s.snapshot.files.iter().find(|(f, _)| f == &file) else {
        return missing("the file is not part of the snapshot");
    };
    let max = aldente_acquire::walk::Limits::default().max_file_bytes;
    let Ok(bytes) = aldente_acquire::walk::read_inside(&s.root, &file, max) else {
        return missing("the file cannot be read");
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
    let first = (from as usize).saturating_sub(context + 1);
    let last = to as usize + context;
    let body: String = text
        .lines()
        .skip(first)
        .take(last.saturating_sub(first))
        .map(|l| format!("{l}\n"))
        .collect();
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
            (
                header::HeaderName::from_static("x-aldente-first-line"),
                (first + 1).to_string(),
            ),
        ],
        body,
    )
        .into_response()
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
