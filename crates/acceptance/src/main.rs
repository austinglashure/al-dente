//! Runs `acceptance/repos.toml`: clones each repository at its pinned commit
//! (hardened, into the cache), analyzes it, checks the parse rate and the
//! listed facts, records M2 statistics (edges by certainty and reason, blind
//! spots, orphaned symbols), and writes each report to
//! `<out>/<name>-acceptance.json` (default `docs/milestones/m2`). Needs the
//! network, so it is not part of `cargo test`.
//!
//! Usage: `cargo run -p acceptance [-- [--out <dir>] <name>…]`.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use aldente_acquire::cache::{Layout, default_dir};
use aldente_acquire::key::KeyInputs;
use aldente_acquire::remote::{EmptyDirs, Git, acquire, parse_url};
use aldente_acquire::tools::{self, CLJ_KONDO};
use aldente_clj::adapter::{CljAdapter, VERSION as CLJ_VERSION};
use aldente_clj::kondo::Kondo;
use aldente_graph::Graph;
use aldente_graph::pipeline::{CacheContext, Outcome, analyze_cached};
use aldente_graph::snapshot::SCHEMA_VERSION;
use aldente_model::{BlindSpotCategory, DiagnosticCode, EdgeKind, NodeId};
use anyhow::{Context, bail};
use serde::Deserialize;

mod stats;

/// The required share of source files that parse (WP1.10).
const MIN_PARSE_RATE: f64 = 0.95;

#[derive(Debug, Deserialize)]
struct Repos {
    repo: Vec<Repo>,
}

#[derive(Debug, Deserialize)]
struct Repo {
    name: String,
    url: String,
    rev: String,
    lang: String,
    #[serde(default)]
    fact: Vec<Fact>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum Fact {
    Node {
        id: String,
    },
    Edge {
        from: String,
        edge: String,
        to: String,
        /// When given, the edge must have this reason (`keyword-dispatch`, …).
        reason: Option<String>,
        /// When given, the edge must have this certainty.
        certainty: Option<String>,
    },
    NoEdge {
        from: String,
        edge: String,
        to: String,
    },
}

/// Runs the named repositories (all when none are named); exits non-zero
/// if any check fails.
///
/// Example: `cargo run -p acceptance -- ring` prints ring's counts and facts.
fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("acceptance/repos.toml"))
        .context("reading acceptance/repos.toml")?;
    let repos: Repos = toml::from_str(&text).context("parsing acceptance/repos.toml")?;
    let (out, wanted) = parse_args(std::env::args().skip(1));
    let out_dir = root.join(out);
    std::fs::create_dir_all(&out_dir)?;
    let mut failures = 0;
    for repo in repos
        .repo
        .iter()
        .filter(|r| wanted.is_empty() || wanted.contains(&r.name))
    {
        match run(repo, &out_dir) {
            Ok(0) => println!("{}: PASS\n", repo.name),
            Ok(n) => {
                println!("{}: FAIL ({n} checks)\n", repo.name);
                failures += n;
            }
            Err(e) => {
                println!("{}: ERROR {e:#}\n", repo.name);
                failures += 1;
            }
        }
    }
    if failures > 0 {
        bail!("{failures} acceptance checks failed");
    }
    Ok(())
}

/// `--out <dir>` (default `docs/milestones/m2`) and the repository names.
///
/// Example: `["--out", "x", "ring"]` gives `("x", ["ring"])`.
fn parse_args(args: impl Iterator<Item = String>) -> (String, Vec<String>) {
    let mut out = "docs/milestones/m2".to_owned();
    let mut names = Vec::new();
    let mut args = args.peekable();
    while let Some(a) = args.next() {
        match (a.as_str(), args.peek()) {
            ("--out", Some(_)) => out = args.next().unwrap_or_default(),
            _ => names.push(a),
        }
    }
    (out, names)
}

