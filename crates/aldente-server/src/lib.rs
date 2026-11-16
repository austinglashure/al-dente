//! The local HTTP API and embedded UI (`06-gui.md`, "API"; `07-safety.md`,
//! "The local server").
//!
//! Loopback only. Every request must carry a `Host` of this server, and an
//! `Origin` (if any) of this server; API requests must carry the session
//! token in `x-aldente-token`. Responses carry a strict CSP and no CORS
//! headers. A handler panic becomes a 500 and the server keeps serving.

mod api;
mod params;
mod security;
mod timing;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use aldente_graph::snapshot::Snapshot;
use aldente_model::analyze::AnalyzeSummary;
use aldente_model::{Diagnostic, SnapshotInfo};
use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use rust_embed::RustEmbed;

/// The built UI, `ui/dist` at the repository root.
#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
struct Assets;

/// Everything a request may read. Immutable while serving.
#[derive(Debug)]
pub struct AppState {
    pub snapshot: Snapshot,
    /// Canonical repository root, for `/source`.
    pub root: PathBuf,
    pub info: SnapshotInfo,
    pub summary: AnalyzeSummary,
    pub token: String,
    pub port: u16,
    /// Print one timing line per request to stderr (`serve --timings`).
    pub timings: bool,
    /// Rollups and the edge index of `snapshot.graph`, computed on first use.
    pub cache: aldente_graph::cache::Cache,
    /// The directory tree of the snapshot, built on first use.
    pub tree: std::sync::OnceLock<aldente_model::tree::TreeDir>,
    /// The crater index of `snapshot.graph`, built on first use.
    pub crater: std::sync::OnceLock<aldente_analysis::crater::CraterIndex>,
    /// The static coverage estimate, computed on first use.
    pub coverage: std::sync::OnceLock<aldente_model::coverage::CoverageReport>,
    /// Measures computed so far in this session.
    pub results: aldente_analysis::measures::cache::SharedMemoryStore,
    /// The hotspots report, computed on first use.
    pub hotspots: std::sync::OnceLock<aldente_model::hotspots::HotspotsReport>,
    /// Hardened git for reading history (`/churn`); `None` turns churn off.
    pub git: Option<aldente_acquire::remote::Git>,
    /// The churn report, read on first use.
    pub churn: std::sync::OnceLock<(aldente_model::churn::ChurnReport, Vec<String>)>,
    /// The editor command from the user's config (`06-gui.md`, "Source and
    /// editor links"); `None` means `/open` only returns the location.
    pub editor: Option<Vec<String>>,
}

impl AppState {
    /// The snapshot's directory tree (built once).
    ///
    /// Example: for clj-mini, one row `src/shop` holding the source files.
    pub fn tree(&self) -> &aldente_model::tree::TreeDir {
        self.tree.get_or_init(|| {
            let files: Vec<aldente_model::RepoPath> =
                self.snapshot.files.iter().map(|(p, _)| p.clone()).collect();
            aldente_graph::tree::tree(&self.snapshot.graph, &files)
        })
    }

    /// Builds the state for a loaded snapshot.
    ///
    /// Example: `AppState::new(snap, root, info, &[], token, 8021)`.
    pub fn new(
        snapshot: Snapshot,
        root: PathBuf,
        info: SnapshotInfo,
        extra: &[Diagnostic],
        token: String,
        port: u16,
    ) -> AppState {
        let summary = aldente_graph::summary::summarize(&snapshot, extra);
        AppState {
            snapshot,
            root,
            info,
            summary,
            token,
            port,
            timings: false,
            cache: aldente_graph::cache::Cache::default(),
            tree: std::sync::OnceLock::new(),
            crater: std::sync::OnceLock::new(),
            coverage: std::sync::OnceLock::new(),
            results: Default::default(),
            hotspots: std::sync::OnceLock::new(),
            git: None,
            churn: std::sync::OnceLock::new(),
            editor: None,
        }
    }

    /// The static coverage estimate (computed once).
    ///
    /// Example: for clj-mini, `shop.pricing/price` is `direct`.
    pub fn coverage(&self) -> &aldente_model::coverage::CoverageReport {
        self.coverage.get_or_init(|| {
            aldente_analysis::coverage::coverage(
                &self.snapshot.graph,
                aldente_analysis::coverage::DEFAULT_DEPTH,
                None,
            )
        })
    }

    /// The crater index (built once).
    ///
    /// Example: `state.crater_index().len()` is the number of nodes.
    pub fn crater_index(&self) -> &aldente_analysis::crater::CraterIndex {
        self.crater
            .get_or_init(|| aldente_analysis::crater::CraterIndex::new(&self.snapshot.graph))
    }
}

