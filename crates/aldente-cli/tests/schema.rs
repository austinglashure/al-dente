//! End-to-end checks of `aldente schema` against the built binary.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::{Command, Output};

use serde_json::Value;

/// Runs `aldente` with `args`.
///
/// Example: `run(&["schema", "envelope"])` returns the process output.
fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(args)
        .output()
        .expect("aldente binary should start")
}

/// Parses stdout as JSON.
///
/// Example: stdout `{"a":1}` gives `json!({"a":1})`.
fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn schema_envelope_prints_valid_json_schema() {
    let output = run(&["schema", "envelope"]);
    assert_eq!(output.status.code(), Some(0));
    let schema = stdout_json(&output);
    assert!(jsonschema::meta::is_valid(&schema), "{schema:#}");
    assert_eq!(schema["title"], "Envelope");
}

#[test]
fn schema_envelope_json_wraps_the_schema_in_a_valid_envelope() {
    let output = run(&["schema", "envelope", "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["schema"], "aldente/schema/v1");
    assert_eq!(envelope["ok"], true);
    let schema = &envelope["data"];
    assert!(jsonschema::meta::is_valid(schema));
    let validator = jsonschema::validator_for(schema).unwrap();
    assert!(
        validator.is_valid(&envelope),
        "output must match its own schema"
    );
}

#[test]
fn schema_without_name_lists_names() {
    let output = run(&["schema"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "analyze\nblindspots\ncache\ncache-removal\nchecklist\nchurn\ncommunities\ncoverage\ncrater\ncycles\ndeadcode\ndoctor\nenvelope\nfind\ngraph\nhotspots\nlib-usage\nlibs\nmetrics\nopen\nserve\nshow\nsnapshot\ntrace\nunits\n"
    );

    let output = run(&["schema", "--json"]);
    assert_eq!(
        stdout_json(&output)["data"],
        serde_json::json!([
            "analyze",
            "blindspots",
            "cache",
            "cache-removal",
            "checklist",
            "churn",
            "communities",
            "coverage",
            "crater",
            "cycles",
            "deadcode",
            "doctor",
            "envelope",
            "find",
            "graph",
            "hotspots",
            "lib-usage",
            "libs",
            "metrics",
            "open",
            "serve",
            "show",
            "snapshot",
            "trace",
            "units",
        ])
    );
}

#[test]
fn unknown_schema_is_a_usage_error() {
    let output = run(&["schema", "nope"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr).unwrap().contains("nope"));

    let output = run(&["schema", "nope", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let envelope = stdout_json(&output);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "usage");
}

#[test]
fn output_is_deterministic() {
    let a = run(&["schema", "envelope", "--json"]).stdout;
    let b = run(&["schema", "envelope", "--json"]).stdout;
    assert_eq!(a, b);
}
