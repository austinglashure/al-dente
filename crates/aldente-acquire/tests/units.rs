//! Unit discovery on the fixtures and on small trees built at test time.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use aldente_acquire::units::{Discovery, discover};
use aldente_acquire::walk::{Limits, walk};
use aldente_model::{DiagnosticCode, Lang, NodeId, RepoPath, RootKind, UnitInfo};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}

/// Walks and discovers units under `root` (treated as a repository without git).
///
/// Example: `run(&fixture("clj-mini")).report.units.len()` is 1.
fn run(root: &Path) -> Discovery {
    discover(root, false, walk(root, &Limits::default()))
}

fn unit<'a>(d: &'a Discovery, dir: &str) -> &'a UnitInfo {
    d.report
        .units
        .iter()
        .find(|u| u.dir.as_str() == dir)
        .unwrap_or_else(|| panic!("no unit {dir}: {:?}", d.report.units))
}

/// `(path, kind, alias)` of a unit's roots.
///
/// Example: `[("src", Source, None), ("test", Test, Some("test"))]`.
fn roots(u: &UnitInfo) -> Vec<(&str, RootKind, Option<&str>)> {
    u.roots
        .iter()
        .map(|r| (r.path.as_str(), r.kind, r.alias.as_deref()))
        .collect()
}

