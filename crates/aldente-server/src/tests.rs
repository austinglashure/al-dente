//! The server over a real loopback socket with raw HTTP/1.1 requests.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::str::FromStr;

use aldente_graph::Graph;
use aldente_graph::snapshot::Snapshot;
use aldente_model::{
    Certainty, Edge, EdgeKind, Location, Node, NodeId, Reason, RepoPath, Site, SnapshotInfo,
};

use super::*;

/// A handler that always panics, mounted only in tests.
///
/// Example: `GET /api/v1/__panic` gives 500.
pub(crate) async fn panics() -> &'static str {
    panic!("handler panic for tests")
}

/// A handler that computes for two seconds without yielding, as the
/// measure handlers do on a big graph. Mounted only in tests.
///
/// Example: `GET /api/v1/__busy` answers after 2 s.
pub(crate) async fn busy() -> &'static str {
    std::thread::sleep(std::time::Duration::from_secs(2));
    "done"
}

/// More slow requests than worker threads must not stop a cheap one: the
/// GUI's first view hung behind `/hotspots` and `/coverage` on a big graph.
#[test]
fn slow_handlers_do_not_starve_other_requests() {
    let s = start("busy");
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let port = s.port;
    let headers = s.good();
    let slow: Vec<_> = (0..workers + 4)
        .map(|_| {
            let h = headers.clone();
            std::thread::spawn(move || {
                let refs: Vec<(&str, &str)> =
                    h.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                get(port, "/api/v1/__busy", &refs)
            })
        })
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let t = std::time::Instant::now();
    let (status, _, _) = s.ok_get("/api/v1/snapshot");
    assert_eq!(status, 200);
    assert!(
        t.elapsed() < std::time::Duration::from_millis(1000),
        "{:?}",
        t.elapsed()
    );
    for h in slow {
        assert_eq!(h.join().unwrap().0, 200);
    }
}

/// Test-only middleware: a graph envelope over the payload cap becomes a
/// 500 naming the overflow, so any test that reaches it fails.
///
/// Example: a `/graph` answer with 2,001 nodes gives 500.
pub(crate) async fn cap_check(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let res = next.run(req).await;
    let (parts, body) = res.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "unreadable body",
        )
            .into_response();
    };
    if let Some(msg) = cap_violation(&bytes) {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, msg).into_response();
    }
    axum::response::Response::from_parts(parts, axum::body::Body::from(bytes))
}

/// What is over the cap in a graph envelope, if anything.
///
/// Example: `{"schema":"aldente/graph/v1","data":{"nodes":[…2001…]}}` gives
/// `Some("payload cap exceeded: 2001 nodes, 0 edges")`.
fn cap_violation(body: &[u8]) -> Option<String> {
    use aldente_graph::rollup::{MAX_EDGES, MAX_NODES};
    let env: serde_json::Value = serde_json::from_slice(body).ok()?;
    if !env["schema"].as_str()?.starts_with("aldente/graph/") {
        return None;
    }
    let len = |k: &str| env["data"][k].as_array().map_or(0, Vec::len);
    let (n, e) = (len("nodes"), len("edges"));
    (n > MAX_NODES || e > MAX_EDGES).then(|| format!("payload cap exceeded: {n} nodes, {e} edges"))
}

#[test]
fn cap_violation_spots_oversized_graph_envelopes_only() {
    let nodes = vec![serde_json::json!({}); 2001];
    let over = serde_json::json!({"schema": "aldente/graph/v1", "data": {"nodes": nodes}});
    assert!(cap_violation(over.to_string().as_bytes()).is_some());
    let crater = serde_json::json!({"schema": "aldente/crater/v1", "data": {"nodes": nodes}});
    assert!(cap_violation(crater.to_string().as_bytes()).is_none());
}

