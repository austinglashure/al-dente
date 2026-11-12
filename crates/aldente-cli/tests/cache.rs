//! End-to-end checks of `aldente cache` and of URL targets. Each test points
//! `ALDENTE_CACHE_DIR` at its own temp directory. Nothing here uses the network.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A fresh cache directory for one test.
///
/// Example: `cache_dir("ls")` gives `/tmp/aldente-cli-cache-<pid>-ls`, empty.
fn cache_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aldente-cli-cache-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `aldente` with `args` and the cache at `cache`.
///
/// Example: `run(cache, &["cache", "ls", "--json"])`.
fn run(cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(args)
        .env("ALDENTE_CACHE_DIR", cache)
        .output()
        .expect("aldente binary should start")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn valid(cache: &Path, schema: &str, value: &Value) -> bool {
    let schema = json(&run(cache, &["schema", schema]));
    jsonschema::validator_for(&schema).unwrap().is_valid(value)
}

#[test]
fn ls_lists_clones_and_snapshots() {
    let cache = cache_dir("ls");
    let clone = cache.join("repos/github.com/metosin/reitit");
    fs::create_dir_all(clone.join(".git")).unwrap();
    fs::write(clone.join("a.clj"), "(ns a)").unwrap();
    fs::create_dir_all(cache.join("snapshots")).unwrap();
    fs::write(cache.join("snapshots/abcd.db"), "xx").unwrap();

    let out = run(&cache, &["cache", "ls", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let env = json(&out);
    assert_eq!(env["schema"], "aldente/cache/v1");
    assert!(valid(&cache, "cache", &env["data"]), "{env:#}");
    assert_eq!(env["data"]["repos"][0]["path"], "github.com/metosin/reitit");
    assert_eq!(env["data"]["snapshots"][0]["key"], "b3:abcd");

    let text = String::from_utf8(run(&cache, &["cache", "ls"]).stdout).unwrap();
    assert!(text.contains("github.com/metosin/reitit"), "{text}");
    assert!(text.contains("b3:abcd"), "{text}");
    fs::remove_dir_all(cache).unwrap();
}

#[test]
fn clear_and_gc_remove_only_cache_contents() {
    let cache = cache_dir("clear");
    fs::create_dir_all(cache.join("repos/h/o/n/.git")).unwrap();
    fs::create_dir_all(cache.join("tmp")).unwrap();
    fs::write(cache.join("tmp/leftover"), "1").unwrap();

    let gc = json(&run(&cache, &["cache", "gc", "--json"]));
    assert_eq!(gc["schema"], "aldente/cache-removal/v1");
    assert!(valid(&cache, "cache-removal", &gc["data"]));
    assert_eq!(gc["data"]["removed"], serde_json::json!(["tmp/leftover"]));
    assert!(cache.join("repos/h/o/n").exists());

    let out = run(&cache, &["cache", "clear"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8(out.stdout).unwrap().contains("repos/h"));
    assert!(!cache.join("repos/h").exists());
    assert!(cache.exists(), "the cache directory itself stays");
    fs::remove_dir_all(cache).unwrap();
}

#[test]
fn dangerous_urls_are_usage_errors_before_git_runs() {
    let cache = cache_dir("urls");
    for url in [
        "file:///etc",
        "ext::sh -c touch% x",
        "https://user:pw@github.com/a/b",
        "https://github.com/a/../b",
    ] {
        let out = run(&cache, &["units", "--json", url]);
        assert_eq!(out.status.code(), Some(2), "{url}: {out:?}");
        assert_eq!(json(&out)["error"]["code"], "usage", "{url}");
    }
    // A leading dash would be read as an option, so pass it after `--`.
    let out = run(&cache, &["units", "--json", "--", "-oProxyCommand=touch x"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        !cache.join("repos").exists()
            || fs::read_dir(cache.join("repos")).unwrap().next().is_none()
    );
    fs::remove_dir_all(cache).unwrap();
}

#[test]
fn ref_is_only_for_urls() {
    let cache = cache_dir("ref");
    let out = run(&cache, &["units", "--ref", "main", "."]);
    assert_eq!(out.status.code(), Some(2));
    let out = run(
        &cache,
        &[
            "units",
            "--json",
            "--ref",
            "--upload-pack=x",
            "https://github.com/a/b",
        ],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    fs::remove_dir_all(cache).unwrap();
}
