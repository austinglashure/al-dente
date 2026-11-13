//! Measure, coverage, hotspot, cycle and churn handlers, and `/subgraph`
//! (a set of nodes with the edges between them, for "show this cycle").

use aldente_analysis::measures::SecondOptions;
use aldente_analysis::measures::cache;
use aldente_model::measures::{CyclesReport, MetricsReport, ProjectionKind, ProjectionSpec};
use aldente_model::{AggregationLevel, EdgeKind, ErrorCode, GraphPayload, NodeId};
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::{St, err, ok, token};
use crate::AppState;
use crate::params::Params;

/// Most IDs `/subgraph` takes.
const SUBGRAPH_LIMIT: usize = 2000;

/// The level from `level=symbol|namespace|unit` (default `default`).
///
/// Example: `?level=namespace` gives `AggregationLevel::Namespace`.
fn level(p: &Params, default: AggregationLevel) -> AggregationLevel {
    p.get("level").and_then(token).unwrap_or(default)
}

/// A usage error response.
///
/// Example: `bad("metrics", "level")`.
fn bad(tag: &str, message: &str) -> Response {
    err(
        StatusCode::BAD_REQUEST,
        tag,
        ErrorCode::Usage,
        message,
        None,
    )
}

/// `GET /metrics?level=`: both measure sets on the invocation projection
/// (the CLI's `metrics`).
///
/// Example: `?level=namespace` gives one row per namespace.
pub async fn metrics(State(s): St, RawQuery(q): RawQuery) -> Response {
    const TAG: &str = "metrics";
    let p = Params::parse(q.as_deref());
    let spec = ProjectionSpec {
        level: level(&p, AggregationLevel::Symbol),
        ..ProjectionSpec::default()
    };
    let g = &s.snapshot.graph;
    let both = cache::measures(g, &spec, &s.results).and_then(|(first, _)| {
        cache::second(g, &spec, SecondOptions::default(), &s.results)
            .map(|(second, _)| MetricsReport { first, second })
    });
    match both {
        Ok(r) => ok(&s, TAG, r),
        Err(e) => bad(TAG, &e.to_string()),
    }
}

/// `GET /coverage`: the static estimate (the CLI's `coverage`).
///
/// Example: for clj-mini, `shop.pricing/price` is `direct`.
pub async fn coverage(State(s): St) -> Response {
    ok(&s, "coverage", s.coverage())
}

/// `GET /hotspots`: findings and risk with the built-in rules (the CLI's
/// `hotspots`, without churn).
///
/// Example: for clj-mini, sixteen dead-code findings.
pub async fn hotspots(State(s): St) -> Response {
    ok(&s, "hotspots", hotspots_report(&s))
}

/// The hotspots report (computed once per session).
///
/// Example: see [`hotspots`].
fn hotspots_report(s: &AppState) -> &aldente_model::hotspots::HotspotsReport {
    s.hotspots.get_or_init(|| {
        let rules = builtin_rules();
        aldente_analysis::hotspots::from_graph(
            &s.snapshot.graph,
            &rules,
            "builtin",
            None,
            &s.results,
        )
        .0
    })
}

/// The hotspot rules the server uses: the built-in `rules/hotspots.toml`.
///
/// Example: `builtin_rules().risk.pagerank` is `0.5`.
fn builtin_rules() -> aldente_rules::hotspots::HotspotRules {
    aldente_rules::hotspots::HotspotRules::builtin()
        .unwrap_or_else(|e| unreachable_rules(&e.to_string()))
}

/// What `GET /defaults` returns.
#[derive(serde::Serialize)]
struct Defaults {
    /// The hotspot rules in use, keys as in the TOML file.
    hotspots: aldente_rules::hotspots::HotspotRules,
    /// Edge weights by certainty for the measures.
    certainty_weights: aldente_model::measures::Weights,
    /// Hops from a test within which reachable code is `indirect`.
    coverage_depth: u32,
}

/// `GET /defaults`: the hotspot rules and analyzer defaults in use, so the
/// GUI's Help guide quotes them instead of hard-coding them.
///
/// Example: `data.hotspots.risk.pagerank` is `0.5`; `data.coverage_depth` is `4`.
pub async fn defaults(State(s): St) -> Response {
    ok(
        &s,
        "defaults",
        Defaults {
            hotspots: builtin_rules(),
            certainty_weights: aldente_model::measures::Weights::default(),
            coverage_depth: aldente_analysis::coverage::DEFAULT_DEPTH,
        },
    )
}