/// A repository with one file and a two-symbol graph, under the temp dir.
///
/// Example: `fixture("source")` gives the root and a snapshot of `src/a.clj`.
fn fixture(name: &str) -> (PathBuf, Snapshot) {
    let root = std::env::temp_dir().join(format!("aldente-server-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    let src =
        "(ns a)\n\n(defn f\n  \"<script>alert(1)</script>\"\n  [x]\n  (g x))\n\n(defn g [x] x)\n";
    std::fs::write(root.join("src/a.clj"), src).unwrap();
    std::fs::write(root.join("secret.txt"), "do not serve").unwrap();
    let root = root.canonicalize().unwrap();
    let file = RepoPath::parse("src/a.clj").unwrap();
    let mut graph = Graph::default();
    for (id, line, end) in [("clj:.//a/f", 3, 6), ("clj:.//a/g", 8, 8)] {
        let id = NodeId::from_str(id).unwrap();
        graph.nodes.insert(
            id.clone(),
            Node {
                name: id.to_string().rsplit('/').next().unwrap().into(),
                kind: id.kind(),
                id,
                subkind: None,
                namespace: Some("a".into()),
                unit: Some(NodeId::from_str("unit:.").unwrap()),
                langs: BTreeSet::new(),
                flags: BTreeSet::new(),
                loc: Some(Location {
                    file: file.clone(),
                    line,
                    col: 1,
                    end_line: end,
                    end_col: 10,
                }),
                doc: Some("<script>alert(1)</script>".into()),
                signature: None,
                members: None,
            },
        );
    }
    let ns = NodeId::from_str("clj:.//a").unwrap();
    graph.nodes.insert(
        ns.clone(),
        Node {
            id: ns.clone(),
            kind: ns.kind(),
            name: "a".into(),
            subkind: None,
            namespace: None,
            unit: Some(NodeId::from_str("unit:.").unwrap()),
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            // As the analyzer gives it: the `ns` form's location.
            loc: Some(Location {
                file: file.clone(),
                line: 1,
                col: 1,
                end_line: 1,
                end_col: 7,
            }),
            doc: None,
            signature: None,
            members: None,
        },
    );
    for child in ["clj:.//a/f", "clj:.//a/g"] {
        graph.edges.push(Edge {
            from: ns.clone(),
            to: NodeId::from_str(child).unwrap(),
            kind: EdgeKind::Contains,
            certainty: None,
            reason: None,
            lang: None,
            sites: vec![],
            count: None,
            by_certainty: None,
        });
    }
    graph.edges.push(Edge {
        from: NodeId::from_str("clj:.//a/f").unwrap(),
        to: NodeId::from_str("clj:.//a/g").unwrap(),
        kind: EdgeKind::Calls,
        certainty: Some(Certainty::Certain),
        reason: Some(Reason::Direct),
        lang: None,
        sites: vec![Site {
            file: file.clone(),
            line: 6,
            col: 3,
        }],
        count: None,
        by_certainty: None,
    });
    if name == "libs" {
        // `g` calls react through an npm require (WP3.6).
        let lib = NodeId::from_str("lib:npm:react").unwrap();
        let ext = NodeId::from_str("ext:clj//react").unwrap();
        let call = NodeId::from_str("ext:clj//react/createElement").unwrap();
        for id in [&lib, &ext, &call] {
            graph.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    kind: id.kind(),
                    name: id.to_string(),
                    subkind: None,
                    namespace: None,
                    unit: None,
                    langs: BTreeSet::new(),
                    flags: BTreeSet::new(),
                    loc: None,
                    doc: None,
                    signature: None,
                    members: None,
                },
            );
        }
        for (from, to, kind) in [
            (lib.clone(), ext.clone(), EdgeKind::Provides),
            (
                NodeId::from_str("clj:.//a/g").unwrap(),
                call.clone(),
                EdgeKind::Calls,
            ),
        ] {
            graph.edges.push(Edge {
                from,
                to,
                kind,
                certainty: Some(Certainty::Certain),
                reason: Some(Reason::Direct),
                lang: None,
                sites: vec![],
                count: None,
                by_certainty: None,
            });
        }
    }
    let hash = blake3::hash(src.as_bytes()).to_hex().to_string();
    let snap = Snapshot {
        key: "b3:00".into(),
        repo: root.display().to_string(),
        rev: None,
        git: false,
        analyzer: "test".into(),
        units: Vec::new(),
        files: vec![(file, hash)],
        graph,
    };
    (root, snap)
}

/// A running server: its port and token. The runtime lives on a background thread.
struct Running {
    port: u16,
    token: String,
}

/// Starts a server for `fixture(name)` on a free port.
///
/// Example: `start("guard")`.
fn start(name: &str) -> Running {
    let (root, snap) = fixture(name);
    start_with(root, snap)
}

