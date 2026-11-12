//! `serve --timings`: one stderr line per request with its wall time.

use std::time::Duration;

/// The line printed for one request. The path and query carry repository
/// names, so control characters are escaped (no terminal escape injection).
///
/// Example: `request_line("GET", "/api/v1/graph", 200, 12ms, Some(2048))` is
/// `"timing GET /api/v1/graph 200 12.0ms 2048B"`.
pub fn request_line(
    method: &str,
    path_and_query: &str,
    status: u16,
    took: Duration,
    bytes: Option<u64>,
) -> String {
    let safe: String = path_and_query
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_unicode().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect();
    let size = bytes.map(|b| format!(" {b}B")).unwrap_or_default();
    format!(
        "timing {method} {safe} {status} {:.1}ms{size}",
        took.as_secs_f64() * 1000.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_request_line_has_method_path_status_time_and_size() {
        let line = request_line(
            "GET",
            "/api/v1/neighborhood?id=clj:.//a/b",
            200,
            Duration::from_micros(12_345),
            Some(2048),
        );
        assert_eq!(
            line,
            "timing GET /api/v1/neighborhood?id=clj:.//a/b 200 12.3ms 2048B"
        );
    }

    #[test]
    fn control_characters_from_repository_names_are_escaped() {
        let line = request_line(
            "GET",
            "/api/v1/node?id=a\u{1b}[31mb\n",
            404,
            Duration::ZERO,
            None,
        );
        assert!(!line.contains('\u{1b}') && !line.contains('\n'), "{line:?}");
        assert!(line.contains("\\u{1b}"), "{line}");
    }
}
