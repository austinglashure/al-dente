//! `aldente serve [TARGET]`: the local GUI and API on 127.0.0.1.

use aldente_model::analyze::ServeInfo;
use aldente_model::{Envelope, ErrorCode, ExitCode};
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
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "serve";

/// Loads the snapshot, starts the server, prints the URL (with the session
/// token), opens the browser unless `--no-open`, and serves until Ctrl-C.
///
/// Example: `aldente serve --no-open .` prints `http://127.0.0.1:41733/?token=…`.
pub fn run(args: &ServeArgs, json: bool) -> anyhow::Result<ExitCode> {
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let info = loaded.info();
    let root = loaded.target.root.clone();
    let extra = loaded.target.diagnostics.clone();
    let token = new_token();
    let snapshot = loaded.outcome.snapshot;
    let state_info = info.clone();
    let no_open = args.no_open;
    let result = serve(
        move |port| AppState::new(snapshot, root, state_info, &extra, token, port),
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
