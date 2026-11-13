//! The GUI's directory tree and the graph at the tree's resolution.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use aldente_graph::pipeline::Analyzed;
use aldente_graph::query::{Filter, filter};
use aldente_graph::tree::{tree, tree_graph};
use aldente_model::tree::{FileStatus, TreeDir};
use aldente_model::{AggregationLevel, EdgeKind, NodeKind, RepoPath};
use aldente_testkit::{analyze_fixture, fixture_dir};

fn analyzed_paths(a: &Analyzed) -> Vec<RepoPath> {
    a.discovery
        .analyzed
        .iter()
        .map(|f| f.path.clone())
        .collect()
}

/// The tree as indented `label: files` lines.
fn render(d: &TreeDir, depth: usize, out: &mut Vec<String>) {
    let files: Vec<&str> = d.files.iter().map(|f| f.name.as_str()).collect();
    out.push(format!(
        "{}{}: {}",
        "  ".repeat(depth),
        d.label,
        files.join(" ")
    ));
    for c in &d.dirs {
        render(c, depth + 1, out);
    }
}

#[test]
fn chains_collapse_into_one_row() {
    let a = analyze_fixture(&fixture_dir("mono-mini"));
    let t = tree(&a.graph, &analyzed_paths(&a));
    let mut lines = Vec::new();
    render(&t, 0, &mut lines);
    assert_eq!(
        lines,
        [
            ".: ",
            "  libs/common/src/common: util.clj",
            "  services: ",
            "    billing/src: config.clj",
            "      billing: core.clj",
            "    orders: ",
            "      src: config.clj",
            "        orders: core.clj",
            "      test/orders: core_test.clj",
        ]
    );
    let common = &t.dirs[0];
    assert_eq!(
        common.id.as_ref().unwrap().to_string(),
        "dir:libs/common/src/common"
    );
    assert_eq!(
        common.files[0].namespaces[0].to_string(),
        "clj:libs/common//common.util"
    );
    assert_eq!(common.files[0].status, FileStatus::Analyzed);
}

#[test]
fn fully_expanded_tree_graph_is_the_repository_namespace_map() {
    for fixture in ["clj-mini", "mono-mini"] {
        let a = analyze_fixture(&fixture_dir(fixture));
        let t = tree(&a.graph, &analyzed_paths(&a));
        let want = filter(
            &a.graph,
            a.graph.payload(AggregationLevel::Namespace),
            &Filter {
                repo_only: true,
                ..Filter::default()
            },
        );
        let got = tree_graph(&a.graph, &t, &BTreeSet::new());
        assert_eq!(got.nodes, want.nodes, "{fixture} nodes");
        assert_eq!(got.edges, want.edges, "{fixture} edges");
    }
}

#[test]
fn a_collapsed_directory_is_one_node_with_rolled_edges() {
    let a = analyze_fixture(&fixture_dir("mono-mini"));
    let t = tree(&a.graph, &analyzed_paths(&a));
    let collapsed: BTreeSet<RepoPath> = [RepoPath::parse("services").unwrap()].into();
    let p = tree_graph(&a.graph, &t, &collapsed);
    let ids: BTreeSet<String> = p.nodes.iter().map(|n| n.id.to_string()).collect();
    assert!(ids.contains("dir:services"), "{ids:?}");
    assert!(ids.contains("clj:libs/common//common.util"));
    assert!(
        !ids.iter()
            .any(|i| i.contains("orders.") || i.contains("billing.")),
        "{ids:?}"
    );
    let services = p
        .nodes
        .iter()
        .find(|n| n.id.to_string() == "dir:services")
        .unwrap();
    assert_eq!(services.kind, NodeKind::Directory);
    assert_eq!(services.name, "services");
    assert!(p.edges.iter().all(|e| e.from != e.to), "no self-loops");
    let calls = p
        .edges
        .iter()
        .find(|e| {
            e.from.to_string() == "dir:services"
                && e.to.to_string() == "clj:libs/common//common.util"
                && e.kind == EdgeKind::Calls
        })
        .expect("services call common.util");
    assert!(calls.count.unwrap_or(0) >= 1);
    for e in &p.edges {
        assert!(ids.contains(&e.from.to_string()) && ids.contains(&e.to.to_string()));
        e.validate().unwrap();
    }
}

#[test]
fn files_the_analyzer_skipped_are_in_the_tree() {
    let dir = std::env::temp_dir().join(format!("aldente-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (p, src) in [
        ("deps.edn", "{:paths [\"src\"]}"),
        ("src/a/core.clj", "(ns a.core)\n(defn f [] 1)\n"),
        ("src/a/broken.clj", "(ns a.broken)\n(defn g [x\n"),
        ("dev/user.clj", "(ns user)\n"),
    ] {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, src).unwrap();
    }
    let a = analyze_fixture(&dir);
    let t = tree(&a.graph, &analyzed_paths(&a));
    let all: Vec<(String, FileStatus)> = {
        fn walk(d: &TreeDir, out: &mut Vec<(String, FileStatus)>) {
            out.extend(d.files.iter().map(|f| (f.path.to_string(), f.status)));
            d.dirs.iter().for_each(|c| walk(c, out));
        }
        let mut v = Vec::new();
        walk(&t, &mut v);
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    assert_eq!(
        all,
        [
            ("dev/user.clj".to_owned(), FileStatus::NotAnalyzed),
            ("src/a/broken.clj".to_owned(), FileStatus::ParseError),
            ("src/a/core.clj".to_owned(), FileStatus::Analyzed),
        ]
    );
}
