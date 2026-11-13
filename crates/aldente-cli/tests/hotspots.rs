//! `aldente hotspots`, `metrics`, `cycles`, `communities`, `deadcode` on a
//! copy of clj-mini (needs clj-kondo on PATH).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

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

fn setup(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aldente-hotspots-cli-{}-{test}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/clj-mini"),
        &dir.join("repo"),
    );
    dir
}

fn json(dir: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(args)
        .arg("--json")
        .arg(dir.join("repo"))
        .env("ALDENTE_CACHE_DIR", dir.join("cache"))
        .env("ALDENTE_MAVEN_REPO", "")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn kinds(v: &Value, kind: &str) -> usize {
    v["data"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == kind)
        .count()
}

#[test]
fn rules_file_changes_the_findings_without_recompiling() {
    let dir = setup("rules");
    let builtin = json(&dir, &["hotspots"]);
    assert_eq!(builtin["schema"], "aldente/hotspots/v1");
    assert_eq!(builtin["data"]["rules"], "builtin");
    assert_eq!(kinds(&builtin, "dead-code"), 16);
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/hotspots.toml"),
    )
    .unwrap()
    .replace(
        "[dead-code]\nenabled = true",
        "[dead-code]\nenabled = false",
    )
    .replace("percentile = 99.0", "percentile = 50.0");
    let rules = dir.join("my-hotspots.toml");
    std::fs::write(&rules, text).unwrap();
    let custom = json(&dir, &["hotspots", "--rules", rules.to_str().unwrap()]);
    assert_eq!(custom["data"]["rules"], "file");
    assert_eq!(kinds(&custom, "dead-code"), 0);
    assert!(kinds(&custom, "god-object") > kinds(&builtin, "god-object"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn measure_commands_print_their_envelopes() {
    let dir = setup("measures");
    let m = json(&dir, &["metrics"]);
    assert_eq!(m["schema"], "aldente/metrics/v1");
    assert_eq!(
        m["data"]["first"]["nodes"].as_array().unwrap().len(),
        m["data"]["second"]["nodes"].as_array().unwrap().len()
    );
    // Second run reads the kept result: identical data.
    assert_eq!(json(&dir, &["metrics"])["data"], m["data"]);
    let c = json(&dir, &["cycles"]);
    assert_eq!(c["data"]["projection"]["kind"], "structure");
    assert_eq!(c["data"]["components"].as_array().unwrap().len(), 0);
    let k = json(&dir, &["communities", "--seed", "7"]);
    assert_eq!(k["data"]["seed"], 7);
    assert_eq!(
        json(&dir, &["communities", "--seed", "7"])["data"],
        k["data"]
    );
    let d = json(&dir, &["deadcode"]);
    assert_eq!(d["data"]["dead"].as_array().unwrap().len(), 16);
    let _ = std::fs::remove_dir_all(&dir);
}
