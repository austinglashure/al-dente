//! `aldente trace normalize` on clj-mini (needs clj-kondo on PATH).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs `aldente <args…>` against fixtures/clj-mini with a private cache,
/// feeding `stdin`.
fn run(test: &str, args: &[&str], stdin: &str) -> Output {
    let dir = std::env::temp_dir().join(format!("aldente-trace-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // A copy outside this repository, so the fixture is its own root.
    copy(&root().join("fixtures/clj-mini"), &dir.join("repo"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(args)
        .arg(dir.join("repo"))
        .env("ALDENTE_CACHE_DIR", dir.join("cache"))
        .env("ALDENTE_MAVEN_REPO", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let _ = std::fs::remove_dir_all(dir);
    out
}

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

fn trace_file(name: &str) -> String {
    root()
        .join("fixtures/traces/jvm/clj-mini")
        .join(name)
        .display()
        .to_string()
}

#[test]
fn normalize_text_and_json() {
    let file = trace_file("checkout-book-nil-price.txt");
    let out = run("text", &["trace", "normalize", &file], "");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    insta::assert_snapshot!(
        "trace_normalize_checkout",
        String::from_utf8(out.stdout).unwrap()
    );

    let text = std::fs::read_to_string(&file).unwrap();
    let out = run("json", &["trace", "normalize", "-", "--json"], &text);
    let j: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(j["schema"], "aldente/trace/v1");
    let schema_out = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(["schema", "trace"])
        .output()
        .unwrap();
    let schema: Value = serde_json::from_slice(&schema_out.stdout).unwrap();
    assert!(
        jsonschema::validator_for(&schema)
            .unwrap()
            .is_valid(&j["data"]),
        "{j:#}"
    );
    let frame = &j["data"]["traces"][0]["frames"][3];
    assert_eq!(frame["node"], "clj:.//shop.pricing/half-off");
    assert_eq!(frame["match"], "exact");
}

#[test]
fn input_without_a_trace_is_a_usage_error() {
    let out = run("none", &["trace", "normalize", "-"], "just a log line\n");
    assert_eq!(out.status.code(), Some(2));
}
