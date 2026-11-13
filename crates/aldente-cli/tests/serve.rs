//! `aldente serve` end to end: start it on a fixture copy, read the URL,
//! call the API with the token, stop it.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Command, Stdio};

fn copy(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), to).unwrap();
        }
    }
}

#[test]
fn serve_answers_the_api_with_the_printed_token() {
    let dir = std::env::temp_dir().join(format!("aldente-cli-serve-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/clj-mini"),
        &dir.join("repo"),
    );
    let repo = dir.join("repo");
    let (mut child, port, token) = start(
        &[
            "serve".as_ref(),
            "--no-open".as_ref(),
            "--json".as_ref(),
            repo.as_os_str(),
        ],
        &dir.join("cache"),
    );
    let ok = get(port, &token, "/api/v1/snapshot");
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    assert!(ok.contains("\"aldente/snapshot/v1\""));
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "GET /api/v1/snapshot HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut no_token = String::new();
    s.read_to_string(&mut no_token).unwrap();
    assert!(no_token.starts_with("HTTP/1.1 401"), "{no_token}");
    child.kill().unwrap();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Starts `aldente <args>` and reads the `serve` envelope it prints:
/// returns the child, the port and the session token.
fn start(args: &[&std::ffi::OsStr], cache: &Path) -> (std::process::Child, u16, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(args)
        .env("ALDENTE_CACHE_DIR", cache)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut json = String::new();
    loop {
        let mut line = String::new();
        assert!(out.read_line(&mut line).unwrap() > 0, "serve exited early");
        json.push_str(&line);
        if line == "}\n" {
            break;
        }
    }
    let env: serde_json::Value = serde_json::from_str(&json).unwrap();
    let url = env["data"]["url"].as_str().unwrap().to_owned();
    let port = env["data"]["port"].as_u64().unwrap() as u16;
    let token = url.split("token=").nth(1).unwrap().to_owned();
    assert_eq!(env["schema"], "aldente/serve/v1");
    assert!(
        url.starts_with(&format!("http://127.0.0.1:{port}/")),
        "{url}"
    );
    (child, port, token)
}

/// `GET path` with the token; the raw response.
fn get(port: u16, token: &str, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nx-aldente-token: {token}\r\n\r\n").unwrap();
    let mut body = String::new();
    s.read_to_string(&mut body).unwrap();
    body
}

/// A one-unit snapshot file with no source files.
fn tiny_snapshot(path: &Path) {
    use aldente_graph::build::Graph;
    use aldente_graph::snapshot::{Snapshot, write};
    use aldente_model::{Node, NodeId, NodeKind, RepoPath};
    let dir = RepoPath::parse("svc").unwrap();
    let id = NodeId::Unit(dir.clone());
    let mut graph = Graph::default();
    graph.nodes.insert(
        id.clone(),
        Node {
            id,
            kind: NodeKind::Unit,
            subkind: None,
            name: "svc".into(),
            namespace: None,
            unit: None,
            langs: Default::default(),
            flags: Default::default(),
            loc: None,
            doc: None,
            signature: None,
        },
    );
    let snap = Snapshot {
        key: "b3:0123abcd".into(),
        repo: "synthetic: tiny".into(),
        rev: None,
        git: false,
        analyzer: "0.1.0".into(),
        units: Vec::new(),
        files: Vec::new(),
        graph,
    };
    write(path, &snap).unwrap();
}

#[test]
fn serve_opens_a_snapshot_file_without_a_target() {
    let dir = std::env::temp_dir().join(format!("aldente-cli-serve-snap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("synthetic.db");
    tiny_snapshot(&file);
    let (mut child, port, token) = start(
        &[
            "serve".as_ref(),
            "--no-open".as_ref(),
            "--json".as_ref(),
            "--snapshot".as_ref(),
            file.as_os_str(),
        ],
        &dir.join("cache"),
    );
    let snap = get(port, &token, "/api/v1/snapshot");
    assert!(snap.starts_with("HTTP/1.1 200"), "{snap}");
    assert!(snap.contains("synthetic: tiny"), "{snap}");
    let graph = get(port, &token, "/api/v1/graph?level=unit");
    assert!(graph.starts_with("HTTP/1.1 200"), "{graph}");
    assert!(graph.contains("unit:svc"), "{graph}");
    child.kill().unwrap();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_refuses_a_file_that_is_not_a_snapshot() {
    let dir = std::env::temp_dir().join(format!("aldente-cli-serve-bad-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let not_db = dir.join("notes.txt");
    std::fs::write(&not_db, "hello").unwrap();
    for file in [not_db, dir.join("missing.db")] {
        let output = Command::new(env!("CARGO_BIN_EXE_aldente"))
            .args(["serve", "--no-open", "--json", "--snapshot"])
            .arg(&file)
            .env("ALDENTE_CACHE_DIR", dir.join("cache"))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{file:?}");
        let env: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(env["error"]["code"], "usage");
        let message = env["error"]["message"].as_str().unwrap();
        assert!(message.contains("snapshot"), "{message}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