/// A synthetic snapshot over the payload cap at symbol and namespace level,
/// with one 1,000-symbol namespace.
///
/// Example: `big("cap")` gives 2,500 namespaces in 4 units.
fn big(name: &str) -> (PathBuf, Snapshot) {
    let root = std::env::temp_dir().join(format!("aldente-server-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let spec = bench::synth::Spec {
        units: 4,
        namespaces: 2_500,
        symbols: 12_000,
        edges: 40_000,
        hubs: 3,
        big_namespace: 1_000,
        seed: 7,
    };
    let corpus = bench::synth::synthesize(&spec).unwrap();
    (
        root.canonicalize().unwrap(),
        bench::out::to_snapshot(&spec, corpus),
    )
}

/// Starts a server for a snapshot on a free port.
///
/// Example: `start_with(root, snap)`.
fn start_with(root: PathBuf, snap: Snapshot) -> Running {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let info = SnapshotInfo {
            key: snap.key.clone(),
            repo: snap.repo.clone(),
            rev: None,
            dirty: false,
            analyzer: "test".into(),
        };
        let _ = run(
            move |port| AppState::new(snap, root, info, &[], "t0k3n".to_owned(), port),
            0,
            |url| {
                let port: u16 = url
                    .trim_start_matches("http://127.0.0.1:")
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                tx.send(port).unwrap();
            },
        );
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    Running {
        port,
        token: "t0k3n".into(),
    }
}

/// A raw GET; returns status, lowercased header block, body.
///
/// Example: `get(port, "/api/v1/snapshot", &[("Host", "127.0.0.1:1"), …])`.
fn get(port: u16, path: &str, headers: &[(&str, &str)]) -> (u16, String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut req = format!("GET {path} HTTP/1.1\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    stream.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap();
    (status, head.to_lowercase(), body.to_owned())
}

impl Running {
    /// The headers a legitimate UI request carries.
    fn good(&self) -> Vec<(String, String)> {
        vec![
            ("Host".into(), format!("127.0.0.1:{}", self.port)),
            ("x-aldente-token".into(), self.token.clone()),
        ]
    }

    /// A GET with the legitimate headers.
    fn ok_get(&self, path: &str) -> (u16, String, String) {
        let h = self.good();
        let refs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        get(self.port, path, &refs)
    }
}

/// Hostile case 9: a wrong `Host`, a missing token and a foreign `Origin`
/// are each rejected; the legitimate request passes.
#[test]
fn hostile_9_host_token_and_origin_are_checked() {
    let s = start("guard");
    let host = format!("127.0.0.1:{}", s.port);
    let (status, head, _) = s.ok_get("/api/v1/snapshot");
    assert_eq!(status, 200);
    assert!(
        head.contains("content-security-policy: default-src 'self'"),
        "{head}"
    );
    assert!(head.contains("x-content-type-options: nosniff"));
    assert!(
        !head.contains("access-control-allow-origin"),
        "no CORS headers"
    );

    let wrong_host = get(
        s.port,
        "/api/v1/snapshot",
        &[("Host", "evil.example"), ("x-aldente-token", &s.token)],
    );
    assert_eq!(wrong_host.0, 403);
    let rebinding = get(
        s.port,
        "/",
        &[("Host", &format!("evil.example:{}", s.port))],
    );
    assert_eq!(rebinding.0, 403, "static files check Host too");
    let no_token = get(s.port, "/api/v1/snapshot", &[("Host", &host)]);
    assert_eq!(no_token.0, 401);
    let bad_token = get(
        s.port,
        "/api/v1/snapshot",
        &[("Host", &host), ("x-aldente-token", "guess")],
    );
    assert_eq!(bad_token.0, 401);
    let foreign = get(
        s.port,
        "/api/v1/snapshot",
        &[
            ("Host", &host),
            ("x-aldente-token", &s.token),
            ("Origin", "http://evil.example"),
        ],
    );
    assert_eq!(foreign.0, 403);
    let own_origin = get(
        s.port,
        "/api/v1/snapshot",
        &[
            ("Host", &host),
            ("x-aldente-token", &s.token),
            ("Origin", &format!("http://{host}")),
        ],
    );
    assert_eq!(own_origin.0, 200);
}

#[test]
fn source_serves_only_snapshot_files_by_node_id() {
    let s = start("source");
    let (status, head, body) = s.ok_get("/api/v1/source?id=clj%3A.%2F%2Fa%2Ff&context=0");
    assert_eq!(status, 200);
    assert!(head.contains("content-type: text/plain"), "{head}");
    assert_eq!(
        body,
        "(defn f\n  \"<script>alert(1)</script>\"\n  [x]\n  (g x))\n"
    );
    for crafted in [
        "src%2Fa.clj",
        "secret.txt",
        "..%2F..%2Fetc%2Fpasswd",
        "clj%3A.%2F%2F..%2F..%2Fsecret.txt",
        "%2Fetc%2Fpasswd",
    ] {
        let (status, _, body) = s.ok_get(&format!("/api/v1/source?id={crafted}"));
        assert!(status == 404 || status == 400, "{crafted}: {status}");
        assert!(!body.contains("do not serve"), "{crafted}");
    }
}

#[test]
fn a_handler_panic_is_a_500_and_the_server_keeps_serving() {
    let s = start("panic");
    assert_eq!(s.ok_get("/api/v1/__panic").0, 500);
    assert_eq!(s.ok_get("/api/v1/snapshot").0, 200);
}

/// API responses match the same schemas as the CLI's output.
#[test]
fn responses_match_the_cli_schemas() {
    let s = start("schemas");
    for (path, schema, tag) in [
        ("/api/v1/snapshot", "snapshot", "aldente/snapshot/v1"),
        ("/api/v1/graph?level=symbol", "graph", "aldente/graph/v1"),
        (
            "/api/v1/neighborhood?id=f&depth=1",
            "graph",
            "aldente/graph/v1",
        ),
        (
            "/api/v1/expand?id=clj%3A.%2F%2Fa",
            "graph",
            "aldente/graph/v1",
        ),
        ("/api/v1/node?id=g", "show", "aldente/show/v1"),
        ("/api/v1/search?q=f", "find", "aldente/find/v1"),
        ("/api/v1/blindspots", "blindspots", "aldente/blindspots/v1"),
        ("/api/v1/libs", "libs", "aldente/libs/v1"),
    ] {
        let (status, _, body) = s.ok_get(path);
        assert_eq!(status, 200, "{path}: {body}");
        let env: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(env["schema"], tag, "{path}");
        let envelope = aldente_model::schema::envelope_schema();
        assert!(
            jsonschema::validator_for(&envelope).unwrap().is_valid(&env),
            "{path}"
        );
        let data = aldente_model::schema::schema_by_name(schema).unwrap();
        assert!(
            jsonschema::validator_for(&data)
                .unwrap()
                .is_valid(&env["data"]),
            "{path}: {env:#}"
        );
    }
    assert_eq!(s.ok_get("/api/v1/lib?id=nothing-like-this").0, 404);
    assert_eq!(s.ok_get("/api/v1/lib").0, 400);
    let (status, _, body) = s.ok_get("/api/v1/node?id=nothing-like-this");
    assert_eq!(status, 404);
    assert!(body.contains("unknown-symbol"));
    let (status, _, _) = s.ok_get("/api/v1/nope");
    assert_eq!(status, 404);
}

#[test]
fn tree_and_tree_graph() {
    let s = start("tree");
    let (_, _, body) = s.ok_get("/api/v1/tree");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["schema"], "aldente/tree/v1", "{env:#}");
    let root = &env["data"];
    assert_eq!(root["label"], ".");
    assert_eq!(root["dirs"][0]["id"], "dir:src");
    assert_eq!(root["dirs"][0]["files"][0]["name"], "a.clj");
    assert_eq!(root["dirs"][0]["files"][0]["status"], "analyzed");

    let (_, _, body) = s.ok_get("/api/v1/tree/graph?collapsed=dir:src");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    let ids: Vec<&str> = env["data"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["dir:src"], "{env:#}");

    let (_, _, body) = s.ok_get("/api/v1/tree/graph?top=1");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(env["data"]["nodes"].as_array().unwrap().len() <= 1);

    let (status, _, _) = s.ok_get("/api/v1/tree/graph?collapsed=clj:.//a");
    assert_eq!(status, 400, "only dir: IDs can be collapsed");
}

#[test]
fn graph_top_keeps_the_level_and_the_busiest_nodes() {
    let s = start("top");
    let (_, _, body) = s.ok_get("/api/v1/graph?level=symbol&top=1");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["nodes"].as_array().unwrap().len(), 1, "{env:#}");
    assert_eq!(env["data"]["truncated"], true);
    assert!(
        env["data"]["aggregated"].is_null(),
        "no coarser level: {env:#}"
    );
    assert!(env["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn neighborhood_and_graph_payloads() {
    let s = start("payloads");
    let (_, _, body) = s.ok_get("/api/v1/neighborhood?id=g&direction=in");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(env["data"]["truncated"], false);
    let (_, _, body) = s.ok_get("/api/v1/graph?level=symbol&min_certainty=certain&kinds=calls");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["edges"].as_array().unwrap().len(), 1);
}

#[test]
fn every_edge_endpoint_honors_min_certainty() {
    let s = start("mincert");
    for path in [
        "/api/v1/graph?level=symbol&min_certainty=certain",
        "/api/v1/neighborhood?id=f&min_certainty=certain",
        "/api/v1/expand?id=clj%3A.%2F%2Fa&min_certainty=certain",
    ] {
        let (status, _, body) = s.ok_get(path);
        assert_eq!(status, 200, "{path}");
        assert!(
            !body.contains("\"likely\"") && !body.contains("\"possible\""),
            "{path}"
        );
    }
    let (status, _, _) = s.ok_get("/api/v1/node?id=g&min_certainty=likely");
    assert_eq!(status, 200);
}

#[test]
fn the_ui_is_served_with_a_strict_policy() {
    let s = start("ui");
    let (status, head, body) = get(s.port, "/", &[("Host", &format!("localhost:{}", s.port))]);
    assert_eq!(status, 200);
    assert!(head.contains("content-type: text/html"));
    assert!(head.contains("frame-ancestors 'none'"));
    assert!(body.contains("<html"));
}

#[test]
fn tokens_are_long_and_differ() {
    let a = new_token();
    assert_eq!(a.len(), 32);
    assert_ne!(a, new_token());
}

#[test]
fn library_endpoints() {
    let s = start("libs");
    let (status, _, body) = s.ok_get("/api/v1/libs");
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["libraries"][0]["id"], "lib:npm:react");
    assert_eq!(env["data"]["libraries"][0]["symbols"], 1);
    let (status, _, body) = s.ok_get("/api/v1/lib?id=react&by=symbol");
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["schema"], "aldente/lib-usage/v1");
    assert_eq!(env["data"]["groups"][0]["id"], "clj:.//a/g");
    let data = aldente_model::schema::schema_by_name("lib-usage").unwrap();
    assert!(
        jsonschema::validator_for(&data)
            .unwrap()
            .is_valid(&env["data"])
    );
    assert_eq!(s.ok_get("/api/v1/lib?id=react&by=nope").0, 400);
}

