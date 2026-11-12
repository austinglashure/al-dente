//! End-to-end checks of `aldente analyze` (needs clj-kondo on PATH). Each
//! test uses its own fixture copy and cache directory.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A fresh temp directory with a copy of `fixtures/<fixture>` in `repo/` and
/// an empty `cache/`.
///
/// Example: `setup("clj-mini", "json")` gives `/tmp/aldente-cli-analyze-<pid>-json`.
fn setup(fixture: &str, test: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("aldente-cli-analyze-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(fixture),
        &dir.join("repo"),
    );
    fs::create_dir_all(dir.join("cache")).unwrap();
    dir
}

fn copy(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &to);
        } else {
            fs::copy(e.path(), to).unwrap();
        }
    }
}

/// Runs `aldente` with `args` in `dir`'s cache, with an optional `PATH`.
///
/// Example: `run(dir, &["analyze", "--json", repo], None)`.
fn run(dir: &Path, args: &[&str], path: Option<&Path>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_aldente"));
    cmd.args(args).env("ALDENTE_CACHE_DIR", dir.join("cache"));
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    cmd.output().expect("aldente binary should start")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn analyze_clj_mini_then_hit_the_cache() {
    let dir = setup("clj-mini", "cache");
    let repo = dir.join("repo");
    let first = run(&dir, &["analyze", "--json", repo.to_str().unwrap()], None);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let env = json(&first);
    assert_eq!(env["schema"], "aldente/analyze/v1");
    assert_eq!(env["data"]["units"], 1);
    assert_eq!(env["data"]["files"], 13);
    assert_eq!(env["data"]["nodes"]["namespace"], 13);
    assert!(env["snapshot"]["key"].as_str().unwrap().starts_with("b3:"));
    assert_eq!(env["snapshot"]["rev"], Value::Null);
    let validate = |schema: &str, value: &Value| {
        let s = json(&run(&dir, &["schema", schema], None));
        jsonschema::validator_for(&s).unwrap().is_valid(value)
    };
    assert!(validate("envelope", &env));
    assert!(validate("analyze", &env["data"]), "{env:#}");

    let started = std::time::Instant::now();
    let second = run(&dir, &["analyze", "--json", repo.to_str().unwrap()], None);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "cache hit should be fast"
    );
    assert_eq!(second.stdout, first.stdout, "byte-identical output");
    assert!(String::from_utf8_lossy(&second.stderr).contains("using cached snapshot"));

    let text =
        String::from_utf8(run(&dir, &["analyze", repo.to_str().unwrap()], None).stdout).unwrap();
    assert!(text.starts_with("units 1, files 13\n"), "{text}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn no_cache_analyzes_again() {
    let dir = setup("clj-mini", "nocache");
    let repo = dir.join("repo");
    run(&dir, &["analyze", repo.to_str().unwrap()], None);
    let again = run(
        &dir,
        &["analyze", "--no-cache", repo.to_str().unwrap()],
        None,
    );
    assert_eq!(again.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&again.stderr).contains("using cached snapshot"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_clj_kondo_exits_5_with_the_doctor_hint() {
    let dir = setup("clj-mini", "nokondo");
    let empty_path = dir.join("empty-bin");
    fs::create_dir_all(&empty_path).unwrap();
    let repo = dir.join("repo");
    let out = run(
        &dir,
        &["analyze", repo.to_str().unwrap()],
        Some(&empty_path),
    );
    assert_eq!(out.status.code(), Some(5), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("needs clj-kondo"));
    let out = run(
        &dir,
        &["analyze", "--json", repo.to_str().unwrap()],
        Some(&empty_path),
    );
    assert_eq!(json(&out)["error"]["code"], "tool-missing");
    fs::remove_dir_all(dir).unwrap();
}
