//! `cargo run --release -p bench -- gen | gen-src`: synthetic corpora for
//! scale work (WP5.1).

use std::path::PathBuf;
use std::time::Instant;

use bench::clj_src::{SrcSpec, write_corpus};
use bench::out::{to_snapshot, write};
use bench::synth::{Spec, synthesize};
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "bench", about = "Synthetic corpora for Al Dente scale work")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Write a synthetic snapshot (no source files); open it with
    /// `aldente serve --snapshot FILE`.
    Gen(GenArgs),
    /// Write Clojure source for a smaller monorepo, for end-to-end timing.
    GenSrc(GenSrcArgs),
}

#[derive(Debug, Args)]
struct GenArgs {
    #[arg(long, default_value_t = Spec::default().units)]
    units: usize,
    #[arg(long, default_value_t = Spec::default().namespaces)]
    namespaces: usize,
    #[arg(long, default_value_t = Spec::default().symbols)]
    symbols: usize,
    /// Every edge, `contains` and `requires` included.
    #[arg(long, default_value_t = Spec::default().edges)]
    edges: usize,
    #[arg(long, default_value_t = Spec::default().hubs)]
    hubs: usize,
    /// Symbols in the largest namespace.
    #[arg(long, default_value_t = Spec::default().big_namespace)]
    big_namespace: usize,
    #[arg(long, default_value_t = Spec::default().seed)]
    seed: u64,
    #[arg(long, default_value = "target/bench/synthetic.db")]
    out: PathBuf,
}

#[derive(Debug, Args)]
struct GenSrcArgs {
    #[arg(long, default_value_t = SrcSpec::default().units)]
    units: usize,
    #[arg(long, default_value_t = SrcSpec::default().namespaces)]
    namespaces: usize,
    #[arg(long, default_value_t = SrcSpec::default().fns_per_ns)]
    fns_per_ns: usize,
    #[arg(long, default_value_t = SrcSpec::default().calls_per_fn)]
    calls_per_fn: usize,
    #[arg(long, default_value_t = SrcSpec::default().seed)]
    seed: u64,
    /// An empty or new directory.
    #[arg(long, default_value = "target/bench/src-corpus")]
    out: PathBuf,
}

/// Parses the command line and runs the generator.
///
/// Example: `bench gen --symbols 1000 --namespaces 50 --edges 10000 --big-namespace 100`.
fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Gen(a) => gen_snapshot(a),
        Command::GenSrc(a) => gen_source(a),
    }
}

/// Generates and writes the synthetic snapshot, printing sizes and times.
///
/// Example: `gen_snapshot(defaults)` prints `wrote target/bench/synthetic.db: 300000 symbols, …`.
fn gen_snapshot(a: GenArgs) -> anyhow::Result<()> {
    let spec = Spec {
        units: a.units,
        namespaces: a.namespaces,
        symbols: a.symbols,
        edges: a.edges,
        hubs: a.hubs,
        big_namespace: a.big_namespace,
        seed: a.seed,
    };
    let started = Instant::now();
    let corpus = synthesize(&spec)?;
    let generated = started.elapsed();
    let snap = to_snapshot(&spec, corpus);
    let (nodes, edges) = (snap.graph.nodes.len(), snap.graph.edges.len());
    write(&a.out, &snap)?;
    println!(
        "wrote {}: {nodes} nodes, {edges} edges (generated in {:.1}s, written in {:.1}s)",
        a.out.display(),
        generated.as_secs_f64(),
        (started.elapsed() - generated).as_secs_f64()
    );
    println!("open it with: aldente serve --snapshot {}", a.out.display());
    Ok(())
}

/// Writes the Clojure corpus, printing what it holds.
///
/// Example: `gen_source(defaults)` prints `wrote 200 files, 5003 functions, … calls`.
fn gen_source(a: GenSrcArgs) -> anyhow::Result<()> {
    let spec = SrcSpec {
        units: a.units,
        namespaces: a.namespaces,
        fns_per_ns: a.fns_per_ns,
        calls_per_fn: a.calls_per_fn,
        seed: a.seed,
    };
    let m = write_corpus(&a.out, &spec)?;
    git_init(&a.out)?;
    println!(
        "wrote {} files, {} functions, {} repository calls under {}",
        m.files,
        m.symbols,
        m.calls.len(),
        a.out.display()
    );
    println!(
        "time it with: aldente analyze --timings --no-cache {}",
        a.out.display()
    );
    Ok(())
}

/// Makes `dir` its own git repository, so Al Dente analyzes the corpus and
/// not a repository around it (a target resolves to its git root).
///
/// Example: `git_init(Path::new("target/bench/src-corpus"))` creates `target/bench/src-corpus/.git`.
fn git_init(dir: &std::path::Path) -> anyhow::Result<()> {
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(dir)
        .status()
        .map_err(|e| anyhow::anyhow!("cannot run git ({e}); git is required"))?;
    anyhow::ensure!(status.success(), "git init failed in {}", dir.display());
    Ok(())
}
