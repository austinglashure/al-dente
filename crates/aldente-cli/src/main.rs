//! The `aldente` command-line binary.

use clap::Parser;

/// Al Dente: turn a git repository into an explorable graph.
#[derive(Debug, Parser)]
#[command(name = "aldente", version, about)]
struct Cli {}

/// Parses arguments; clap handles `--version` and `--help` and exits.
///
/// Example: `aldente --version` prints `aldente 0.1.0` and exits 0.
fn main() {
    let _cli = Cli::parse();
}
