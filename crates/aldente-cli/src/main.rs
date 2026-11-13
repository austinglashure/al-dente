//! The `aldente` command-line binary.

mod commands;
mod formats;
mod output;
mod progress;

use clap::{Parser, Subcommand};

/// Al Dente: turn a git repository into an explorable graph.
#[derive(Debug, Parser)]
#[command(name = "aldente", version, about, arg_required_else_help = true)]
struct Cli {
    /// Print the result as a JSON envelope (see `aldente schema envelope`).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build or reuse the snapshot and print counts.
    Analyze(commands::analyze::AnalyzeArgs),
    /// List what the analyzer could not see through, and skipped files.
    Blindspots(commands::blindspots::BlindspotsArgs),
    /// Manage remote clones and snapshots in the cache.
    Cache(commands::cache::CacheArgs),
    /// Change frequency and co-change coupling from git history.
    Churn(commands::churn::ChurnArgs),
    /// The impact crater: everything seeds can reach, or that reaches them.
    Crater(commands::crater::CraterArgs),
    /// Check that git, clj-kondo and the cache directory are usable.
    Doctor(commands::doctor::DoctorArgs),
    /// Search symbols, impls and namespaces.
    Find(commands::find::FindArgs),
    /// Print a graph or a neighborhood (text, JSON, DOT, Mermaid, GraphML).
    Graph(commands::graph::GraphArgs),
    /// Libraries in use and where the repository invokes them.
    Lib(commands::lib::LibArgs),
    /// Start the local GUI and API (127.0.0.1 only).
    Serve(commands::serve::ServeArgs),
    /// Show a node with its callers and callees.
    Show(commands::show::ShowArgs),
    /// Print JSON Schemas for the envelope and data shapes.
    Schema(commands::schema::SchemaArgs),
    /// Normalize stack traces and map their frames to nodes.
    Trace(commands::trace::TraceArgs),
    /// List build units, their source roots, and skipped files.
    Units(commands::units::UnitsArgs),
}

/// Parses arguments, runs the command and exits with its exit code.
/// clap handles `--version` and `--help` itself.
///
/// Example: `aldente --version` prints `aldente 0.1.0` and exits 0.
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Analyze(args) => commands::analyze::run(args, cli.json),
        Command::Blindspots(args) => commands::blindspots::run(args, cli.json),
        Command::Find(args) => commands::find::run(args, cli.json),
        Command::Graph(args) => commands::graph::run(args, cli.json),
        Command::Lib(args) => commands::lib::run(args, cli.json),
        Command::Serve(args) => commands::serve::run(args, cli.json),
        Command::Show(args) => commands::show::run(args, cli.json),
        Command::Cache(args) => commands::cache::run(args, cli.json),
        Command::Churn(args) => commands::churn::run(args, cli.json),
        Command::Crater(args) => commands::crater::run(args, cli.json),
        Command::Doctor(args) => commands::doctor::run(args, cli.json),
        Command::Schema(args) => commands::schema::run(args, cli.json),
        Command::Trace(args) => commands::trace::run(args, cli.json),
        Command::Units(args) => commands::units::run(args, cli.json),
    };
    match result {
        Ok(code) => std::process::ExitCode::from(code.code()),
        // The reader closed early (e.g. `| head`); nothing more to say.
        Err(err) if output::is_broken_pipe(&err) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("aldente: internal error: {err:#}");
            std::process::ExitCode::from(aldente_model::ExitCode::Internal.code())
        }
    }
}
