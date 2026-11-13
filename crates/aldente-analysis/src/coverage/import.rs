//! Measured coverage reports: LCOV (Cloverage `--lcov`, ClojureScript
//! tools) and Codecov JSON (Cloverage `--codecov`). Report paths are
//! matched to repository files by the longest shared path tail, since
//! Cloverage writes paths relative to a source root (`shop/core.clj`).

use std::collections::{BTreeMap, BTreeSet};

use aldente_model::coverage::ImportInfo;
use thiserror::Error;

/// Hits per line (1-based) per repository file.
pub type LineHits = BTreeMap<String, BTreeMap<u32, u64>>;

/// Why a report could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("not an LCOV or Codecov JSON coverage report")]
    UnknownFormat,
    #[error("bad Codecov JSON: {0}")]
    Json(String),
}

/// Parses a report and maps it onto `repo_files` (repository-relative,
/// forward slashes).
///
/// Example: an LCOV report with `SF:shop/core.clj` and `DA:3,1` gives hits
/// `{"src/shop/core.clj": {3: 1}}` when that is the repository file.
pub fn parse_report(
    text: &str,
    repo_files: &BTreeSet<String>,
) -> Result<(LineHits, ImportInfo), ParseError> {
    let (format, raw) = if text.trim_start().starts_with('{') {
        ("codecov-json", parse_codecov(text)?)
    } else if text.lines().any(|l| l.starts_with("SF:")) {
        ("lcov", parse_lcov(text))
    } else {
        return Err(ParseError::UnknownFormat);
    };
    let mut hits: LineHits = BTreeMap::new();
    let mut unmatched = Vec::new();
    let mut matched = 0;
    for (path, lines) in &raw {
        match match_file(path, repo_files) {
            Some(file) => {
                matched += 1;
                let entry = hits.entry(file.to_owned()).or_default();
                for (&l, &h) in lines {
                    *entry.entry(l).or_insert(0) += h;
                }
            }
            None => unmatched.push(path.clone()),
        }
    }
    unmatched.sort();
    Ok((
        hits,
        ImportInfo {
            format: format.into(),
            files: raw.len() as u32,
            matched,
            unmatched,
        },
    ))
}

/// LCOV: `SF:<path>`, then `DA:<line>,<hits>[,<checksum>]`, until
/// `end_of_record`. Other records are ignored.
///
/// Example: `"SF:a.clj\nDA:1,2\nend_of_record\n"` gives `{"a.clj": {1: 2}}`.
fn parse_lcov(text: &str) -> BTreeMap<String, BTreeMap<u32, u64>> {
    let mut out: BTreeMap<String, BTreeMap<u32, u64>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines().map(str::trim) {
        if let Some(path) = line.strip_prefix("SF:") {
            current = Some(normalize(path));
            out.entry(normalize(path)).or_default();
        } else if let (Some(da), Some(file)) = (line.strip_prefix("DA:"), &current) {
            let mut parts = da.split(',');
            if let (Some(Ok(l)), Some(Ok(h))) = (
                parts.next().map(str::parse::<u32>),
                parts.next().map(str::parse::<u64>),
            ) && let Some(m) = out.get_mut(file)
            {
                *m.entry(l).or_insert(0) += h;
            }
        } else if line == "end_of_record" {
            current = None;
        }
    }
    out
}

/// Codecov JSON: `{"coverage": {"<path>": [null, hits, …]}}`, index =
/// line number, `null` = not instrumented. Hits may also be `"1/2"`
/// (partial branches), counted as covered.
///
/// Example: `{"coverage": {"a.clj": [null, 1, 0]}}` gives `{1: 1, 2: 0}`.
fn parse_codecov(text: &str) -> Result<BTreeMap<String, BTreeMap<u32, u64>>, ParseError> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ParseError::Json(e.to_string()))?;
    let files = v
        .get("coverage")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ParseError::Json("no \"coverage\" object".into()))?;
    let mut out = BTreeMap::new();
    for (path, lines) in files {
        let mut m = BTreeMap::new();
        for (i, h) in lines.as_array().into_iter().flatten().enumerate() {
            let hits = match h {
                serde_json::Value::Number(n) => n.as_u64(),
                serde_json::Value::String(s) => Some(u64::from(!s.starts_with("0/"))),
                serde_json::Value::Bool(b) => Some(u64::from(*b)),
                _ => None,
            };
            if let (Some(hits), Ok(line)) = (hits, u32::try_from(i)) {
                m.insert(line, hits);
            }
        }
        out.insert(normalize(path), m);
    }
    Ok(out)
}

/// Forward slashes, no leading `./`.
///
/// Example: `normalize(".\\src\\a.clj")` is `"src/a.clj"`.
fn normalize(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    p.strip_prefix("./").unwrap_or(&p).to_owned()
}

/// The repository file sharing the longest whole-segment tail with `path`
/// (at least the file name); `None` when nothing or several tie.
///
/// Example: `shop/core.clj` matches `src/shop/core.clj`;
/// `/home/u/proj/src/shop/core.clj` too.
fn match_file<'a>(path: &str, repo_files: &'a BTreeSet<String>) -> Option<&'a str> {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut best: Option<(&str, usize)> = None;
    let mut tie = false;
    for f in repo_files {
        let fs: Vec<&str> = f.split('/').collect();
        let shared = segs
            .iter()
            .rev()
            .zip(fs.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        if shared == 0 {
            continue;
        }
        match best {
            Some((_, n)) if shared < n => {}
            Some((_, n)) if shared == n => tie = true,
            _ => {
                best = Some((f.as_str(), shared));
                tie = false;
            }
        }
    }
    if tie { None } else { best.map(|(f, _)| f) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> BTreeSet<String> {
        [
            "src/shop/core.clj",
            "src/shop/util.clj",
            "test/shop/core_test.clj",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
    }

    #[test]
    fn lcov_maps_paths_by_tail() {
        let text = "TN:\nSF:shop/core.clj\nDA:3,1\nDA:4,0\nLF:2\nend_of_record\n\
                    SF:/abs/proj/src/shop/util.clj\nDA:1,5,abc\nend_of_record\nSF:other/x.clj\nend_of_record\n";
        let (hits, info) = parse_report(text, &files()).unwrap();
        assert_eq!(info.format, "lcov");
        assert_eq!((info.files, info.matched), (3, 2));
        assert_eq!(info.unmatched, vec!["other/x.clj"]);
        assert_eq!(hits["src/shop/core.clj"], BTreeMap::from([(3, 1), (4, 0)]));
        assert_eq!(hits["src/shop/util.clj"][&1], 5);
    }

    #[test]
    fn codecov_json_and_unknown_formats() {
        let text = r#"{"coverage": {"shop/core.clj": [null, 1, 0, "1/2", null]}}"#;
        let (hits, info) = parse_report(text, &files()).unwrap();
        assert_eq!(info.format, "codecov-json");
        assert_eq!(
            hits["src/shop/core.clj"],
            BTreeMap::from([(1, 1), (2, 0), (3, 1)])
        );
        assert_eq!(
            parse_report("hello", &files()),
            Err(ParseError::UnknownFormat)
        );
        assert!(matches!(
            parse_report("{", &files()),
            Err(ParseError::Json(_))
        ));
    }

    #[test]
    fn ambiguous_tails_do_not_match() {
        let f: BTreeSet<String> = ["a/core.clj", "b/core.clj"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(match_file("core.clj", &f), None);
        assert_eq!(match_file("x/b/core.clj", &f), Some("b/core.clj"));
    }
}
