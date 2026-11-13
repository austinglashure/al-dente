//! Free helpers shared by the resolver rules.

use super::*;

/// R12: an edge that exists for both `clj` and `cljs` (from the two reads of
/// a `.cljc` file) with the same certainty is stored once, without a
/// language, its sites combined. Edges that differ between the languages
/// keep theirs. Also returns how many call/reference sites were dropped as
/// duplicates (the same position seen by both reads), for [`UsageStats`].
///
/// Example: `both -calls-> now` from clj and from cljs becomes one edge with
/// `lang: None` and 1 dropped site; `now -calls-> java:…` (clj only) keeps
/// `lang: Some(Clj)`.
pub(super) fn merge_languages(edges: Vec<Edge>) -> (Vec<Edge>, usize) {
    type Key = (NodeId, NodeId, EdgeKind, Option<Reason>, Option<Certainty>);
    let mut groups: BTreeMap<Key, Vec<Edge>> = BTreeMap::new();
    for e in edges {
        groups
            .entry((e.from.clone(), e.to.clone(), e.kind, e.reason, e.certainty))
            .or_default()
            .push(e);
    }
    let mut out = Vec::new();
    let mut dropped = 0;
    for (_, group) in groups {
        let langs: BTreeSet<Option<Lang>> = group.iter().map(|e| e.lang).collect();
        let both = langs.contains(&Some(Lang::Clj)) && langs.contains(&Some(Lang::Cljs));
        if !both {
            out.extend(group);
            continue;
        }
        let mut merged = group[0].clone();
        merged.lang = None;
        merged.sites = Vec::new();
        for e in group {
            for site in e.sites {
                if !merged.sites.contains(&site) {
                    merged.sites.push(site);
                } else if matches!(merged.kind, EdgeKind::Calls | EdgeKind::References) {
                    dropped += 1;
                }
            }
        }
        out.push(merged);
    }
    (out, dropped)
}

/// Items grouped by a borrowed key, in input order within each group.
///
/// Example: usages grouped by file name.
pub(super) fn group<'a, T>(
    items: &'a [T],
    key: impl Fn(&'a T) -> &'a str,
) -> HashMap<&'a str, Vec<&'a T>> {
    let mut out: HashMap<&'a str, Vec<&'a T>> = HashMap::new();
    for item in items {
        out.entry(key(item)).or_default().push(item);
    }
    out
}

/// R1 subkind from `defined-by` (and the name for record constructors).
///
/// Example: `defined-by: clojure.core/defrecord`, name `->Card` gives `Fn`;
/// name `Card` gives `Record`.
pub(super) fn subkind(def: &VarDefinition) -> SymbolSubkind {
    let by = def
        .defined_by_lint_as
        .as_deref()
        .or(def.defined_by.as_deref())
        .map(|s| local(Some(s)))
        .unwrap_or("");
    if def.test == Some(true) {
        return SymbolSubkind::Test;
    }
    match by {
        "defn" | "defn-" | "fn" => SymbolSubkind::Fn,
        "defmacro" => SymbolSubkind::Macro,
        "defmulti" => SymbolSubkind::Multimethod,
        "defprotocol" if def.protocol_name.is_some() => SymbolSubkind::ProtocolMethod,
        "defprotocol" => SymbolSubkind::Protocol,
        "defrecord" | "deftype" if def.name.starts_with("->") || def.name.starts_with("map->") => {
            SymbolSubkind::Fn
        }
        "defrecord" => SymbolSubkind::Record,
        "deftype" => SymbolSubkind::Type,
        "definterface" => SymbolSubkind::Interface,
        "deftest" => SymbolSubkind::Test,
        "def" | "defonce" => SymbolSubkind::Var,
        _ if def.arglist_strs.as_ref().is_some_and(|a| !a.is_empty()) => SymbolSubkind::Fn,
        _ => SymbolSubkind::Var,
    }
}

/// Namespace prefixes a Maven coordinate suggests: the artifact, the
/// artifact with `-` as `.`, the group, and the group's last part joined to
/// the artifact (both spellings).
///
/// Example: `org.clojure/core.async` gives `core.async`, `org.clojure`,
/// `clojure.core.async`; `metosin/reitit-core` gives `reitit-core`,
/// `reitit.core`, `metosin`, `metosin.reitit-core`, `metosin.reitit.core`.
pub(super) fn coordinate_prefixes(coord: &str) -> Vec<String> {
    let Some((group, artifact)) = coord.split_once('/') else {
        return Vec::new();
    };
    let dotted = artifact.replace('-', ".");
    let last = group.rsplit('.').next().unwrap_or(group);
    let mut out = vec![
        artifact.to_owned(),
        dotted.clone(),
        group.to_owned(),
        format!("{last}.{artifact}"),
        format!("{last}.{dotted}"),
    ];
    out.sort();
    out.dedup();
    out
}