/// A raw POST with a text body; returns status and body.
///
/// Example: `post(port, "/api/v1/trace/normalize", &headers, "java.lang.Error: x\n…")`.
fn post(port: u16, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nConnection: close\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap();
    (status, body.to_owned())
}

fn schema_ok(name: &str, data: &serde_json::Value) -> bool {
    let schema = aldente_model::schema::schema_by_name(name).unwrap();
    jsonschema::validator_for(&schema).unwrap().is_valid(data)
}

#[test]
fn crater_checklist_and_trace_endpoints() {
    let s = start("crater");
    let (status, _, body) = s.ok_get("/api/v1/crater?id=f&min_certainty=possible");
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["schema"], "aldente/crater/v1");
    assert!(schema_ok("crater", &env["data"]), "{env:#}");
    let ids: Vec<&str> = env["data"]["reach"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["clj:.//a/f", "clj:.//a/g"]);

    let (status, _, body) = s.ok_get("/api/v1/checklist?id=g&direction=reverse");
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(schema_ok("checklist", &env["data"]), "{env:#}");
    assert_eq!(env["data"]["rows"].as_array().unwrap().len(), 2);
    assert_eq!(s.ok_get("/api/v1/crater").0, 400);
    assert_eq!(s.ok_get("/api/v1/crater?id=nothing-like-this").0, 404);

    let h = s.good();
    let refs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let trace =
        "java.lang.Error: boom\n\tat a$g.invokeStatic(a.clj:8)\n\tat a$f.invokeStatic(a.clj:6)\n";
    let (status, body) = post(s.port, "/api/v1/trace/normalize", &refs, trace);
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(schema_ok("trace", &env["data"]), "{env:#}");
    assert_eq!(env["data"]["traces"][0]["frames"][0]["node"], "clj:.//a/g");
    assert_eq!(env["data"]["traces"][0]["frames"][0]["match"], "exact");

    let (status, body) = post(s.port, "/api/v1/crater/trace", &refs, trace);
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(schema_ok("crater", &env["data"]), "{env:#}");
    assert_eq!(env["data"]["direction"], "reverse");
    let path: Vec<&str> = env["data"]["pinned"]["path"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(path, ["clj:.//a/f", "clj:.//a/g"]);
    assert_eq!(
        post(s.port, "/api/v1/trace/normalize", &refs, "no trace here").0,
        400
    );
    // Without the token the POST is refused like any API call.
    assert_eq!(
        post(s.port, "/api/v1/trace/normalize", &refs[..1], trace).0,
        401
    );
}

