//! `aldente lib ls` and `aldente lib usage` on fixtures/clj-rules/libraries
//! (needs clj-kondo on PATH).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A copy of the fixture outside this repository, with its own cache.
struct Env {
    dir: PathBuf,
}

impl Env {
    fn new(test: &str) -> Env {
        let dir = std::env::temp_dir().join(format!("aldente-lib-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/clj-rules/libraries"),
            &dir.join("repo"),
        );
        Env { dir }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aldente"))
            .args(args)
            .arg(self.dir.join("repo"))
            .env("ALDENTE_CACHE_DIR", self.dir.join("cache"))
            .env("ALDENTE_MAVEN_REPO", "")
            .output()
            .unwrap()
    }

    fn text(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
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

fn valid(name: &str, value: &Value) -> bool {
    let out = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(["schema", name])
        .output()
        .unwrap();
    let schema: Value = serde_json::from_slice(&out.stdout).unwrap();
    jsonschema::validator_for(&schema).unwrap().is_valid(value)
}

#[test]
fn lib_ls_and_usage() {
    let env = Env::new("usage");
    insta::assert_snapshot!("lib_ls", env.text(&["lib", "ls"]));
    insta::assert_snapshot!("lib_usage_reagent", env.text(&["lib", "usage", "reagent"]));
    insta::assert_snapshot!(
        "lib_usage_react_by_symbol",
        env.text(&["lib", "usage", "react", "--by", "symbol"])
    );
    let j: Value = serde_json::from_slice(&env.run(&["lib", "ls", "--json"]).stdout).unwrap();
    assert_eq!(j["schema"], "aldente/libs/v1");
    assert!(valid("libs", &j["data"]), "{j:#}");
    let j: Value =
        serde_json::from_slice(&env.run(&["lib", "usage", "reagent", "--json"]).stdout).unwrap();
    assert_eq!(j["schema"], "aldente/lib-usage/v1");
    assert!(valid("lib-usage", &j["data"]), "{j:#}");
    assert_eq!(j["data"]["require_only"][0]["id"], "clj:.//lib.state");
    assert_eq!(env.run(&["lib", "usage", "nope"]).status.code(), Some(3));
}