/// The unit directory owning `file`, among analyzed files.
///
/// Example: `owner(&d, "libs/common/src/common/util.clj")` is `Some("libs/common")`.
fn owner<'a>(d: &'a Discovery, file: &str) -> Option<&'a str> {
    d.analyzed
        .iter()
        .find(|f| f.path.as_str() == file)
        .map(|f| f.unit.as_str())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aldente-units-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn put(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn clj_mini_is_one_unit_with_a_tagged_test_root() {
    let d = run(&fixture("clj-mini"));
    assert_eq!(d.report.units.len(), 1);
    let u = unit(&d, ".");
    assert_eq!(u.id, NodeId::Unit(RepoPath::root()));
    assert_eq!(u.manifests, ["deps.edn"]);
    assert_eq!(
        roots(u),
        [
            ("src", RootKind::Source, None),
            ("test", RootKind::Test, Some("test"))
        ]
    );
    assert_eq!(u.langs, [Lang::Clj, Lang::Cljs].into());
    assert_eq!(u.files, 13);
    assert!(!u.service);
    assert!(
        d.report.diagnostics.is_empty(),
        "{:?}",
        d.report.diagnostics
    );
    assert_eq!(owner(&d, "test/shop/pricing_test.clj"), Some("."));
}

#[test]
fn mono_mini_has_three_units_with_correct_ownership() {
    let d = run(&fixture("mono-mini"));
    let dirs: Vec<_> = d.report.units.iter().map(|u| u.dir.as_str()).collect();
    assert_eq!(dirs, ["libs/common", "services/billing", "services/orders"]);

    let orders = unit(&d, "services/orders");
    assert_eq!(
        roots(orders),
        [
            ("services/orders/src", RootKind::Source, None),
            ("services/orders/test", RootKind::Test, Some("test")),
        ]
    );
    assert_eq!(orders.files, 3);
    let billing = unit(&d, "services/billing");
    assert_eq!(billing.manifests, ["project.clj"]);
    assert!(billing.service, ":main marks a service");
    assert_eq!(billing.files, 2);
    assert_eq!(unit(&d, "libs/common").files, 1);

    // Two units define `config`; each file belongs to its own unit.
    assert_eq!(
        owner(&d, "services/orders/src/config.clj"),
        Some("services/orders")
    );
    assert_eq!(
        owner(&d, "services/billing/src/config.clj"),
        Some("services/billing")
    );
    // The :local/root dependency does not pull common's files into orders.
    assert_eq!(
        owner(&d, "libs/common/src/common/util.clj"),
        Some("libs/common")
    );
    assert!(
        d.report.diagnostics.is_empty(),
        "{:?}",
        d.report.diagnostics
    );
}

#[test]
fn discovery_is_deterministic() {
    let a = run(&fixture("mono-mini"));
    let b = run(&fixture("mono-mini"));
    assert_eq!(a, b);
}

/// Hostile case 5: a project.clj full of read-eval and unquote is read as data;
/// nothing runs, the literal roots still count.
#[test]
fn hostile_5_read_eval_project_clj_is_only_read() {
    let root = fixture("hostile/read-eval");
    let d = run(&root);
    let u = unit(&d, ".");
    assert_eq!(roots(u), [("src", RootKind::Source, None)]);
    assert!(u.service);
    for marker in [
        "aldente-pwned-read-eval",
        "aldente-pwned-unquote",
        "aldente-pwned-injection",
    ] {
        assert!(!root.join(marker).exists(), "{marker} must not exist");
        assert!(!std::env::temp_dir().join(marker).exists());
    }
}

#[test]
fn units_without_declared_roots_fall_back() {
    let root = scratch("fallback");
    // shadow-cljs.edn without :source-paths declares no roots.
    put(
        &root,
        "app/shadow-cljs.edn",
        "{:builds {:app {:target :browser}}}",
    );
    put(&root, "app/src/app/core.cljs", "(ns app.core)");
    put(&root, "app/test/app/core_test.cljs", "(ns app.core-test)");
    // bb.edn without :paths and no src/test: the unit directory itself.
    put(&root, "tools/bb.edn", "{:tasks {}}");
    put(&root, "tools/run.bb", "(println 1)");
    let d = run(&root);
    let app = unit(&d, "app");
    assert_eq!(
        roots(app),
        [
            ("app/src", RootKind::Source, None),
            ("app/test", RootKind::Test, None)
        ]
    );
    assert!(app.service);
    assert_eq!(app.langs, [Lang::Cljs].into());
    assert_eq!(
        roots(unit(&d, "tools")),
        [("tools", RootKind::Source, None)]
    );
    assert_eq!(
        d.report.units.len(),
        2,
        "no implicit root unit when nothing is orphaned"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn orphan_sources_get_an_implicit_root_unit() {
    let root = scratch("implicit");
    put(&root, "svc/deps.edn", "{}");
    put(&root, "svc/src/a.clj", "(ns a)");
    put(&root, "scripts/tool.clj", "(ns tool)");
    let d = run(&root);
    let implicit = unit(&d, ".");
    assert!(implicit.manifests.is_empty());
    assert_eq!(roots(implicit), [(".", RootKind::Source, None)]);
    assert_eq!(owner(&d, "scripts/tool.clj"), Some("."));
    assert_eq!(owner(&d, "svc/src/a.clj"), Some("svc"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn problems_become_diagnostics_not_failures() {
    let root = scratch("problems");
    put(
        &root,
        "deps.edn",
        r#"{:paths ["src" "missing" "../../outside"]}"#,
    );
    put(&root, "src/a.clj", "(ns a)");
    put(&root, "build.clj", "(ns build)");
    put(&root, "broken/deps.edn", "{:paths [");
    put(&root, "broken/src/b.clj", "(ns b)");
    let d = run(&root);
    let codes: Vec<_> = d
        .report
        .diagnostics
        .iter()
        .map(|x| (x.path.as_ref().map_or("", RepoPath::as_str), x.code))
        .collect();
    assert!(
        codes.contains(&("missing", DiagnosticCode::MissingRoot)),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("deps.edn", DiagnosticCode::RootOutsideRepo)),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("build.clj", DiagnosticCode::OutsideRoots)),
        "{codes:?}"
    );
    assert!(
        codes.contains(&("broken/deps.edn", DiagnosticCode::ManifestError)),
        "{codes:?}"
    );
    // A broken manifest still yields its unit, with fallback roots.
    assert_eq!(
        roots(unit(&d, "broken")),
        [("broken/src", RootKind::Source, None)]
    );
    assert_eq!(owner(&d, "broken/src/b.clj"), Some("broken"));
    assert_eq!(owner(&d, "build.clj"), None);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_dockerfile_marks_a_service() {
    let root = scratch("docker");
    put(&root, "deps.edn", "{}");
    put(&root, "Dockerfile", "FROM scratch");
    put(&root, "src/a.clj", "(ns a)");
    assert!(unit(&run(&root), ".").service);
    fs::remove_dir_all(root).unwrap();
}

/// The key over clj-mini's analyzed files is pinned, so CI on Linux, macOS and
/// Windows proves it is the same everywhere (`.gitattributes` keeps LF line
/// endings on checkout). If the fixture changes on purpose, update the value.
#[test]
fn clj_mini_snapshot_key_is_the_same_on_every_os() {
    use aldente_acquire::key::{KeyInputs, snapshot_key};
    let d = run(&fixture("clj-mini"));
    let files = d.key_files();
    assert_eq!(files.len(), 14);
    let inputs = KeyInputs {
        analyzer: "test".into(),
        schema: "1".into(),
        adapters: vec![],
        clj_kondo: None,
        config: String::new(),
    };
    assert_eq!(
        snapshot_key(&inputs, &files),
        "b3:94838b0cb86c5011168aa052f900825e9a953300f3739d935826c701d7e356e0"
    );
}