/// WP4.6 endpoints: each returns its CLI schema; `/subgraph` keeps only
/// edges among the named nodes; churn without git is empty with a warning.
#[test]
fn measure_coverage_hotspot_cycle_churn_and_subgraph_endpoints() {
    let s = start("measures");
    for (path, schema) in [
        ("/api/v1/metrics", "metrics"),
        ("/api/v1/metrics?level=namespace", "metrics"),
        ("/api/v1/coverage", "coverage"),
        ("/api/v1/hotspots", "hotspots"),
        ("/api/v1/cycles?level=symbol", "cycles"),
        ("/api/v1/churn", "churn"),
    ] {
        let (status, _, body) = s.ok_get(path);
        assert_eq!(status, 200, "{path}: {body}");
        let env: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(schema_ok(schema, &env["data"]), "{path}: {env:#}");
    }
    let (_, _, body) = s.ok_get("/api/v1/churn");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["history"], "none");
    assert_eq!(env["warnings"][0]["code"], "no-history");
    let (status, _, body) = s.ok_get("/api/v1/subgraph?id=clj:.//a/f&id=clj:.//a/g");
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["nodes"].as_array().unwrap().len(), 2);
    assert!(
        env["data"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["kind"] != "contains")
    );
    assert_eq!(s.ok_get("/api/v1/subgraph").0, 400);
    assert_eq!(
        s.ok_get("/api/v1/metrics?level=bogus").0,
        200,
        "unknown level falls back to symbol"
    );
}

