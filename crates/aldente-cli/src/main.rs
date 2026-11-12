//! The `aldente` command-line binary.

mod commands;
mod formats;
mod output;

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
    /// Check that git, clj-kondo and the cache directory are usable.
    Doctor(commands::doctor::DoctorArgs),
    /// Search symbols, impls and namespaces.
    Find(commands::find::FindArgs),
    /// Print a graph or a neighborhood (text, JSON, DOT, Mermaid, GraphML).
    Graph(commands::graph::GraphArgs),
    /// Show a node with its callers and callees.
    Show(commands::show::ShowArgs),
    /// Print JSON Schemas for the envelope and data shapes.
    Schema(commands::schema::SchemaArgs),
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
        Command::Show(args) => commands::show::run(args, cli.json),
        Command::Cache(args) => commands::cache::run(args, cli.json),
        Command::Doctor(args) => commands::doctor::run(args, cli.json),
        Command::Schema(args) => commands::schema::run(args, cli.json),
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
