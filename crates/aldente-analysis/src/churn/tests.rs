//! Churn tests: a scripted temporary repository with known history, read
//! with the real (hardened) git log and analyzed with the real pipeline.

use std::fs;
use std::path::Path;
use std::process::Command;

use aldente_acquire::history::{self, LogOptions};
use aldente_acquire::remote::{EmptyDirs, Git};
use aldente_model::churn::HistoryDepth;

use super::*;

fn plain_git(cwd: &Path, args: &[&str], time: u32) {
    // `parent()`, not `join("..")`: on Windows `canonicalize` gives `\\?\` paths, where `..` is not resolved and git cannot open the file.
    let empty = cwd.parent().unwrap().join("empty-gitconfig");
    fs::write(&empty, "").unwrap();
    let stamp = format!("{} +0000", 1_700_000_000 + time * 86_400);
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", &empty)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_DATE", &stamp)
        .env("GIT_COMMITTER_DATE", &stamp)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Appends a comment line to each file and commits.
fn touch_commit(repo: &Path, files: &[&str], n: u32) {
    for f in files {
        let p = repo.join(f);
        let mut s = fs::read_to_string(&p).unwrap();
        s.push_str(&format!(";; change {n}\n"));
        fs::write(p, s).unwrap();
    }
    plain_git(repo, &["commit", "-q", "-am", "change"], n);
}