/// `/defaults`: the hotspot rules the server uses (the built-in file) and
/// the analyzer defaults the Help guide quotes, so it never hard-codes them.
#[test]
fn defaults_endpoint_serves_the_rules_and_analyzer_defaults() {
    let s = start("defaults");
    let (status, _, body) = s.ok_get("/api/v1/defaults");
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["schema"], "aldente/defaults/v1", "{env:#}");
    let data = &env["data"];
    let builtin =
        serde_json::to_value(aldente_rules::hotspots::HotspotRules::builtin().unwrap()).unwrap();
    assert_eq!(data["hotspots"], builtin);
    assert_eq!(data["hotspots"]["risk"]["fan-in"], 0.3);
    assert_eq!(
        data["coverage_depth"],
        aldente_analysis::coverage::DEFAULT_DEPTH
    );
    assert_eq!(data["certainty_weights"]["likely"], 0.6);
    assert_eq!(data["certainty_weights"]["possible"], 0.25);
}

/// A raw POST with the legitimate headers; returns status and body.
///
/// Example: `s.ok_post("/api/v1/expand?id=a", "[]")`.
impl Running {
    fn ok_post(&self, path: &str, body: &str) -> (u16, String) {
        let h = self.good();
        let refs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        post(self.port, path, &refs, body)
    }
}

/// The node IDs of a graph envelope, in order.
///
/// Example: `node_ids(&body)` is `["clj:.//a/f", "clj:.//a"]`.
fn node_ids(body: &str) -> Vec<String> {
    let env: serde_json::Value = serde_json::from_str(body).unwrap();
    env["data"]["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("{env:#}"))
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_owned())
        .collect()
}

/// WP5.2: `/expand` pages with `offset` and `limit`; children not loaded
/// are the aggregate itself; `POST` takes the visible IDs as a JSON array.
#[test]
fn expand_pages_and_takes_visible_by_post() {
    let s = start("expand-pages");
    let a = "clj%3A.%2F%2Fa";
    let (status, _, body) = s.ok_get(&format!("/api/v1/expand?id={a}&limit=1"));
    assert_eq!(status, 200, "{body}");
    let first = node_ids(&body);
    assert_eq!(first.len(), 2, "one child and the rest: {first:?}");
    assert_eq!(first[1], "clj:.//a");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["truncated"], true);
    assert_eq!(env["data"]["nodes"][1]["members"], 2);
    assert!(schema_ok("graph", &env["data"]), "{env:#}");
    let visible = serde_json::to_string(&[first[0].as_str(), "clj:.//a"]).unwrap();
    let (status, body) = s.ok_post(&format!("/api/v1/expand?id={a}&offset=1&limit=1"), &visible);
    assert_eq!(status, 200, "{body}");
    let second = node_ids(&body);
    assert_eq!(second.len(), 1, "the last child, no rest: {second:?}");
    assert_ne!(second[0], first[0]);
    let (status, _) = s.ok_post(&format!("/api/v1/expand?id={a}"), "not json");
    assert_eq!(status, 400);
}

/// WP5.2: `level=auto` gives the finest level that fits the cap and says
/// which (`aggregated.level`).
#[test]
fn graph_level_auto_picks_the_finest_level_that_fits() {
    let s = start("auto");
    let (_, _, body) = s.ok_get("/api/v1/graph?level=auto");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["aggregated"]["level"], "symbol", "{env:#}");
    assert!(env["warnings"].as_array().unwrap().is_empty());
    let (root, snap) = big("auto-big");
    let s = start_with(root, snap);
    let (_, _, body) = s.ok_get("/api/v1/graph?level=auto");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["aggregated"]["level"], "unit", "{env:#}");
    assert_eq!(env["data"]["truncated"], false);
}

