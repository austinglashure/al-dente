//! Library handlers (`lib ls`, `lib usage`).

use aldente_model::ErrorCode;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::Response;

use super::{St, err, ok};
use crate::params::Params;

/// `GET /libs`: libraries in use with their counts (the CLI's `lib ls`).
///
/// Example: for clj-mini, `data.libraries` includes `lib:npm:react`.
pub async fn libs(State(s): St) -> Response {
    let report = aldente_analysis::libraries::summaries(&s.snapshot.graph, &s.snapshot.units);
    ok(&s, "libs", report)
}

/// `GET /lib?id=&by=namespace|symbol`: every use of one library (the CLI's
/// `lib usage`). `id` takes a full ID, a coordinate or an artifact name.
///
/// Example: `?id=react` gives `lib:npm:react`'s calls and requires.
pub async fn lib(State(s): St, RawQuery(q): RawQuery) -> Response {
    use aldente_analysis::libraries::{LibraryLookup, find_library, usage};
    use aldente_model::libraries::UsageGrouping;
    const TAG: &str = "lib-usage";
    let p = Params::parse(q.as_deref());
    let Some(query) = p.get("id") else {
        return err(
            StatusCode::BAD_REQUEST,
            TAG,
            ErrorCode::Usage,
            "id is required",
            None,
        );
    };
    let by = match p.get("by") {
        None | Some("namespace") => UsageGrouping::Namespace,
        Some("symbol") => UsageGrouping::Symbol,
        Some(_) => {
            return err(
                StatusCode::BAD_REQUEST,
                TAG,
                ErrorCode::Usage,
                "by is namespace or symbol",
                None,
            );
        }
    };
    match find_library(&s.snapshot.graph, query) {
        LibraryLookup::Found(id) => ok(&s, TAG, usage(&s.snapshot.graph, &id, by)),
        LibraryLookup::NotFound => err(
            StatusCode::NOT_FOUND,
            TAG,
            ErrorCode::UnknownSymbol,
            "no such library",
            None,
        ),
        LibraryLookup::Ambiguous(ids) => err(
            StatusCode::BAD_REQUEST,
            TAG,
            ErrorCode::AmbiguousSymbol,
            "several libraries match",
            Some(ids.iter().map(ToString::to_string).collect()),
        ),
    }
}