/// The router: `/api/v1/*` handlers and the embedded UI, behind the guard.
///
/// Example: `router(Arc::new(state))` serves `GET /api/v1/snapshot`.
pub fn router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/api/v1/snapshot", get(api::snapshot))
        .route("/api/v1/graph", get(api::graph))
        .route("/api/v1/neighborhood", get(api::neighborhood))
        .route("/api/v1/expand", get(api::expand).post(api::expand_post))
        .route("/api/v1/node", get(api::node))
        .route("/api/v1/source", get(api::source))
        .route("/api/v1/search", get(api::search))
        .route("/api/v1/blindspots", get(api::blindspots))
        .route("/api/v1/libs", get(api::libs))
        .route("/api/v1/crater", get(api::crater))
        .route("/api/v1/checklist", get(api::checklist))
        .route("/api/v1/trace/normalize", post(api::trace_normalize))
        .route("/api/v1/crater/trace", post(api::crater_trace))
        .route("/api/v1/lib", get(api::lib))
        .route("/api/v1/tree", get(api::tree))
        .route("/api/v1/tree/graph", get(api::tree_graph))
        .route("/api/v1/metrics", get(api::metrics))
        .route("/api/v1/coverage", get(api::coverage))
        .route("/api/v1/hotspots", get(api::hotspots))
        .route("/api/v1/defaults", get(api::defaults))
        .route("/api/v1/cycles", get(api::cycles))
        .route("/api/v1/churn", get(api::churn))
        .route("/api/v1/subgraph", get(api::subgraph))
        .route("/api/v1/open", post(api::open))
        .route("/api/v1/unit-links", get(api::unit_links));
    #[cfg(test)]
    let api = api
        .route("/api/v1/__panic", get(tests::panics))
        .route("/api/v1/__busy", get(tests::busy))
        .layer(axum::middleware::from_fn(tests::cap_check));
    api.fallback(asset)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::guard,
        ))
        .with_state(state)
}

/// Serves an embedded UI file; unknown non-API paths get `index.html`.
///
/// Example: `GET /` gives `index.html`; `GET /assets/app.js` gives that file.
async fn asset(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return api::not_found("no such API endpoint");
    }
    let file = Assets::get(path)
        .map(|f| (path, f))
        .or_else(|| Assets::get("index.html").map(|f| ("index.html", f)));
    match file {
        Some((name, f)) => (
            [(header::CONTENT_TYPE, content_type(name))],
            f.data.into_owned(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// The MIME type for a UI file name.
///
/// Example: `content_type("app.js")` is `"text/javascript; charset=utf-8"`.
fn content_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// A fresh session token: 32 hex digits from the OS-seeded keys behind
/// `std`'s `RandomState`, mixed with the time and process ID through blake3.
///
/// Example: `new_token()` gives something like `"9f2c41…"` (32 characters).
pub fn new_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = blake3::Hasher::new();
    for i in 0..4u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i);
        hasher.update(&h.finish().to_le_bytes());
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.finalize().to_hex()[..32].to_owned()
}

/// Binds `127.0.0.1:port` (0 picks a free port), then calls `ready` with the
/// UI URL (token included) and serves until Ctrl-C.
///
/// Example: `run(state_without_port, 0, |url| open_browser(&url))`.
pub fn run(
    make_state: impl FnOnce(u16) -> AppState,
    port: u16,
    ready: impl FnOnce(&str),
) -> std::io::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener =
            tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).await?;
        let port = listener.local_addr()?.port();
        let state = Arc::new(make_state(port));
        // Rollups are computed beside the first requests, not before the
        // URL is printed; a request that needs one waits for it.
        let _ = warm(Arc::clone(&state));
        let url = format!("http://127.0.0.1:{port}/?token={}", state.token);
        ready(&url);
        axum::serve(listener, router(state))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
    })
}

/// Computes the namespace and unit rollups and the edge index on a thread
/// of its own (`06-gui.md`: aggregated graphs are derived once at load).
///
/// Example: `warm(state.clone()).join()` returns once `state.cache.is_warm()`.
pub fn warm(state: Arc<AppState>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || state.cache.warm(&state.snapshot.graph))
}

/// Opens `url` in the default browser with the platform's opener, as an
/// argument vector (no shell).
///
/// Example: on Linux runs `xdg-open http://127.0.0.1:8021/?token=…`.
pub fn open_browser(url: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(url);
        c
    } else if cfg!(windows) {
        let mut c = Command::new("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests;
