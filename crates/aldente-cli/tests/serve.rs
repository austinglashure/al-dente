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
    let mut child = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(["serve", "--no-open", "--json"])
        .arg(dir.join("repo"))
        .env("ALDENTE_CACHE_DIR", dir.join("cache"))
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
    assert_eq!(env["schema"], "aldente/serve/v1");
    let url = env["data"]["url"].as_str().unwrap().to_owned();
    let port = env["data"]["port"].as_u64().unwrap();
    let token = url.split("token=").nth(1).unwrap();
    assert!(url.starts_with(&format!("http://127.0.0.1:{port}/")));

    let request = |headers: &str| {
        let mut s = TcpStream::connect(("127.0.0.1", port as u16)).unwrap();
        write!(s, "GET /api/v1/snapshot HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n{headers}\r\n").unwrap();
        let mut body = String::new();
        s.read_to_string(&mut body).unwrap();
        body
    };
    let ok = request(&format!("x-aldente-token: {token}\r\n"));
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    assert!(ok.contains("\"aldente/snapshot/v1\""));
    assert!(request("").starts_with("HTTP/1.1 401"));
    child.kill().unwrap();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
