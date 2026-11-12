//! End-to-end checks of `aldente units`. Fixtures are copied to the system
//! temp dir first: inside this repository their nearest `.git` would be
//! Al Dente's own.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// Copies `fixtures/<name>` to a fresh temp directory named after the test
/// (tests run in parallel, so each needs its own copy) and returns it.
///
/// Example: `copy_fixture("clj-mini", "json")` gives `/tmp/aldente-cli-units-<pid>-json`.
fn copy_fixture(name: &str, test: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    let dst = std::env::temp_dir().join(format!("aldente-cli-units-{}-{test}", std::process::id()));
    let _ = fs::remove_dir_all(&dst);
    copy_dir(&src, &dst);
    dst
}

/// Recursively copies a directory of plain files.
///
/// Example: `copy_dir("a", "b")` makes `b/x` for every `a/x`.
fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), to).unwrap();
        }
    }
}

/// Runs `aldente` with `args`.
///
/// Example: `run(&["units", "--json", dir])`.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(args)
        .output()
        .expect("aldente binary should start")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

fn schema(name: &str) -> Value {
    json(&run(&["schema", name]))
}

#[test]
fn units_json_on_clj_mini() {
    let dir = copy_fixture("clj-mini", "json");
    let output = run(&["units", "--json", dir.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let env = json(&output);
    assert_eq!(env["schema"], "aldente/units/v1");
    assert_eq!(env["ok"], true);
    assert!(
        jsonschema::validator_for(&schema("envelope"))
            .unwrap()
            .is_valid(&env)
    );
    assert!(
        jsonschema::validator_for(&schema("units"))
            .unwrap()
            .is_valid(&env["data"]),
        "{env:#}"
    );
    let data = &env["data"];
    assert_eq!(data["git"], false);
    assert_eq!(data["units"].as_array().unwrap().len(), 1);
    let unit = &data["units"][0];
    assert_eq!(unit["id"], "unit:.");
    assert_eq!(unit["files"], 13);
    assert_eq!(unit["langs"], serde_json::json!(["clj", "cljs"]));
    assert_eq!(unit["roots"][1]["path"], "test");
    assert_eq!(unit["roots"][1]["kind"], "test");
    assert_eq!(unit["roots"][1]["alias"], "test");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        !text.contains(dir.to_str().unwrap()),
        "no absolute paths in data"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn units_output_is_deterministic() {
    let dir = copy_fixture("mono-mini", "determinism");
    let a = run(&["units", "--json", dir.to_str().unwrap()]);
    let b = run(&["units", "--json", dir.to_str().unwrap()]);
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(json(&a)["data"]["units"].as_array().unwrap().len(), 3);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_subdirectory_resolves_to_the_git_root() {
    let dir = copy_fixture("mono-mini", "subdir");
    fs::create_dir_all(dir.join(".git")).unwrap();
    let sub = dir.join("services/orders/src");
    let env = json(&run(&["units", "--json", sub.to_str().unwrap()]));
    assert_eq!(env["data"]["git"], true);
    assert_eq!(env["data"]["units"].as_array().unwrap().len(), 3);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn units_text_lists_units_roots_and_services() {
    let dir = copy_fixture("mono-mini", "text");
    let output = run(&["units", dir.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("unit:services/billing") && l.contains("service")),
        "{text}"
    );
    assert!(
        lines.iter().any(|l| l.contains("services/orders/test")
            && l.contains("test")
            && l.contains(":test")),
        "{text}"
    );
    assert!(!text.contains("diagnostics"), "{text}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_target_is_a_usage_error() {
    let output = run(&["units", "/definitely/not/here"]);
    assert_eq!(output.status.code(), Some(2));
    let output = run(&["units", "--json", "/definitely/not/here"]);
    assert_eq!(output.status.code(), Some(2));
    let env = json(&output);
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "usage");
}