/// The built-in rules are checked by tests; a broken build stops here.
///
/// Example: never reached in a tested build.
fn unreachable_rules(message: &str) -> aldente_rules::hotspots::HotspotRules {
    panic!("built-in hotspot rules do not parse: {message}")
}

/// `GET /cycles?level=namespace|unit|symbol`: strongly connected components
/// (structure projection, invocation at symbol level).
///
/// Example: `?level=symbol` lists call cycles.
pub async fn cycles(State(s): St, RawQuery(q): RawQuery) -> Response {
    const TAG: &str = "cycles";
    let p = Params::parse(q.as_deref());
    let lv = level(&p, AggregationLevel::Namespace);
    let spec = ProjectionSpec {
        kind: if lv == AggregationLevel::Symbol {
            ProjectionKind::Invocation
        } else {
            ProjectionKind::Structure
        },
        level: lv,
        ..ProjectionSpec::default()
    };
    match cache::measures(&s.snapshot.graph, &spec, &s.results) {
        Ok((r, _)) => ok(
            &s,
            TAG,
            CyclesReport {
                projection: spec,
                components: r.components,
            },
        ),
        Err(e) => bad(TAG, &e.to_string()),
    }
}

/// `GET /churn`: change counts from local history (no fetch). An empty
/// report when the target has no history or the server has no git.
///
/// Example: for a repository with two commits, `data.commits` is 2.
pub async fn churn(State(s): St) -> Response {
    let (report, warnings) = s.churn.get_or_init(|| read_churn(&s));
    super::with_warnings(&s, "churn", report, warnings)
}

/// Reads history with the session's git.
///
/// Example: see [`churn`].
fn read_churn(s: &AppState) -> (aldente_model::churn::ChurnReport, Vec<String>) {
    use aldente_acquire::history::{self, LogOptions};
    use aldente_analysis::churn::{ChurnOptions, History};
    use aldente_model::churn::HistoryDepth;
    let empty = |depth| History {
        depth,
        commits: vec![],
        hunks: vec![],
        since: None,
    };
    let mut warnings = Vec::new();
    let hist = match (&s.git, s.snapshot.git) {
        (Some(git), true) => {
            let opts = LogOptions {
                max_commits: 1000,
                since: None,
            };
            let window = LogOptions {
                max_commits: 200,
                since: None,
            };
            match (
                history::numstat(git, &s.root, &opts),
                history::hunks(git, &s.root, &window),
            ) {
                (Ok(commits), Ok(hunks)) => {
                    let depth = if history::is_shallow(&s.root) {
                        warnings.push("a shallow clone: older history is missing (run `aldente churn` to fetch it)".to_owned());
                        HistoryDepth::Shallow
                    } else {
                        HistoryDepth::Full
                    };
                    History {
                        depth,
                        commits,
                        hunks,
                        since: None,
                    }
                }
                (Err(e), _) | (_, Err(e)) => {
                    warnings.push(format!("history could not be read: {e}"));
                    empty(HistoryDepth::None)
                }
            }
        }
        _ => {
            warnings.push("no git history".to_owned());
            empty(HistoryDepth::None)
        }
    };
    let root = s.root.clone();
    let exists = move |p: &str| {
        aldente_model::RepoPath::parse(p).is_ok()
            && std::fs::symlink_metadata(root.join(p)).is_ok_and(|m| m.is_file())
    };
    let report =
        aldente_analysis::churn::churn(&s.snapshot.graph, &hist, &exists, &ChurnOptions::default());
    (report, warnings)
}

/// `GET /subgraph?id=…`: the named nodes and every edge between them
/// (except `contains`), for showing one cycle on its own.
///
/// Example: `?id=clj:.//a&id=clj:.//b` gives both namespaces and their requires.
pub async fn subgraph(State(s): St, RawQuery(q): RawQuery) -> Response {
    const TAG: &str = "graph";
    let p = Params::parse(q.as_deref());
    let ids: std::collections::BTreeSet<NodeId> =
        p.all("id").iter().filter_map(|i| i.parse().ok()).collect();
    if ids.is_empty() {
        return bad(TAG, "missing id");
    }
    if ids.len() > SUBGRAPH_LIMIT {
        return bad(TAG, "too many ids");
    }
    let g = &s.snapshot.graph;
    let payload = GraphPayload {
        nodes: ids
            .iter()
            .filter_map(|id| g.nodes.get(id).cloned())
            .collect(),
        edges: g
            .edges
            .iter()
            .filter(|e| {
                e.kind != EdgeKind::Contains && ids.contains(&e.from) && ids.contains(&e.to)
            })
            .cloned()
            .collect(),
        truncated: false,
        aggregated: None,
    };
    ok(&s, TAG, payload)
}