/// WP5.2: on a corpus over the cap, no graph response exceeds it (the
/// test-only cap check turns a violation into a 500), the big namespace
/// pages, and the rollups are warm without any request.
#[test]
fn no_graph_response_exceeds_the_payload_cap() {
    let (root, snap) = big("cap");
    let biggest = snap
        .graph
        .nodes
        .values()
        .filter(|n| n.kind == aldente_model::NodeKind::Namespace)
        .max_by_key(|n| {
            snap.graph
                .edges
                .iter()
                .filter(|e| e.kind == EdgeKind::Contains && e.from == n.id)
                .count()
        })
        .unwrap()
        .id
        .to_string();
    let s = start_with(root, snap);
    let enc = |id: &str| id.replace(':', "%3A").replace('/', "%2F");
    let big_ns = enc(&biggest);
    let mut paths: Vec<String> = [
        "/api/v1/graph",
        "/api/v1/graph?level=symbol",
        "/api/v1/graph?level=namespace",
        "/api/v1/graph?level=unit",
        "/api/v1/graph?level=auto",
        "/api/v1/graph?level=namespace&top=5000",
        "/api/v1/graph?level=symbol&scope=repo",
        "/api/v1/tree/graph",
    ]
    .iter()
    .map(|p| (*p).to_owned())
    .collect();
    paths.push(format!("/api/v1/expand?id={big_ns}&limit=100000"));
    paths.push(format!("/api/v1/neighborhood?id={big_ns}&depth=10"));
    for path in &paths {
        let (status, _, body) = s.ok_get(path);
        assert_eq!(status, 200, "{path}: {}", &body[..body.len().min(300)]);
    }
    // The 1,000 children come in pages of at most 200.
    let mut loaded: Vec<String> = Vec::new();
    loop {
        let visible = serde_json::to_string(&loaded).unwrap();
        let path = format!("/api/v1/expand?id={big_ns}&offset={}", loaded.len());
        let (status, body) = s.ok_post(&path, &visible);
        assert_eq!(status, 200, "{path}");
        let ids: Vec<String> = node_ids(&body)
            .into_iter()
            .filter(|id| id != &biggest)
            .collect();
        assert!(ids.len() <= 200 && !ids.is_empty(), "{path}: {}", ids.len());
        loaded.extend(ids);
        let env: serde_json::Value = serde_json::from_str(&body).unwrap();
        if env["data"]["truncated"] == false {
            break;
        }
    }
    assert_eq!(loaded.len(), 1_000);
}

/// WP5.2: the namespace and unit rollups and the edge index are computed
/// when the server starts, not on the first request.
#[test]
fn rollups_are_warmed_at_start() {
    let state = Arc::new({
        let (root, snap) = fixture("warm");
        let info = SnapshotInfo {
            key: snap.key.clone(),
            repo: snap.repo.clone(),
            rev: None,
            dirty: false,
            analyzer: "test".into(),
        };
        AppState::new(snap, root, info, &[], "t".into(), 1)
    });
    assert!(!state.cache.is_warm());
    warm(Arc::clone(&state)).join().unwrap();
    assert!(state.cache.is_warm());
}

/// WP5.4: `/source?file=&line=` serves the lines around a call site of a
/// snapshot file; both forms say which line the text starts at.
#[test]
fn source_serves_sites_and_says_where_it_starts() {
    let s = start("site-source");
    let (status, head, body) = s.ok_get("/api/v1/source?file=src/a.clj&line=6&context=1");
    assert_eq!(status, 200, "{body}");
    assert!(head.contains("x-aldente-first-line: 5"), "{head}");
    assert_eq!(body, "  [x]\n  (g x))\n\n");
    let (_, head, _) = s.ok_get("/api/v1/source?id=g&context=0");
    assert!(head.contains("x-aldente-first-line: 8"), "{head}");
    for bad in [
        "file=secret.txt&line=1",
        "file=../etc/passwd&line=1",
        "file=src/a.clj",
    ] {
        let (status, _, _) = s.ok_get(&format!("/api/v1/source?{bad}"));
        assert!(status == 404 || status == 400, "{bad}: {status}");
    }
}

/// The stand-in editor for `/open` (any OS, no shell): run as a program
/// with an `ALDENTE_ARGV_OUT=<path>` argument (to the test harness just
/// another name filter), it writes its arguments to that path. In a normal
/// test run it does nothing.
#[test]
fn argv_recorder() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(out) = args
        .iter()
        .find_map(|a| a.strip_prefix("ALDENTE_ARGV_OUT="))
    {
        std::fs::write(out, serde_json::to_string(&args).unwrap()).unwrap();
    }
}

