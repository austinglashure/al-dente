//! End-to-end checks of `aldente doctor` with a stubbed `PATH`: each test
//! puts fake `git` and `clj-kondo` executables that print a fixed version
//! into its own directory and runs doctor with only that directory on `PATH`.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const GIT_OK: (&str, &str) = ("git", "git version 2.34.1");
const GIT_OLD: (&str, &str) = ("git", "git version 2.20.0");
const KONDO_OK: (&str, &str) = ("clj-kondo", "clj-kondo v2026.10.26");
const KONDO_OLD: (&str, &str) = ("clj-kondo", "clj-kondo v2023.09.07");

/// A fresh directory for one test under Cargo's per-target temp dir.
///
/// Example: `scratch("missing")` gives `target/tmp/doctor-missing`, emptied.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("doctor-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin")).unwrap();
    dir
}

/// Writes an executable stub called `name` into `bin` that prints `line`.
/// Unix: a `/bin/sh` script. Windows: a `.cmd` file, found through `PATHEXT`.
///
/// Example: `stub(bin, ("git", "git version 2.34.1"))` makes `bin/git`.
fn stub(bin: &Path, (name, line): (&str, &str)) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = bin.join(name);
        fs::write(&path, format!("#!/bin/sh\necho '{line}'\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(windows)]
    fs::write(bin.join(format!("{name}.cmd")), format!("@echo {line}\r\n")).unwrap();
}

/// Runs `aldente doctor` (plus `args`) with `PATH` set to `dir/bin` holding
/// `stubs`, and the Linux cache dir pointed into `dir`.
///
/// Example: `doctor("ok", &[GIT_OK, KONDO_OK], &["--json"])` exits 0.
fn doctor(name: &str, stubs: &[(&str, &str)], args: &[&str]) -> Output {
    let dir = scratch(name);
    let bin = dir.join("bin");
    for s in stubs {
        stub(&bin, *s);
    }
    Command::new(env!("CARGO_BIN_EXE_aldente"))
        .arg("doctor")
        .args(args)
        .env("PATH", &bin)
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .output()
        .expect("aldente binary should start")
}

/// Parses stdout as JSON.
///
/// Example: stdout `{"a":1}` gives `json!({"a":1})`.
fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

/// The tool entry called `name` in a doctor envelope.
///
/// Example: `tool(&env, "git")["status"]` is `"ok"`.
fn tool<'a>(envelope: &'a Value, name: &str) -> &'a Value {
    envelope["data"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap()
}

#[test]
fn found_and_new_enough_tools_pass() {
    let output = doctor("found", &[GIT_OK, KONDO_OK], &["--json"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let env = stdout_json(&output);
    assert_eq!(env["schema"], "aldente/doctor/v1");
    assert_eq!(env["ok"], true);
    assert_eq!(env["error"], Value::Null);
    assert_eq!(tool(&env, "git")["status"], "ok");
    assert_eq!(tool(&env, "git")["version"], "2.34.1");
    assert_eq!(tool(&env, "clj-kondo")["status"], "ok");
    assert_eq!(tool(&env, "clj-kondo")["version"], "2026.10.26");
    assert_eq!(tool(&env, "git")["hint"], Value::Null);
    assert!(
        tool(&env, "git")["path"]
            .as_str()
            .unwrap()
            .contains("doctor-found")
    );
    assert_eq!(env["data"]["cache"]["writable"], true);
    assert_eq!(env["data"]["platform"]["os"], std::env::consts::OS);
}

#[test]
fn missing_tools_exit_5_with_hints() {
    let output = doctor("missing", &[], &["--json"]);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    let env = stdout_json(&output);
    assert_eq!(env["ok"], false);
    assert_eq!(env["error"]["code"], "tool-missing");
    for name in ["git", "clj-kondo"] {
        assert_eq!(tool(&env, name)["status"], "missing");
        assert_eq!(tool(&env, name)["path"], Value::Null);
        assert!(
            tool(&env, name)["hint"]
                .as_str()
                .unwrap()
                .starts_with("needs")
        );
    }
}

#[test]
fn one_missing_tool_is_enough_to_fail() {
    let output = doctor("one-missing", &[GIT_OK], &["--json"]);
    assert_eq!(output.status.code(), Some(5));
    let env = stdout_json(&output);
    assert_eq!(tool(&env, "git")["status"], "ok");
    assert_eq!(tool(&env, "clj-kondo")["status"], "missing");
}

#[test]
fn too_old_tools_exit_5() {
    let output = doctor("too-old", &[GIT_OLD, KONDO_OLD], &["--json"]);
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    let env = stdout_json(&output);
    assert_eq!(tool(&env, "git")["status"], "too-old");
    assert_eq!(tool(&env, "git")["version"], "2.20.0");
    assert_eq!(tool(&env, "git")["minimum"], "2.30.0");
    assert_eq!(tool(&env, "clj-kondo")["status"], "too-old");
    assert!(
        tool(&env, "clj-kondo")["hint"]
            .as_str()
            .unwrap()
            .contains("2026.10.26")
    );
}

#[test]
fn unrecognized_version_output_fails() {
    let output = doctor("garbage", &[("git", "hello"), KONDO_OK], &["--json"]);
    assert_eq!(output.status.code(), Some(5));
    let env = stdout_json(&output);
    assert_eq!(tool(&env, "git")["status"], "unknown-version");
    assert!(
        tool(&env, "git")["detail"]
            .as_str()
            .unwrap()
            .contains("hello")
    );
}

#[test]
fn json_output_matches_the_published_schemas() {
    let output = doctor("schema", &[GIT_OLD, KONDO_OK], &["--json"]);
    let env = stdout_json(&output);
    let schema = |name: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_aldente"))
            .args(["schema", name])
            .output()
            .unwrap();
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let envelope = jsonschema::validator_for(&schema("envelope")).unwrap();
    assert!(envelope.is_valid(&env), "{env:#}");
    let data = jsonschema::validator_for(&schema("doctor")).unwrap();
    assert!(data.is_valid(&env["data"]), "{env:#}");
}

#[test]
fn text_output_names_each_check_and_the_fix() {
    let output = doctor("text", &[GIT_OK], &[]);
    assert_eq!(output.status.code(), Some(5));
    let text = String::from_utf8(output.stdout).unwrap();
    let line = |prefix: &str| {
        text.lines()
            .find(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix:?} line in:\n{text}"))
            .to_owned()
    };
    assert!(line("git").contains("ok"), "{text}");
    assert!(line("git").contains("2.34.1"), "{text}");
    assert!(line("clj-kondo").contains("missing"), "{text}");
    assert!(
        text.contains("needs clj-kondo 2026.10.26 or newer"),
        "{text}"
    );
    assert!(line("cache").contains("writable"), "{text}");
    assert!(line("platform").contains(std::env::consts::OS), "{text}");
}
