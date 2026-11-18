//! `aldente serve [TARGET]`: the local GUI and API on 127.0.0.1.

use std::path::{Path, PathBuf};

use aldente_graph::snapshot::{self, Snapshot};
use aldente_model::analyze::ServeInfo;
use aldente_model::{Diagnostic, Envelope, ErrorCode, ExitCode, SnapshotInfo};
use aldente_server::{AppState, new_token, open_browser, run as serve};
use clap::Args;

use crate::commands::loaded::load;
use crate::commands::target::{Failure, TargetArgs};
use crate::output::{fail, print_json};

/// Arguments for `aldente serve`.
#[derive(Debug, Args)]
pub struct ServeArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Port on 127.0.0.1; 0 picks a free one.
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// Do not open the browser.
    #[arg(long)]
    no_open: bool,
    /// Serve this snapshot file instead of analyzing a target, e.g. a
    /// synthetic one from `cargo run -p bench -- gen`.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["target", "git_ref", "fetch_timeout"])]
    snapshot: Option<PathBuf>,
}

/// What `serve` needs from either a target or a snapshot file.
struct Served {
    snapshot: Snapshot,
    root: PathBuf,
    info: SnapshotInfo,
    extra: Vec<Diagnostic>,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "serve";

/// Loads the snapshot, starts the server, prints the URL (with the session
/// token), opens the browser unless `--no-open`, and serves until Ctrl-C.
///
/// Example: `aldente serve --no-open .` prints `http://127.0.0.1:41733/?token=…`.
pub fn run(args: &ServeArgs, json: bool) -> anyhow::Result<ExitCode> {
    let served = match &args.snapshot {
        Some(file) => open_file(file, args.target.timings),
        None => open_target(&args.target),
    };
    let Served {
        snapshot,
        root,
        info,
        extra,
    } = match served {
        Ok(s) => s,
        Err(f) => return fail(TAG, &f, json),
    };
    let timings = args.target.timings;
    let editor = match crate::config::editor() {
        Ok(e) => e,
        Err(e) => {
            let f = crate::commands::target::Failure::new(
                aldente_model::ErrorCode::Usage,
                format!("config: {e}"),
            );
            return fail(TAG, &f, json);
        }
    };
    let token = new_token();
    let state_info = info.clone();
    let no_open = args.no_open;
    let result = serve(
        move |port| AppState {
            timings,
            git: crate::commands::crater_diff::hardened_git().ok(),
            editor,
            ..AppState::new(snapshot, root, state_info, &extra, token, port)
        },
        args.port,
        |url| {
            let port = url
                .trim_start_matches("http://127.0.0.1:")
                .split('/')
                .next()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);
            if json {
                let _ = print_json(&Envelope::success(
                    TAG,
                    Some(info.clone()),
                    ServeInfo {
                        url: url.to_owned(),
                        port,
                    },
                ));
            } else {
                println!("{url}");
            }
            eprintln!("aldente: serving on 127.0.0.1 only; press Ctrl-C to stop");
            if !no_open && let Err(e) = open_browser(url) {
                eprintln!("aldente: could not open a browser ({e}); open the URL above");
            }
        },
    );
    match result {
        Ok(()) => Ok(ExitCode::Success),
        Err(e) => fail(
            TAG,
            &Failure::new(ErrorCode::Usage, format!("cannot serve: {e}")),
            json,
        ),
    }
}

/// Loads (or analyzes) the target, as every other command does.
///
/// Example: `open_target(&args)` on clj-mini gives its cached snapshot and root.
fn open_target(target: &TargetArgs) -> Result<Served, Failure> {
    let loaded = load(target, false)?;
    Ok(Served {
        info: loaded.info(),
        root: loaded.target.root.clone(),
        extra: loaded.target.diagnostics.clone(),
        snapshot: loaded.outcome.snapshot,
    })
}

/// Opens a snapshot file directly. The source root is the snapshot's `repo`
/// when that is an existing directory (a local snapshot from the cache),
/// otherwise the file's own directory; `/source` serves only files the
/// snapshot lists, and a synthetic snapshot lists none.
///
/// Example: `open_file(Path::new("target/bench/synthetic.db"), false)`.
fn open_file(file: &Path, timings: bool) -> Result<Served, Failure> {
    let started = std::time::Instant::now();
    let usage = |why: String| {
        Failure::new(
            ErrorCode::Usage,
            format!("{} is not a usable snapshot: {why}", file.display()),
        )
    };
    let snapshot = match snapshot::load(file) {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err(usage(
                "missing, not a snapshot, or from another schema version".into(),
            ));
        }
        Err(e) => return Err(usage(e.to_string())),
    };
    if timings {
        eprintln!(
            "timings:\n  snapshot-load {:.3}s",
            started.elapsed().as_secs_f64()
        );
    }
    let repo = Path::new(&snapshot.repo);
    let root = if repo.is_dir() {
        repo.to_path_buf()
    } else {
        file.canonicalize()
            .ok()
            .and_then(|f| f.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
    };
    Ok(Served {
        info: SnapshotInfo {
            key: snapshot.key.clone(),
            repo: snapshot.repo.clone(),
            rev: snapshot.rev.clone(),
            dirty: false,
            analyzer: snapshot.analyzer.clone(),
        },
        root,
        extra: Vec::new(),
        snapshot,
    })
}
