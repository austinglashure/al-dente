//! The clj-kondo runner against the real, pinned clj-kondo (CI installs it).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use aldente_acquire::tools::locate;
use aldente_acquire::units::discover;
use aldente_acquire::walk::{Limits, walk};
use aldente_clj::kondo::{Analysis, BUCKETS, CONFIG, Kondo, KondoError};
use aldente_model::{DiagnosticCode, RepoPath};

fn kondo_path() -> PathBuf {
    locate("clj-kondo", std::env::var_os("PATH").as_deref())
        .expect("clj-kondo must be on PATH for these tests")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aldente-kondo-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn kondo(name: &str) -> Kondo {
    Kondo::new(kondo_path(), scratch(&format!("{name}-empty")))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

/// The analyzed files of every unit under `root`.
///
/// Example: clj-mini gives its 13 sources.
fn files(root: &Path) -> Vec<RepoPath> {
    discover(root, false, walk(root, &Limits::default()))
        .analyzed
        .into_iter()
        .map(|f| f.path)
        .collect()
}

/// Copies a directory, skipping entries named in `skip`.
///
/// Example: `copy(src, dst, &[".clj-kondo"])`.
fn copy(src: &Path, dst: &Path, skip: &[&str]) {
    fs::create_dir_all(dst).unwrap();
    for e in fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        if skip.contains(&e.file_name().to_string_lossy().as_ref()) {
            continue;
        }
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &to, skip);
        } else {
            fs::copy(e.path(), to).unwrap();
        }
    }
}

/// The probe from `04-clojure-analysis.md`: every bucket the plan relies on
/// is present for the pinned clj-kondo with the plan's config.
#[test]
fn probe_every_bucket_is_present() {
    let dir = scratch("probe");
    fs::write(
        dir.join("a.clj"),
        "(ns a) (defprotocol P (m [x])) (defrecord R [] P (m [_] 1)) (defn f [x] (.toString x))",
    )
    .unwrap();
    let empty = scratch("probe-empty");
    let out = Command::new(kondo_path())
        .arg("--lint")
        .arg(dir.join("a.clj"))
        .arg("--config-dir")
        .arg(&empty)
        .args(["--cache", "false", "--config", CONFIG])
        .current_dir(&empty)
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let analysis = json["analysis"].as_object().expect("analysis map");
    for bucket in BUCKETS {
        assert!(
            analysis.contains_key(*bucket),
            "bucket {bucket} missing; keys: {:?}",
            analysis.keys()
        );
    }
    let typed: Analysis = serde_json::from_value(json["analysis"].clone()).unwrap();
    assert_eq!(typed.protocol_impls.len(), 1);
    assert_eq!(typed.instance_invocations[0].method_name, "toString");
}

#[test]
fn analyzes_clj_mini_with_relative_paths() {
    let root = fixture("clj-mini");
    let run = kondo("mini").analyze(&root, &files(&root)).unwrap();
    assert!(run.diagnostics.is_empty(), "{:?}", run.diagnostics);
    let a = &run.analysis;
    let checkout = a
        .var_definitions
        .iter()
        .find(|d| d.ns == "shop.core" && d.name == "checkout")
        .expect("shop.core/checkout");
    assert_eq!(checkout.filename, "src/shop/core.clj");
    assert!(
        a.namespace_definitions
            .iter()
            .any(|n| n.name == "shop.ui.views" && n.filename == "src/shop/ui/views.cljs")
    );
    assert!(a.var_usages.iter().all(|u| !u.filename.starts_with('/')));
    let defmethods = a
        .var_usages
        .iter()
        .filter(|u| u.defmethod == Some(true))
        .count();
    assert_eq!(defmethods, 4, "four defmethods of price");
}

#[test]
fn output_is_deterministic() {
    let root = fixture("clj-mini");
    let files = files(&root);
    let a = kondo("det-a").analyze(&root, &files).unwrap();
    let b = kondo("det-b").analyze(&root, &files).unwrap();
    assert_eq!(a, b);
}

#[test]
fn a_broken_file_does_not_stop_the_others() {
    let root = scratch("broken");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/good.clj"), "(ns good) (defn ok [] 1)").unwrap();
    fs::write(root.join("src/bad.clj"), "(ns bad)\n(defn broken [x\n").unwrap();
    let files = [
        RepoPath::parse("src/bad.clj").unwrap(),
        RepoPath::parse("src/good.clj").unwrap(),
    ];
    let run = kondo("broken").analyze(&root, &files).unwrap();
    assert!(run.analysis.var_definitions.iter().any(|d| d.name == "ok"));
    assert_eq!(run.diagnostics.len(), 1, "{:?}", run.diagnostics);
    assert_eq!(run.diagnostics[0].code, DiagnosticCode::ParseError);
    assert_eq!(
        run.diagnostics[0].path.as_ref().unwrap().as_str(),
        "src/bad.clj"
    );
}

/// Hostile case 1: the fixture's `.clj-kondo` config (lint-as and a hook that
/// rewrites a call) has no effect. A control run that lets clj-kondo find the
/// directory shows the config is live.
#[test]
fn hostile_1_target_clj_kondo_config_is_ignored() {
    let with = scratch("hook-with");
    copy(&fixture("hostile/kondo-hook"), &with, &[]);
    let without = scratch("hook-without");
    copy(&fixture("hostile/kondo-hook"), &without, &[".clj-kondo"]);
    let file = [RepoPath::parse("src/evil/core.clj").unwrap()];

    let control = Command::new(kondo_path())
        .args(["--lint", "src", "--cache", "false", "--config", CONFIG])
        .current_dir(&with)
        .output()
        .unwrap();
    let control: serde_json::Value = serde_json::from_slice(&control.stdout).unwrap();
    let widget_defined = control["analysis"]["var-definitions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["name"] == "widget");
    assert!(
        widget_defined,
        "control run should pick up the fixture's lint-as"
    );

    let a = kondo("hook-a").analyze(&with, &file).unwrap();
    let b = kondo("hook-b").analyze(&without, &file).unwrap();
    assert_eq!(a, b, "the .clj-kondo directory changed the analysis");
    assert!(
        !a.analysis
            .var_definitions
            .iter()
            .any(|d| d.name == "widget")
    );
    assert!(
        !a.analysis
            .var_usages
            .iter()
            .any(|u| u.name == "target" && u.from_var.as_deref() == Some("caller"))
    );
}

#[test]
fn a_missing_binary_is_reported_as_missing() {
    let k = Kondo::new(
        PathBuf::from("/nonexistent/clj-kondo"),
        scratch("missing-empty"),
    );
    let root = scratch("missing");
    let err = k
        .analyze(&root, &[RepoPath::parse("a.clj").unwrap()])
        .unwrap_err();
    assert!(matches!(err, KondoError::Missing(_)), "{err}");
}