/// A server whose snapshot has a file named with spaces (and quotes where
/// the OS allows), a symbol in it, and a symbol in a file the snapshot does
/// not list; `editor` as given.
fn open_server(name: &str, editor: Option<Vec<String>>) -> (Running, PathBuf) {
    let (root, mut snap) = fixture(name);
    let file_name = if cfg!(windows) {
        "it's a file.clj"
    } else {
        "it's a \"file\".clj"
    };
    std::fs::write(
        root.join("src").join(file_name),
        "(ns odd)\n(defn h [] 1)\n",
    )
    .unwrap();
    let rel = RepoPath::parse(&format!("src/{file_name}")).unwrap();
    snap.files.push((
        rel.clone(),
        blake3::hash(b"(ns odd)\n(defn h [] 1)\n")
            .to_hex()
            .to_string(),
    ));
    let mut add = |id: &str, file: RepoPath| {
        let id = NodeId::from_str(id).unwrap();
        let mut n = snap.graph.nodes.values().next().unwrap().clone();
        n.id = id.clone();
        n.name = id.to_string().rsplit('/').next().unwrap().into();
        n.loc = Some(Location {
            file,
            line: 2,
            col: 1,
            end_line: 2,
            end_col: 14,
        });
        snap.graph.nodes.insert(id, n);
    };
    add("clj:.//odd/h", rel);
    add(
        "clj:.//odd/unlisted",
        RepoPath::parse("secret.txt").unwrap(),
    );
    let info = SnapshotInfo {
        key: snap.key.clone(),
        repo: snap.repo.clone(),
        rev: None,
        dirty: false,
        analyzer: "test".into(),
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let r = root.clone();
    std::thread::spawn(move || {
        let _ = run(
            move |port| AppState {
                editor,
                ..AppState::new(snap, r, info, &[], "t0k3n".to_owned(), port)
            },
            0,
            |url| {
                let port: u16 = url
                    .trim_start_matches("http://127.0.0.1:")
                    .split('/')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                tx.send(port).unwrap();
            },
        );
    });
    let port = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    (
        Running {
            port,
            token: "t0k3n".into(),
        },
        root,
    )
}

/// WP5.4: with no editor configured, `/open` only says where the code is.
#[test]
fn open_without_an_editor_gives_the_location() {
    let (s, _) = open_server("open-none", None);
    let (status, body) = s.ok_post("/api/v1/open", r#"{"id": "clj:.//a/f"}"#);
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["opened"], false);
    assert_eq!(env["data"]["location"], "src/a.clj:3:1");
    assert!(schema_ok("open", &env["data"]), "{env:#}");
}

/// WP5.4: the editor command comes only from the user's config; the request
/// can name a node or a site, nothing else; files outside the snapshot are
/// refused; the file name is one argument, never parsed by a shell.
#[test]
fn open_runs_only_the_configured_editor_with_plain_arguments() {
    let out = std::env::temp_dir().join(format!("aldente-argv-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let editor = vec![
        std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        "--exact".into(),
        "tests::argv_recorder".into(),
        format!("ALDENTE_ARGV_OUT={}", out.display()),
        "{file}".into(),
        "+{line}:{col}".into(),
    ];
    let (s, root) = open_server("open-run", Some(editor));
    for refused in [
        r#"{"id": "clj:.//odd/h", "editor": ["sh", "-c", "touch /tmp/pwned"]}"#,
        r#"{"command": "rm -rf /"}"#,
        r#"{"id": "clj:.//odd/unlisted"}"#,
        r#"{"file": "../outside.clj", "line": 1}"#,
        "not json",
    ] {
        let (status, body) = s.ok_post("/api/v1/open", refused);
        assert!(status == 400 || status == 404, "{refused}: {status} {body}");
    }
    assert!(!out.exists(), "nothing ran for a refused request");
    let (status, body) = s.ok_post("/api/v1/open", r#"{"id": "clj:.//odd/h"}"#);
    assert_eq!(status, 200, "{body}");
    let env: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(env["data"]["opened"], true, "{env:#}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !out.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let args: Vec<String> = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    let file_name = if cfg!(windows) {
        "it's a file.clj"
    } else {
        "it's a \"file\".clj"
    };
    let expected = root
        .join("src")
        .join(file_name)
        .to_string_lossy()
        .into_owned();
    assert_eq!(args[4..], [expected, "+2:1".to_owned()], "{args:?}");
}
