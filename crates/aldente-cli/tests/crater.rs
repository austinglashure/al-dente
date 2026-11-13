//! `aldente crater`: golden text and JSON on clj-mini, and `--diff` on
//! temporary git repositories (needs clj-kondo and git on PATH).

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

/// A scratch directory with a repository copy and its own cache.
struct Env {
    dir: PathBuf,
}

impl Env {
    /// An empty scratch directory for `test`.
    ///
    /// Example: `Env::empty("diff-body")`.
    fn empty(test: &str) -> Env {
        let dir =
            std::env::temp_dir().join(format!("aldente-crater-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("repo")).unwrap();
        Env { dir }
    }

    /// A copy of `fixtures/<fixture>`.
    ///
    /// Example: `Env::fixture("clj-mini", "golden")`.
    fn fixture(fixture: &str, test: &str) -> Env {
        let env = Env::empty(test);
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures")
                .join(fixture),
            &env.repo(),
        );
        env
    }

    fn repo(&self) -> PathBuf {
        self.dir.join("repo")
    }

    /// Runs `aldente <args…> --target <repo>` with cache `cache`.
    ///
    /// Example: `env.run_in("cache", &["crater", "checkout"])`.
    fn run_in(&self, cache: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aldente"))
            .args(args)
            .arg("--target")
            .arg(self.repo())
            .env("ALDENTE_CACHE_DIR", self.dir.join(cache))
            .env("ALDENTE_MAVEN_REPO", "")
            .output()
            .unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in("cache", args)
    }

    /// Stdout of a successful run.
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

    /// The envelope of a `--json` run in cache `cache`.
    fn json_in(&self, cache: &str, args: &[&str]) -> Value {
        let mut all = args.to_vec();
        all.push("--json");
        let out = self.run_in(cache, &all);
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| panic!("{args:?}: {}", String::from_utf8_lossy(&out.stderr)))
    }

    fn json(&self, args: &[&str]) -> Value {
        self.json_in("cache", args)
    }

