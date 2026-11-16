//! `POST /open`: opens a node's or a site's location in the user's editor
//! (`06-gui.md`, "Source and editor links"; `07-safety.md`). The command
//! comes only from the user's config; the request names a location and
//! nothing else; the file must be one the snapshot analyzed, inside the
//! tree; placeholders become separate arguments and no shell is involved.

use std::path::Path;
use std::process::{Command, Stdio};

use aldente_model::query::OpenResult;
use aldente_model::{ErrorCode, RepoPath};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use super::{St, err, ok, resolve_id};

/// What the UI may send: a node ID, or a site's file and line. Anything
/// else (a command, say) is refused.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenRequest {
    id: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    col: Option<u32>,
}

/// The editor's argument vector: each template element with `{file}`,
/// `{line}` and `{col}` replaced; the file path stays inside the element
/// that names it, whatever characters it has.
///
/// Example: `["emacsclient", "-n", "+{line}:{col}", "{file}"]` with
/// `/r/it's a "f".clj`, 3, 1 gives `["emacsclient", "-n", "+3:1", "/r/it's a \"f\".clj"]`.
fn editor_argv(template: &[String], file: &Path, line: u32, col: u32) -> Vec<String> {
    let file = file.to_string_lossy();
    template
        .iter()
        .map(|t| {
            t.replace("{line}", &line.to_string())
                .replace("{col}", &col.to_string())
                .replace("{file}", &file)
        })
        .collect()
}

/// `POST /open` with `{"id": …}` or `{"file": …, "line": …, "col": …}`.
///
/// Example: `{"id": "clj:.//shop.core/checkout"}` with no editor configured
/// gives `{opened: false, location: "src/shop/core.clj:12:1"}`.
pub async fn open(State(s): St, body: String) -> Response {
    let bad = |m: &str| err(StatusCode::BAD_REQUEST, "open", ErrorCode::Usage, m, None);
    let refused = |m: &str| {
        err(
            StatusCode::NOT_FOUND,
            "open",
            ErrorCode::UnknownSymbol,
            m,
            None,
        )
    };
    let Ok(req) = serde_json::from_str::<OpenRequest>(&body) else {
        return bad("the body must be {\"id\": …} or {\"file\": …, \"line\": …}");
    };
    let (file, line, col) = match (&req.id, &req.file) {
        (Some(id), None) => {
            let id = match resolve_id(&s, "open", Some(id)) {
                Ok(id) => id,
                Err(res) => return *res,
            };
            match s.snapshot.graph.nodes.get(&id).and_then(|n| n.loc.clone()) {
                Some(loc) => (loc.file, loc.line, loc.col),
                None => return refused("this node has no source location"),
            }
        }
        (None, Some(file)) => {
            let (Ok(file), Some(line)) = (RepoPath::parse(file), req.line) else {
                return bad("a site needs a repository-relative file and a line");
            };
            (file, line, req.col.unwrap_or(1))
        }
        _ => return bad("name either a node (id) or a site (file and line)"),
    };
    if !s.snapshot.files.iter().any(|(f, _)| f == &file) {
        return refused("the file is not part of the snapshot");
    }
    let Ok(path) = aldente_acquire::walk::inside_path(&s.root, &file) else {
        return refused("the file is not inside the analyzed tree");
    };
    let location = format!("{file}:{line}:{col}");
    let Some(template) = s.editor.as_ref().filter(|t| !t.is_empty()) else {
        return ok(
            &s,
            "open",
            OpenResult {
                opened: false,
                location,
            },
        );
    };
    let argv = editor_argv(template, &path, line, col);
    let spawned = argv.split_first().map(|(program, args)| {
        Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    });
    match spawned {
        Some(Ok(_)) => ok(
            &s,
            "open",
            OpenResult {
                opened: true,
                location,
            },
        ),
        Some(Err(e)) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "open",
            ErrorCode::Internal,
            &format!("the editor command failed to start: {e}"),
            None,
        ),
        None => bad("the editor command is empty"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_fill_whole_arguments_and_the_path_stays_one() {
        let t: Vec<String> = ["emacsclient", "-n", "+{line}:{col}", "{file}"]
            .map(String::from)
            .to_vec();
        let argv = editor_argv(&t, Path::new("/r/it's a \"f\".clj"), 3, 1);
        assert_eq!(argv, ["emacsclient", "-n", "+3:1", "/r/it's a \"f\".clj"]);
    }
}