/// The part of a symbol after its namespace; `""` for `None`.
///
/// Example: `local(Some("clojure.core/defn"))` is `"defn"`; `local(Some("/"))` is `"/"`.
pub(super) fn local(sym: Option<&str>) -> &str {
    let Some(sym) = sym else { return "" };
    match sym.rsplit_once('/') {
        Some((_, n)) if !n.is_empty() => n,
        _ => sym,
    }
}

/// The language of a record: clj-kondo's `lang` for `.cljc`, else the extension.
///
/// Example: `lang_of(None, "src/a.cljs")` is `Cljs`; `lang_of(Some("clj"), "a.cljc")` is `Clj`.
pub(super) fn lang_of(lang: Option<&str>, file: &str) -> Lang {
    lang.and_then(Lang::from_token)
        .unwrap_or(if file.ends_with(".cljs") {
            Lang::Cljs
        } else {
            Lang::Clj
        })
}

/// A location from clj-kondo coordinates; `None` if the file is not a valid path.
///
/// Example: `location("src/a.clj", 1, 1, Some(3), Some(9))`.
pub(super) fn location(
    file: &str,
    line: u32,
    col: u32,
    end_line: Option<u32>,
    end_col: Option<u32>,
) -> Option<Location> {
    Some(Location {
        file: RepoPath::parse(file).ok()?,
        line,
        col,
        end_line: end_line.unwrap_or(line),
        end_col: end_col.unwrap_or(col),
    })
}

/// The record or type a constructor call builds: `(->Rec …)`,
/// `(map->Rec …)` and `(Rec. …)` give `Rec`; anything else `None`.
///
/// Example: `constructor_type("(s/->Circle 2)")` is `Some("Circle")`.
pub(super) fn constructor_type(text: &str) -> Option<String> {
    let inner = text.strip_prefix('(')?;
    let head: String = inner
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != ')')
        .collect();
    let name = local(Some(&head));
    if let Some(t) = name
        .strip_prefix("map->")
        .or_else(|| name.strip_prefix("->"))
    {
        return (!t.is_empty()).then(|| t.to_owned());
    }
    let t = head.strip_suffix('.')?;
    (!t.is_empty()).then(|| t.to_owned())
}

/// A usage's position, if clj-kondo gave one.
///
/// Example: a usage at row 3, col 5 gives `Some(Pos { line: 3, col: 5 })`.
pub(super) fn upos(u: &VarUsage) -> Option<Pos> {
    Some(Pos {
        line: u.row?,
        col: u.col?,
    })
}

/// A usage's name position, falling back to its position.
///
/// Example: `(f x)` at 3:5 has name position 3:6.
pub(super) fn uname_pos(u: &VarUsage) -> Option<Pos> {
    let pos = upos(u)?;
    Some(Pos {
        line: u.name_row.unwrap_or(pos.line),
        col: u.name_col.unwrap_or(pos.col),
    })
}

/// A usage's site, if it has a position and a valid file.
///
/// Example: a usage in `src/a.clj` at 3:5 gives that site.
pub(super) fn site_of(u: &VarUsage) -> Option<Site> {
    let pos = upos(u)?;
    site(&u.filename, pos.line, pos.col)
}

/// A site; `None` if the file is not a valid path.
///
/// Example: `site("src/a.clj", 3, 5)`.
pub(super) fn site(file: &str, line: u32, col: u32) -> Option<Site> {
    Some(Site {
        file: RepoPath::parse(file).ok()?,
        line,
        col,
    })
}

/// Whether a var definition's range contains `pos`.
///
/// Example: a def spanning 3:1–5:20 contains 4:7.
pub(super) fn def_span_contains(d: &VarDefinition, pos: Pos) -> bool {
    let start = Pos {
        line: d.row,
        col: d.col,
    };
    let end = Pos {
        line: d.end_row.unwrap_or(d.row),
        col: d.end_col.unwrap_or(d.col),
    };
    start <= pos && pos < end
}

/// A platform node for a JS global.
///
/// Example: `js:console.log` named `js/console.log`.
pub(super) fn platform_node(id: &NodeId, name: &str) -> Node {
    Node {
        id: id.clone(),
        kind: NodeKind::Platform,
        subkind: None,
        name: name.to_owned(),
        namespace: None,
        unit: None,
        langs: BTreeSet::new(),
        flags: BTreeSet::new(),
        loc: None,
        doc: None,
        signature: None,
    }
}