    /// Runs plain git in the repository with no user or system config.
    ///
    /// Example: `env.git(&["commit", "-qam", "two"])`.
    fn git(&self, args: &[&str]) {
        let empty = self.dir.join("empty-gitconfig");
        fs::write(&empty, "").unwrap();
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
            .current_dir(self.repo())
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

    fn write(&self, path: &str, text: &str) {
        let p = self.repo().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
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

/// Whether `value` matches the published schema `name`.
fn valid(name: &str, value: &Value) -> bool {
    let out = Command::new(env!("CARGO_BIN_EXE_aldente"))
        .args(["schema", name])
        .output()
        .unwrap();
    let schema: Value = serde_json::from_slice(&out.stdout).unwrap();
    jsonschema::validator_for(&schema).unwrap().is_valid(value)
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn forward_and_reverse_goldens() {
    let env = Env::fixture("clj-mini", "golden");
    insta::assert_snapshot!("crater_forward_checkout", env.text(&["crater", "checkout"]));
    insta::assert_snapshot!(
        "crater_reverse_price",
        env.text(&[
            "crater",
            "shop.pricing/price",
            "--direction",
            "reverse",
            "--min-certainty",
            "possible"
        ])
    );
    insta::assert_snapshot!(
        "crater_reverse_half_off_checklist",
        env.text(&[
            "crater",
            "shop.pricing/half-off",
            "--direction",
            "reverse",
            "--checklist"
        ])
    );
    let forward = env.json(&["crater", "checkout"]);
    assert_eq!(forward["schema"], "aldente/crater/v1");
    assert!(valid("crater", &forward["data"]), "{forward:#}");
    insta::assert_snapshot!(
        "crater_forward_checkout_json",
        serde_json::to_string_pretty(&forward["data"]).unwrap()
    );
    let reverse = env.json(&["crater", "shop.pricing/half-off", "--direction", "reverse"]);
    assert!(valid("crater", &reverse["data"]));
    insta::assert_snapshot!(
        "crater_reverse_half_off_json",
        serde_json::to_string_pretty(&reverse["data"]).unwrap()
    );
    let list = env.json(&[
        "crater",
        "shop.pricing/half-off",
        "--direction",
        "reverse",
        "--checklist",
    ]);
    assert_eq!(list["schema"], "aldente/checklist/v1");
    assert!(valid("checklist", &list["data"]), "{list:#}");
    let a = env.run(&["crater", "checkout", "--json"]).stdout;
    let b = env.run(&["crater", "checkout", "--json"]).stdout;
    assert_eq!(a, b, "byte-identical JSON");
}

#[test]
fn dot_and_mermaid() {
    let env = Env::fixture("clj-mini", "formats");
    let dot = env.text(&["crater", "checkout", "--format", "dot"]);
    assert!(dot.starts_with("digraph"), "{dot}");
    let mermaid = env.text(&["crater", "checkout", "--format", "mermaid"]);
    assert!(mermaid.contains("reserve!"), "{mermaid}");
}

const CORE_V1: &str = "(ns app.core)

;; adds one
(defn inc1 [x]
  (+ x 1))

(defn twice [x]
  (inc1 (inc1 x)))

(defn gone [] (twice 1))

(defn user-of-gone [] (gone))
";

/// A git repository with one commit of `CORE_V1`.
fn diff_repo(test: &str) -> Env {
    let env = Env::empty(test);
    env.write("deps.edn", "{:paths [\"src\"]}\n");
    env.write("src/app/core.clj", CORE_V1);
    env.git(&["init", "-q", "-b", "main"]);
    env.git(&["add", "."]);
    env.git(&["commit", "-q", "-m", "one"]);
    env
}

#[test]
fn diff_seeds_exactly_the_changed_function() {
    let env = diff_repo("diff-body");
    env.write("src/app/core.clj", &CORE_V1.replace("(+ x 1)", "(+ 1 x)"));
    let j = env.json(&["crater", "--diff", "HEAD"]);
    let diff = &j["data"]["diff"];
    assert_eq!(ids(&diff["changed"]), ["clj:.//app.core/inc1"], "{j:#}");
    assert_eq!(j["data"]["direction"], "reverse");
    assert_eq!(ids(&j["data"]["seeds"]), ["clj:.//app.core/inc1"]);
    assert!(valid("crater", &j["data"]), "{j:#}");
    let reached: Vec<String> = j["data"]["reach"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        reached.contains(&"clj:.//app.core/twice".to_owned()),
        "{reached:?}"
    );
}

#[test]
fn diff_ignores_a_comment_above_a_function() {
    let env = diff_repo("diff-comment");
    env.write(
        "src/app/core.clj",
        &CORE_V1.replace(";; adds one", ";; adds 1"),
    );
    let j = env.json(&["crater", "--diff", "HEAD"]);
    assert_eq!(
        ids(&j["data"]["diff"]["changed"]),
        Vec::<String>::new(),
        "{j:#}"
    );
    assert_eq!(j["data"]["reach"].as_array().unwrap().len(), 0);
}

#[test]
fn diff_reports_a_removed_function_with_and_without_a_base_snapshot() {
    let env = diff_repo("diff-removed");
    // Analyze the first commit so its snapshot is in "cache".
    assert_eq!(env.run(&["crater", "inc1"]).status.code(), Some(0));
    env.write(
        "src/app/core.clj",
        &CORE_V1.replace("(defn gone [] (twice 1))\n\n", ""),
    );
    env.git(&["commit", "-qam", "two"]);

    let with = env.json(&["crater", "--diff", "HEAD~1"]);
    let removed = &with["data"]["diff"]["removed"];
    assert_eq!(removed.as_array().unwrap().len(), 1, "{with:#}");
    assert_eq!(removed[0]["namespace"], "app.core");
    assert_eq!(removed[0]["name"], "gone");
    assert_eq!(removed[0]["file"], "src/app/core.clj");
    assert_eq!(removed[0]["line"], 10);
    assert_eq!(
        ids(&removed[0]["former_callers"]),
        ["clj:.//app.core/user-of-gone"]
    );
    assert_eq!(with["data"]["diff"]["base_snapshot"], true);
    assert!(with["warnings"].as_array().unwrap().is_empty(), "{with:#}");

    let without = env.json_in("fresh-cache", &["crater", "--diff", "HEAD~1"]);
    let removed = &without["data"]["diff"]["removed"];
    assert_eq!(removed[0]["name"], "gone");
    assert!(removed[0]["former_callers"].is_null());
    assert_eq!(without["data"]["diff"]["base_snapshot"], false);
    assert_eq!(
        without["warnings"][0]["code"], "no-base-snapshot",
        "{without:#}"
    );

    let text = String::from_utf8(env.run(&["crater", "--diff", "HEAD~1"]).stdout).unwrap();
    assert!(
        text.contains(
            "app.core/gone  src/app/core.clj:10  former callers: clj:.//app.core/user-of-gone"
        ),
        "{text}"
    );
}

#[test]
fn diff_rejects_option_like_and_unknown_revisions() {
    let env = diff_repo("diff-bad");
    assert_eq!(
        env.run(&["crater", "--diff", "--output=x"]).status.code(),
        Some(2)
    );
    assert_eq!(env.run(&["crater", "--diff=-x"]).status.code(), Some(2));
    assert_eq!(
        env.run(&["crater", "--diff", "nope"]).status.code(),
        Some(2)
    );
    env.write("src/app/core.clj", &CORE_V1.replace("(+ x 1)", "(+ 1 x)"));
    env.git(&["commit", "-qam", "two"]);
    assert_eq!(
        env.run(&["crater", "--diff", "HEAD~1..HEAD"]).status.code(),
        Some(0)
    );
    assert_eq!(
        env.run(&["crater", "--diff", "HEAD~1..HEAD~1"])
            .status
            .code(),
        Some(2)
    );
}

/// Hostile case 6a for `--diff`: the repository's own config names a clean
/// filter, textconv, external diff, pager and fsmonitor that write a marker.
#[test]
fn diff_runs_nothing_from_the_repository_config() {
    let env = diff_repo("diff-hostile");
    env.write(".gitattributes", "* diff=evil filter=evil\n");
    env.git(&["add", "."]);
    env.git(&["commit", "-q", "-m", "attrs"]);
    let marker = env.dir.join("ran").display().to_string().replace('\\', "/");
    let touch = format!("touch '{marker}'");
    let config_path = env.repo().join(".git/config");
    let mut config = fs::read_to_string(&config_path).unwrap();
    config.push_str(&format!(
        "[core]\n\tpager = {touch}\n\tfsmonitor = {touch}\n[diff]\n\texternal = {touch}\n\
         [diff \"evil\"]\n\ttextconv = {touch}\n[filter \"evil\"]\n\tclean = {touch}\n\tsmudge = {touch}\n"
    ));
    fs::write(&config_path, config).unwrap();
    env.write("src/app/core.clj", &CORE_V1.replace("(+ x 1)", "(+ 1 x)"));
    let j = env.json(&["crater", "--diff", "HEAD"]);
    assert_eq!(
        ids(&j["data"]["diff"]["changed"]),
        ["clj:.//app.core/inc1"],
        "{j:#}"
    );
    assert!(
        !env.dir.join("ran").exists(),
        "a command from the repository's config ran"
    );
}

#[test]
fn trace_pinned_crater() {
    let env = Env::fixture("clj-mini", "pinned");
    let trace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/traces/jvm/clj-mini/checkout-book-nil-price.txt")
        .display()
        .to_string();
    insta::assert_snapshot!(
        "crater_trace_checkout_book",
        env.text(&["crater", "--trace", &trace, "--min-certainty", "possible"])
    );
    let j = env.json(&["crater", "--trace", &trace]);
    assert!(valid("crater", &j["data"]), "{j:#}");
    assert_eq!(j["data"]["direction"], "reverse");
    assert_eq!(ids(&j["data"]["pinned"]["path"]).len(), 4);
    assert_eq!(
        j["data"]["pinned"]["overlay"][0]["reason"],
        "trace-observed"
    );
    assert_eq!(
        env.run(&["crater", "checkout", "--trace", &trace])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn a_library_seeds_a_reverse_crater_of_its_users() {
    let env = Env::fixture("clj-mini", "lib-seed");
    let j = env.json(&["crater", "lib:npm:react", "--direction", "reverse"]);
    let reached: Vec<String> = j["data"]["reach"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    for user in [
        "clj:.//shop.ui.views/cart-view",
        "clj:.//shop.ui.views/submit-button",
    ] {
        assert!(
            reached.contains(&user.to_owned()),
            "{user} not in {reached:?}"
        );
    }
    assert!(
        !reached.iter().any(|r| r.contains("shop.pricing")),
        "{reached:?}"
    );
}
