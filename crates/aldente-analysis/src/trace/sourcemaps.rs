//! Mapping JavaScript frame positions back to ClojureScript source with
//! source maps (v3), then to repository files.

use std::collections::BTreeSet;

use aldente_graph::Graph;
use aldente_model::NodeKind;
use aldente_model::trace::{Frame, FrameLang};
use sourcemap::SourceMap;

/// A source map the user handed over, with the path it was read from.
pub struct SourceMapFile {
    /// As given on the command line; used to tell which generated file the
    /// map describes (`out/app/core.js.map` describes `out/app/core.js`).
    pub path: String,
    pub map: SourceMap,
}

impl SourceMapFile {
    /// Parses a map.
    ///
    /// Example: `SourceMapFile::parse("out/app/core.js.map", bytes)`.
    pub fn parse(path: &str, bytes: &[u8]) -> Result<SourceMapFile, sourcemap::Error> {
        Ok(SourceMapFile {
            path: path.replace('\\', "/"),
            map: SourceMap::from_slice(bytes)?,
        })
    }

    /// Path components of the generated file this map describes.
    ///
    /// Example: `out/app/core.js.map` gives `["out", "app", "core.js"]`.
    fn generated(&self) -> Vec<&str> {
        let p = self.path.strip_suffix(".map").unwrap_or(&self.path);
        p.split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .collect()
    }

    /// The largest generated line the map has a mapping on (1-based).
    ///
    /// Example: 31 for the clj-mini cljs fixture map.
    fn last_line(&self) -> u32 {
        self.map
            .tokens()
            .map(|t| t.get_dst_line() + 1)
            .max()
            .unwrap_or(0)
    }
}

/// Rewrites each JavaScript frame that a map covers: file and line become
/// the repository's ClojureScript file and original position, the language
/// `cljs`, and the namespace that file defines (when the frame's own name
/// gave none).
///
/// Which map covers a frame: the one whose generated path shares the
/// longest tail with the frame's URL path (at least the file name), and
/// whose mappings reach the frame's line. With one shared component only
/// (the file name), a map is used when no other map shares more.
///
/// Example: `at app$core$parse_qty (/srv/app/out/app/core.js:7:8)` with
/// `out/app/core.js.map` becomes `src/app/core.cljs` line 7.
pub fn apply(graph: &Graph, maps: &[SourceMapFile], frames: &mut [Frame]) {
    if maps.is_empty() {
        return;
    }
    let files = repo_cljs_files(graph);
    for f in frames.iter_mut().filter(|f| f.lang == FrameLang::Js) {
        let (Some(url), Some(line), Some(col)) = (f.file.clone(), f.line, f.col) else {
            continue;
        };
        let Some(map) = best_map(maps, &url, line) else {
            continue;
        };
        let Some(token) = map.map.lookup_token(line - 1, col.saturating_sub(1)) else {
            continue;
        };
        let Some(source) = token.get_source() else {
            continue;
        };
        let Some(repo_file) = repo_file(&files, map, source, f.namespace.as_deref()) else {
            continue;
        };
        f.file = Some(repo_file.clone());
        f.line = Some(token.get_src_line() + 1);
        f.col = Some(token.get_src_col() + 1);
        f.lang = FrameLang::Cljs;
        if f.namespace.is_none() {
            f.namespace = namespace_of(graph, &repo_file);
        }
    }
}

/// See [`apply`].
///
/// Example: for URL `/srv/app/out/cljs/core.js` line 18041 and only the map
/// of `out/app/core.js`, no map (one shared component, and the line is
/// beyond the map).
fn best_map<'a>(maps: &'a [SourceMapFile], url: &str, line: u32) -> Option<&'a SourceMapFile> {
    let path = url_path(url);
    maps.iter()
        .map(|m| (tail_len(&path, &m.generated()), m))
        .filter(|(n, m)| *n >= 1 && line <= m.last_line())
        .max_by_key(|(n, _)| *n)
        .map(|(_, m)| m)
}

/// The path part of a URL or file name, as components.
///
/// Example: `url_path("http://h:8080/out/app/core.js?v=1")` is `["out", "app", "core.js"]`.
fn url_path(url: &str) -> Vec<&str> {
    let no_scheme = url.split_once("://").map_or(url, |(_, rest)| {
        rest.split_once('/').map_or("", |(_, path)| path)
    });
    let no_query = no_scheme.split(['?', '#']).next().unwrap_or(no_scheme);
    no_query
        .split(['/', '\\'])
        .filter(|c| !c.is_empty())
        .collect()
}

/// How many trailing components `a` and `b` share.
///
/// Example: `tail_len(["x","app","core.js"], ["out","app","core.js"])` is 2.
fn tail_len(a: &[&str], b: &[&str]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}

/// Every repository file that holds ClojureScript definitions.
///
/// Example: `{"src/app/core.cljs"}` for the cljs fixture.
fn repo_cljs_files(graph: &Graph) -> BTreeSet<String> {
    graph
        .nodes
        .values()
        .filter_map(|n| n.loc.as_ref())
        .map(|l| l.file.to_string())
        .filter(|f| f.ends_with(".cljs") || f.ends_with(".cljc"))
        .collect()
}

/// The repository file a map's `source` names: the one sharing the longest
/// path tail with the source (resolved against the map's directory); on a
/// tie, the one whose path matches the frame's namespace.
///
/// Example: source `core.cljs` of `out/app/core.js.map` with namespace
/// `app.core` gives `src/app/core.cljs`.
fn repo_file(
    files: &BTreeSet<String>,
    map: &SourceMapFile,
    source: &str,
    ns: Option<&str>,
) -> Option<String> {
    let mut source_path: Vec<String> = map
        .generated()
        .split_last()
        .map(|(_, dir)| dir.iter().map(|s| (*s).to_owned()).collect())
        .unwrap_or_default();
    source_path.extend(url_path(source).iter().map(|s| (*s).to_owned()));
    let source_refs: Vec<&str> = source_path.iter().map(String::as_str).collect();
    let ns_tail: Vec<String> = ns
        .map(|n| n.replace('-', "_").split('.').map(str::to_owned).collect())
        .unwrap_or_default();
    files
        .iter()
        .map(|f| {
            let parts: Vec<&str> = f.split('/').collect();
            let shared = tail_len(&parts, &source_refs);
            let stem: Vec<String> = parts
                .iter()
                .map(|p| p.rsplit_once('.').map_or(*p, |(s, _)| s).to_owned())
                .collect();
            let ns_match = !ns_tail.is_empty() && stem.ends_with(&ns_tail);
            ((shared, ns_match), f)
        })
        .filter(|((shared, _), _)| *shared >= 1)
        .max_by_key(|(key, _)| *key)
        .map(|(_, f)| f.clone())
}

/// The namespace defined in `file`.
///
/// Example: `src/app/core.cljs` gives `app.core`.
fn namespace_of(graph: &Graph, file: &str) -> Option<String> {
    graph
        .nodes
        .values()
        .find(|n| {
            n.kind == NodeKind::Namespace && n.loc.as_ref().is_some_and(|l| l.file.as_str() == file)
        })
        .map(|n| n.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_tails() {
        assert_eq!(
            url_path("http://h:8080/out/app/core.js?v=1"),
            ["out", "app", "core.js"]
        );
        assert_eq!(
            url_path("/srv/app/out/app/core.js"),
            ["srv", "app", "out", "app", "core.js"]
        );
        assert_eq!(
            tail_len(&["x", "app", "core.js"], &["out", "app", "core.js"]),
            2
        );
        assert_eq!(tail_len(&["cljs", "core.js"], &["app", "core.js"]), 1);
    }
}