/// Clones, analyzes and checks one repository; returns the number of failed checks.
///
/// Example: `run(&ring, out_dir)` gives `Ok(0)` when everything holds.
fn run(repo: &Repo, out_dir: &Path) -> anyhow::Result<usize> {
    println!(
        "== {} ({}) @ {}",
        repo.name,
        repo.lang,
        &repo.rev[..12.min(repo.rev.len())]
    );
    let path_var = std::env::var_os("PATH");
    let git = tools::locate("git", path_var.as_deref()).context("git not on PATH")?;
    let kondo = tools::check(&CLJ_KONDO, path_var.as_deref(), std::env::consts::OS);
    let kondo_path = kondo.path.clone().context("clj-kondo not on PATH")?;
    let layout = Layout::open(&default_dir().context("no cache directory")?)?;
    let url = parse_url(&repo.url)?;
    let started = Instant::now();
    let acquired = acquire(
        &Git::new(git, EmptyDirs::prepare(&layout.tmp)?),
        &url,
        Some(&repo.rev),
        &layout.repos,
    )?;
    let fetched = started.elapsed();
    let root: PathBuf = acquired.dir.canonicalize()?;
    let ctx = CacheContext {
        snapshots: &layout.snapshots,
        inputs: KeyInputs {
            analyzer: env!("CARGO_PKG_VERSION").to_owned(),
            schema: SCHEMA_VERSION.to_owned(),
            adapters: vec![("clj".to_owned(), CLJ_VERSION.to_owned())],
            clj_kondo: kondo.version.clone(),
            config: aldente_acquire::maven::default_repository()
                .map(|p| format!("maven={}", p.display()))
                .unwrap_or_default(),
        },
        repo: repo.url.clone(),
        rev: Some(repo.rev.clone()),
        no_cache: true,
    };
    let adapter = CljAdapter::new(Kondo::new(
        kondo_path.into(),
        layout.tmp.join("kondo-empty"),
    ))
    .with_maven_repo(aldente_acquire::maven::default_repository());
    let analyzing = Instant::now();
    let outcome: Outcome =
        analyze_cached(&root, true, &[&adapter], &ctx).map_err(|e| anyhow::anyhow!("{e}"))?;
    let analyzed = analyzing.elapsed();
    let summary = aldente_graph::summary::summarize(&outcome.snapshot, &acquired.diagnostics);
    let mut failed = 0;

    let files = outcome.snapshot.files.len();
    let unparsed = unparsed_files(&outcome.snapshot.graph);
    let rate = if files == 0 {
        0.0
    } else {
        (files - unparsed) as f64 / files as f64
    };
    println!(
        "fetch {:.1}s, analyze {:.1}s; units {}, files {files}, parse rate {:.1}% ({unparsed} with errors)",
        fetched.as_secs_f64(),
        analyzed.as_secs_f64(),
        summary.units,
        rate * 100.0
    );
    println!("nodes {:?}", summary.nodes);
    println!("edges {:?}", summary.edges);
    println!("blind spots {:?}", summary.blindspots);
    if rate < MIN_PARSE_RATE {
        println!("  FAIL parse rate below {:.0}%", MIN_PARSE_RATE * 100.0);
        failed += 1;
    }
    for fact in &repo.fact {
        let (ok, label) = check(&outcome.snapshot.graph, fact);
        println!("  {} {label}", if ok { "ok  " } else { "FAIL" });
        failed += usize::from(!ok);
    }
    let edges = stats::edge_breakdown(&outcome.snapshot.graph);
    let orphans = stats::orphans(&outcome.snapshot.graph, 20);
    println!(
        "orphans: {} of {} eligible symbols ({:.1}%) have no incoming edge",
        orphans.orphaned,
        orphans.eligible,
        orphans.share * 100.0
    );
    for o in &orphans.largest {
        println!("  {:>4} lines  {}  {}", o.lines, o.id, o.location);
    }
    let report = serde_json::json!({
        "repo": repo.url,
        "rev": repo.rev,
        "fetch_seconds": fetched.as_secs_f64(),
        "analyze_seconds": analyzed.as_secs_f64(),
        "parse_rate": rate,
        "summary": summary,
        "edges_by_kind_certainty_reason": edges,
        "orphans": orphans,
    });
    std::fs::write(
        out_dir.join(format!("{}-acceptance.json", repo.name)),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;
    Ok(failed)
}

/// Files with a parse error or an `unparsed-file` blind spot.
///
/// Example: a graph with one `parse-error` diagnostic gives 1.
fn unparsed_files(g: &Graph) -> usize {
    let mut files: std::collections::BTreeSet<String> = g
        .diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::ParseError)
        .filter_map(|d| d.path.as_ref().map(|p| p.to_string()))
        .collect();
    files.extend(
        g.blindspots
            .iter()
            .filter(|b| b.category == BlindSpotCategory::UnparsedFile)
            .filter_map(|b| b.site.as_ref().map(|s| s.file.to_string())),
    );
    files.len()
}

/// Checks one fact; returns whether it holds and a printable label.
///
/// Example: an `edge` fact holds when an edge of that kind joins the two IDs.
fn check(g: &Graph, fact: &Fact) -> (bool, String) {
    let token = |v: serde_json::Value| v.as_str().map(str::to_owned);
    let has_edge =
        |from: &str, edge: &str, to: &str, reason: Option<&str>, certainty: Option<&str>| {
            let kind =
                serde_json::from_value::<EdgeKind>(serde_json::Value::String(edge.to_owned())).ok();
            let (Ok(from), Ok(to)) = (NodeId::from_str(from), NodeId::from_str(to)) else {
                return false;
            };
            g.edges.iter().any(|e| {
                e.from == from
                    && e.to == to
                    && Some(e.kind) == kind
                    && reason.is_none_or(|r| {
                        e.reason
                            .and_then(|x| serde_json::to_value(x).ok().and_then(token))
                            .as_deref()
                            == Some(r)
                    })
                    && certainty.is_none_or(|c| {
                        e.certainty
                            .and_then(|x| serde_json::to_value(x).ok().and_then(token))
                            .as_deref()
                            == Some(c)
                    })
            })
        };
    match fact {
        Fact::Node { id } => (
            NodeId::from_str(id).is_ok_and(|i| g.nodes.contains_key(&i)),
            format!("node {id}"),
        ),
        Fact::Edge {
            from,
            edge,
            to,
            reason,
            certainty,
        } => {
            let attrs: Vec<&str> = [certainty.as_deref(), reason.as_deref()]
                .into_iter()
                .flatten()
                .collect();
            let label = if attrs.is_empty() {
                format!("{from} -{edge}-> {to}")
            } else {
                format!("{from} -{edge}({})-> {to}", attrs.join(" "))
            };
            (
                has_edge(from, edge, to, reason.as_deref(), certainty.as_deref()),
                label,
            )
        }
        Fact::NoEdge { from, edge, to } => (
            !has_edge(from, edge, to, None, None),
            format!("no {from} -{edge}-> {to}"),
        ),
    }
}
