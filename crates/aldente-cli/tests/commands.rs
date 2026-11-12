//! Text snapshots and JSON schema checks for the core commands, on copies of
//! the fixtures outside this repository (needs clj-kondo on PATH).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A copy of `fixtures/<fixture>` in `<tmp>/<test>/repo` with its own cache.
struct Env {
    dir: PathBuf,
}

impl Env {
    /// Sets up the copy.
    ///
    /// Example: `Env::new("clj-mini", "find")`.
    fn new(fixture: &str, test: &str) -> Env {
        let dir =
            std::env::temp_dir().join(format!("aldente-cli-cmd-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures")
                .join(fixture),
            &dir.join("repo"),
        );
        Env { dir }
    }

    /// Runs `aldente <args…> <repo>`.
    ///
    /// Example: `env.run(&["find", "charge"])`.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aldente"))
            .args(args)
            .arg(self.dir.join("repo"))
            .env("ALDENTE_CACHE_DIR", self.dir.join("cache"))
            // Library attribution must not depend on this machine's ~/.m2.
            .env("ALDENTE_MAVEN_REPO", "")
            .output()
            .expect("aldente binary should start")
    }

    /// Stdout of a successful run.
    ///
    /// Example: `env.text(&["blindspots"])`.
    fn text(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// Parsed JSON of a run with `--json` appended to `args`.
    ///
    /// Example: `env.json(&["find", "charge"])`.
    fn json(&self, args: &[&str]) -> Value {
        let mut all = args.to_vec();
        all.push("--json");
        serde_json::from_slice(&self.run(&all).stdout).unwrap()
    }

    /// Whether `value` matches the published schema `name`.
    ///
    /// Example: `env.valid("find", &json["data"])`.
    fn valid(&self, name: &str, value: &Value) -> bool {
        let out = Command::new(env!("CARGO_BIN_EXE_aldente"))
            .args(["schema", name])
            .output()
            .unwrap();
        let schema: Value = serde_json::from_slice(&out.stdout).unwrap();
        jsonschema::validator_for(&schema).unwrap().is_valid(value)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
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

#[test]
fn analyze_text_and_json() {
    let env = Env::new("clj-mini", "analyze");
    insta::assert_snapshot!("analyze_clj_mini", env.text(&["analyze"]));
    let j = env.json(&["analyze"]);
    assert!(env.valid("analyze", &j["data"]));
}

#[test]
fn find_text_json_and_determinism() {
    let env = Env::new("clj-mini", "find");
    insta::assert_snapshot!("find_charge", env.text(&["find", "charge"]));
    let a = env.run(&["find", "charge", "--json"]).stdout;
    let b = env.run(&["find", "charge", "--json"]).stdout;
    assert_eq!(a, b, "byte-identical JSON");
    let j: Value = serde_json::from_slice(&a).unwrap();
    assert!(env.valid("find", &j["data"]), "{j:#}");
    assert_eq!(j["schema"], "aldente/find/v1");
}

#[test]
fn show_text_and_json() {
    let env = Env::new("clj-mini", "show");
    insta::assert_snapshot!("show_checkout", env.text(&["show", "checkout"]));
    insta::assert_snapshot!(
        "show_half_off",
        env.text(&["show", "shop.pricing/half-off"])
    );
    insta::assert_snapshot!(
        "show_namespace_pricing",
        env.text(&["show", "clj:.//shop.pricing"])
    );
    insta::assert_snapshot!("show_unit", env.text(&["show", "unit:."]));
    let j = env.json(&["show", "checkout"]);
    assert!(env.valid("show", &j["data"]), "{j:#}");
    assert_eq!(j["data"]["node"]["id"], "clj:.//shop.core/checkout");
}

#[test]
fn show_an_ambiguous_name_exits_3_with_candidates() {
    let env = Env::new("mono-mini", "ambiguous");
    let out = env.run(&["show", "charge!"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("clj:services/billing//billing.core/charge!"),
        "{stderr}"
    );
    assert!(
        stderr.contains("clj:services/orders//orders.core/charge!"),
        "{stderr}"
    );
    let j = env.json(&["show", "charge!"]);
    assert_eq!(j["error"]["code"], "ambiguous-symbol");
    assert_eq!(j["error"]["candidates"].as_array().unwrap().len(), 2);
    let ok = env.json(&["show", "billing.core/charge!"]);
    assert_eq!(
        ok["data"]["node"]["id"],
        "clj:services/billing//billing.core/charge!"
    );
    assert_eq!(env.run(&["show", "nope-nothing"]).status.code(), Some(3));
    assert_eq!(
        env.run(&["show", "core/charge!"]).status.code(),
        Some(3),
        "a suffix starts at a whole component"
    );
}

#[test]
fn graph_formats() {
    let env = Env::new("mono-mini", "graph");
    insta::assert_snapshot!(
        "graph_namespace_text",
        env.text(&["graph", "--level", "namespace"])
    );
    insta::assert_snapshot!(
        "graph_namespace_dot",
        env.text(&["graph", "--level", "namespace", "--format", "dot"])
    );
    insta::assert_snapshot!(
        "graph_namespace_mermaid",
        env.text(&["graph", "--level", "namespace", "--format", "mermaid"])
    );
    insta::assert_snapshot!(
        "graph_unit_graphml",
        env.text(&["graph", "--level", "unit", "--format", "graphml"])
    );
    let a = env.run(&["graph", "--level", "symbol", "--json"]).stdout;
    let b = env.run(&["graph", "--level", "symbol", "--json"]).stdout;
    assert_eq!(a, b, "byte-identical JSON");
    let j: Value = serde_json::from_slice(&a).unwrap();
    assert!(env.valid("graph", &j["data"]), "{j:#}");
}

#[test]
fn graph_neighborhood() {
    let env = Env::new("clj-mini", "hood");
    insta::assert_snapshot!(
        "graph_focus_checkout_out",
        env.text(&[
            "graph",
            "--level",
            "symbol",
            "--focus",
            "checkout",
            "--direction",
            "out"
        ])
    );
    let out = env.run(&["graph", "--level", "namespace", "--focus", "checkout"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a symbol is not a namespace-level node"
    );
}

#[test]
fn blindspots_text_and_json() {
    let env = Env::new("clj-mini", "blind");
    insta::assert_snapshot!("blindspots_clj_mini", env.text(&["blindspots"]));
    let j = env.json(&["blindspots", "--category", "macro-generated"]);
    assert!(env.valid("blindspots", &j["data"]));
    assert_eq!(j["data"]["blindspots"].as_array().unwrap().len(), 1);
}

#[test]
fn units_text() {
    let env = Env::new("mono-mini", "units");
    insta::assert_snapshot!("units_mono_mini", env.text(&["units"]));
}

/// WP2.1: `--min-certainty certain` removes exactly the edges whose
/// certainty is lower; edges without a certainty (contains, requires) stay.
#[test]
fn min_certainty_filters_exactly_the_weaker_edges() {
    let env = Env::new("clj-mini", "mincert");
    let all = env.json(&["graph", "--level", "symbol"]);
    let strong = env.json(&["graph", "--level", "symbol", "--min-certainty", "certain"]);
    let edges = |v: &Value| v["data"]["edges"].as_array().unwrap().clone();
    let weaker = edges(&all)
        .iter()
        .filter(|e| matches!(e["certainty"].as_str(), Some("likely" | "possible")))
        .count();
    assert!(weaker > 0, "the fixture has likely and possible edges");
    assert_eq!(edges(&strong).len(), edges(&all).len() - weaker);
    assert!(
        edges(&strong)
            .iter()
            .all(|e| matches!(e["certainty"].as_str(), None | Some("certain")))
    );

    let show = env.json(&["show", "shop.util/with-timing", "--min-certainty", "likely"]);
    assert_eq!(
        show["data"]["callees"].as_array().unwrap().len(),
        0,
        "macro-body edges are possible"
    );
    let analyze = env.json(&["analyze"]);
    assert!(
        analyze["data"]["edge_certainty"]["references"]["possible"]
            .as_u64()
            .unwrap()
            >= 4
    );
}

/// WP2.9: `find --kind <subkind>` and `--flag <flag>` (an empty pattern
/// matches everything).
#[test]
fn find_by_subkind_and_flag() {
    let env = Env::new("clj-mini", "find-filters");
    let ids = |args: &[&str]| -> Vec<String> {
        let j = env.json(args);
        assert!(env.valid("find", &j["data"]), "{j:#}");
        j["data"]["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let tests = [
        "clj:.//shop.pricing-test/book-is-half-off",
        "clj:.//shop.pricing-test/food-is-taxed",
    ];
    assert_eq!(ids(&["find", "", "--kind", "test"]), tests);
    let entry = ids(&["find", "", "--flag", "entry-point"]);
    for id in ["clj:.//shop.core/-main", "clj:.//shop.ui.views/init"]
        .iter()
        .chain(&tests)
    {
        assert!(entry.contains(&(*id).to_owned()), "{id} in {entry:?}");
    }
    assert_eq!(
        ids(&["find", "", "--kind", "test", "--flag", "entry-point"]),
        tests
    );
    assert_eq!(
        ids(&["find", "", "--flag", "test", "--kind", "namespace"]),
        ["clj:.//shop.pricing-test"]
    );
    let text = env.text(&["find", "half", "--kind", "fn"]);
    assert!(text.contains("clj:.//shop.pricing/half-off"), "{text}");
    assert_eq!(
        env.run(&["find", "", "--flag", "nonsense"]).status.code(),
        Some(2)
    );
}
