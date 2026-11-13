//! `aldente churn` (needs git and clj-kondo on PATH).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aldente-churn-cli-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("repo/src/a")).unwrap();
    std::fs::write(dir.join("repo/deps.edn"), "{:paths [\"src\"]}\n").unwrap();
    std::fs::write(
        dir.join("repo/src/a/core.clj"),
        "(ns a.core)\n(defn f [] 1)\n",
    )
    .unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aldente"))
        .arg("churn")
        .args(args)
        .arg(dir.join("repo"))
        .env("ALDENTE_CACHE_DIR", dir.join("cache"))
        .env("ALDENTE_MAVEN_REPO", "")
        .output()
        .unwrap()
}

fn git(cwd: &Path, args: &[&str]) {
    let empty = cwd.join("../empty-gitconfig");
    std::fs::write(&empty, "").unwrap();
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", &empty)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// No git history: an empty result with a `no-history` warning, exit 0.
#[test]
fn no_history_is_an_empty_result_with_a_warning() {
    let dir = scratch("nohist");
    let out = run(&dir, &["--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema"], "aldente/churn/v1");
    assert_eq!(v["data"]["history"], "none");
    assert_eq!(v["data"]["files"].as_array().unwrap().len(), 0);
    assert_eq!(v["warnings"][0]["code"], "no-history");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A repository with commits: files counted, text output, bad dates refused.
#[test]
fn history_is_counted_and_dates_are_checked() {
    let dir = scratch("hist");
    let repo = dir.join("repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("src/a/core.clj"), "(ns a.core)\n(defn f [] 2)\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "two"]);
    let out = run(&dir, &["--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["history"], "full");
    assert_eq!(v["data"]["commits"], 2);
    assert_eq!(v["data"]["files"][0]["path"], "src/a/core.clj");
    assert_eq!(v["data"]["files"][0]["commits"], 2);
    assert_eq!(v["data"]["symbols"][0]["id"], "clj:.//a.core/f");
    let text = String::from_utf8(run(&dir, &[]).stdout).unwrap();
    assert!(text.contains("src/a/core.clj"), "{text}");
    let bad = run(&dir, &["--since", "--all"]);
    assert_eq!(bad.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}