/// History: (1) add a, b, c, d; (2) a, b, d; (3–4) a, b; (5–7) a, c;
/// (8) a bulk commit over every file, which co-change ignores at
/// `max_files_per_commit = 3`; (9) rename d to dd; (10) edit inside `f`. `a` calls `b`; `c`
/// and `a` share no static path.
#[test]
fn scripted_history_gives_known_churn_and_co_change() {
    let base = std::env::temp_dir().join(format!("aldente-churn-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let repo = base.join("repo");
    fs::create_dir_all(repo.join("src/app")).unwrap();
    let base = base.canonicalize().unwrap();
    let repo = repo.canonicalize().unwrap();
    fs::write(repo.join("deps.edn"), "{:paths [\"src\"]}\n").unwrap();
    fs::write(
        repo.join("src/app/a.clj"),
        "(ns app.a (:require [app.b :as b]))\n\n(defn f []\n  (b/g))\n",
    )
    .unwrap();
    fs::write(repo.join("src/app/b.clj"), "(ns app.b)\n\n(defn g [] 1)\n").unwrap();
    fs::write(repo.join("src/app/c.clj"), "(ns app.c)\n\n(defn h [] 2)\n").unwrap();
    fs::write(repo.join("src/app/d.clj"), "(ns app.d)\n\n(defn k [] 3)\n").unwrap();
    plain_git(&repo, &["init", "-q", "-b", "main"], 0);
    plain_git(&repo, &["add", "."], 0);
    plain_git(&repo, &["commit", "-q", "-m", "one"], 1);
    let (a, b, c, d) = (
        "src/app/a.clj",
        "src/app/b.clj",
        "src/app/c.clj",
        "src/app/d.clj",
    );
    touch_commit(&repo, &[a, b, d], 2);
    touch_commit(&repo, &[a, b], 3);
    touch_commit(&repo, &[a, b], 4);
    for n in 5..8 {
        touch_commit(&repo, &[a, c], n);
    }
    touch_commit(&repo, &[a, b, c, d, "deps.edn"], 8);
    plain_git(&repo, &["mv", d, "src/app/dd.clj"], 9);
    plain_git(&repo, &["commit", "-q", "-m", "rename"], 9);
    // (10) an edit inside f's body (line 4).
    let text = fs::read_to_string(repo.join(a))
        .unwrap()
        .replace("  (b/g))", "  (inc (b/g)))");
    fs::write(repo.join(a), text).unwrap();
    plain_git(&repo, &["commit", "-q", "-am", "edit f"], 10);

    let git = Git::new("git".into(), EmptyDirs::prepare(&base.join("tmp")).unwrap());
    let opts = LogOptions {
        max_commits: 100,
        since: None,
    };
    let commits = history::numstat(&git, &repo, &opts).unwrap();
    let hunks = history::hunks(
        &git,
        &repo,
        &LogOptions {
            max_commits: 3,
            ..opts
        },
    )
    .unwrap();
    let h = History {
        depth: HistoryDepth::Full,
        commits,
        hunks,
        since: None,
    };
    let graph = aldente_testkit::analyze_fixture(&repo).graph;
    let exists = |p: &str| repo.join(p).is_file();
    let copts = ChurnOptions {
        max_files_per_commit: 3,
        ..ChurnOptions::default()
    };
    let r = churn(&graph, &h, &exists, &copts);

    assert_eq!(r.commits, 10);
    let file = |p: &str| r.files.iter().find(|f| f.path == p).unwrap();
    assert_eq!(file(a).commits, 9);
    assert_eq!(file(b).commits, 5);
    assert_eq!(file(c).commits, 5);
    // d's history follows the rename; the old path is gone.
    assert_eq!(file("src/app/dd.clj").commits, 4);
    assert!(r.files.iter().all(|f| f.path != d));
    assert_eq!(
        file("src/app/dd.clj").last_changed,
        date(1_700_000_000 + 9 * 86_400)
    );
    assert_eq!(r.files[0].path, a, "most changed first");

    // a–b: commit 1 has 5 files (over the limit of 3, skipped); 2, 3 and
    // 4 count → support 3, one static hop.
    let pair = |x: &str, y: &str| r.co_changes.iter().find(|p| p.a == x && p.b == y);
    let ab = pair(a, b).expect("a–b");
    assert_eq!(ab.support, 3);
    assert_eq!(ab.static_hops, Some(1));
    assert!(!ab.hidden);
    let ac = pair(a, c).expect("a–c");
    assert_eq!(ac.support, 3);
    assert_eq!(ac.static_hops, None);
    assert!(ac.hidden, "a and c change together with no static path");
    assert!((ac.confidence_b_a - 3.0 / 5.0).abs() < 1e-12);
    assert!(pair(b, c).is_none(), "b and c only shared the bulk commits");

    // Symbol window = the last 3 commits: the edit inside f, the rename
    // (no line changes) and the bulk commit, whose comments land after
    // every definition and so touch none.
    let sym = |s: &str| {
        r.symbols
            .iter()
            .find(|x| x.id.to_string().ends_with(s))
            .map(|x| x.commits)
    };
    assert_eq!(r.symbol_commits, 3);
    assert_eq!(sym("app.a/f"), Some(1));
    assert_eq!(sym("app.b/g"), None);
    fs::remove_dir_all(&base).unwrap();
}

#[test]
fn no_history_gives_an_empty_report() {
    let h = History {
        depth: HistoryDepth::None,
        commits: vec![],
        hunks: vec![],
        since: None,
    };
    let r = churn(&Graph::default(), &h, &|_| true, &ChurnOptions::default());
    assert_eq!(r.history, HistoryDepth::None);
    assert!(r.files.is_empty() && r.symbols.is_empty() && r.co_changes.is_empty());
}

#[test]
fn dates_and_renames() {
    assert_eq!(date(0), "1970-01-01");
    assert_eq!(date(1_700_000_000), "2023-11-14");
    assert_eq!(date(951_782_400), "2000-02-29");
    let change = |path: &str, old: Option<&str>| history::FileChange {
        path: path.into(),
        old_path: old.map(Into::into),
        added: Some(1),
        deleted: Some(0),
    };
    let commits = vec![
        CommitStat {
            id: "3".into(),
            time: 3,
            files: vec![change("c", Some("b"))],
        },
        CommitStat {
            id: "2".into(),
            time: 2,
            files: vec![change("b", Some("a"))],
        },
        CommitStat {
            id: "1".into(),
            time: 1,
            files: vec![change("a", None)],
        },
    ];
    let al = rename_aliases(&commits);
    assert_eq!(al.get(&(1, "b".into())).map(String::as_str), Some("c"));
    assert_eq!(al.get(&(2, "a".into())).map(String::as_str), Some("c"));
}
