//! A stable, sorted, human-readable text form of a graph for golden files:
//! one node or edge per line.

use std::fmt::Write;

use aldente_model::{Edge, GraphPayload, Node};
use serde::Serialize;

/// Renders `graph` for a golden file. Lines are sorted, so input order never
/// changes the output.
///
/// Example: an empty payload renders as
/// `"# nodes 0\n# edges 0\n# truncated false\n"`.
pub fn render_graph(graph: &GraphPayload) -> String {
    let mut nodes: Vec<String> = graph.nodes.iter().map(node_line).collect();
    let mut edges: Vec<String> = graph.edges.iter().map(edge_line).collect();
    nodes.sort();
    edges.sort();
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(out, "# nodes {}", nodes.len());
    nodes.iter().for_each(|l| out.push_str(l));
    let _ = writeln!(out, "# edges {}", edges.len());
    edges.iter().for_each(|l| out.push_str(l));
    let _ = writeln!(out, "# truncated {}", graph.truncated);
    if let Some(agg) = &graph.aggregated {
        let _ = writeln!(out, "# aggregated {}", token(&agg.level));
    }
    out
}

/// One node: `node <id> <kind>[/<subkind>] [flags] langs:<a>,<b>`.
///
/// Example: `node clj:.//a/f symbol/fn [private]`.
fn node_line(node: &Node) -> String {
    let mut line = format!("node {} {}", clean(&node.id.to_string()), token(&node.kind));
    if let Some(sub) = &node.subkind {
        let _ = write!(line, "/{}", token(sub));
    }
    if !node.flags.is_empty() {
        let flags: Vec<String> = node.flags.iter().map(token).collect();
        let _ = write!(line, " [{}]", flags.join(" "));
    }
    if !node.langs.is_empty() {
        let langs: Vec<String> = node.langs.iter().map(token).collect();
        let _ = write!(line, " langs:{}", langs.join(","));
    }
    line.push('\n');
    line
}

/// One edge: `edge <from> -<kind>(<certainty> <reason> <lang>)-> <to> [x<count>
/// [<by certainty>]] @ <sites>`; the count part only on rolled-up edges.
///
/// Example: `edge clj:.//a/f -calls(certain direct)-> ext:clj//clojure.core/map @ src/a.clj:3:2`.
fn edge_line(edge: &Edge) -> String {
    let attrs: Vec<String> = [
        edge.certainty.as_ref().map(token),
        edge.reason.as_ref().map(token),
        edge.lang.as_ref().map(token),
    ]
    .into_iter()
    .flatten()
    .collect();
    let label = if attrs.is_empty() {
        token(&edge.kind)
    } else {
        format!("{}({})", token(&edge.kind), attrs.join(" "))
    };
    let mut line = format!(
        "edge {} -{}-> {}",
        clean(&edge.from.to_string()),
        label,
        clean(&edge.to.to_string())
    );
    if let Some(count) = edge.count {
        let _ = write!(line, " x{count}");
        if let Some(by) = &edge.by_certainty {
            let parts: Vec<String> = by
                .iter()
                .map(|(c, n)| format!("{}:{n}", token(c)))
                .collect();
            let _ = write!(line, " [{}]", parts.join(" "));
        }
    }
    let mut sites: Vec<_> = edge.sites.iter().collect();
    sites.sort_by(|a, b| (&a.file, a.line, a.col).cmp(&(&b.file, b.line, b.col)));
    if !sites.is_empty() {
        line.push_str(" @");
        for s in sites {
            let _ = write!(line, " {}:{}:{}", clean(s.file.as_str()), s.line, s.col);
        }
    }
    line.push('\n');
    line
}

/// The JSON name of a model enum value.
///
/// Example: `token(&EdgeKind::TalksTo)` is `"talks-to"`.
fn token<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        other => format!("{other:?}"),
    }
}

/// Escapes backslashes and control characters (newlines, terminal escapes) so
/// repository strings can never split or recolor a golden line.
///
/// Example: `clean("a\nb")` is `"a\\u{a}b"` (a literal backslash-u sequence).
fn clean(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{Certainty, Edge, EdgeKind, Node, NodeKind, Reason, RepoPath, Site};
    use std::collections::BTreeSet;

    fn node(id: &str, kind: NodeKind) -> Node {
        Node {
            id: id.parse().unwrap(),
            kind,
            subkind: None,
            name: id.to_owned(),
            namespace: None,
            unit: None,
            langs: BTreeSet::new(),
            flags: BTreeSet::new(),
            loc: None,
            doc: None,
            signature: None,
        }
    }

    fn site(file: &str, line: u32, col: u32) -> Site {
        Site {
            file: RepoPath::parse(file).unwrap(),
            line,
            col,
        }
    }

    #[test]
    fn empty_graph() {
        assert_eq!(
            render_graph(&GraphPayload::default()),
            "# nodes 0\n# edges 0\n# truncated false\n"
        );
    }

    #[test]
    fn nodes_and_edges_are_sorted_and_one_per_line() {
        let mut f = node("clj:.//app.core/f", NodeKind::Symbol);
        f.subkind = Some(aldente_model::SymbolSubkind::Fn);
        f.flags = BTreeSet::from([
            aldente_model::SymbolFlag::Private,
            aldente_model::SymbolFlag::EntryPoint,
        ]);
        let graph = GraphPayload {
            nodes: vec![f, node("clj:.//app.core", NodeKind::Namespace)],
            edges: vec![
                Edge {
                    from: "clj:.//app.core/f".parse().unwrap(),
                    to: "ext:clj//clojure.core/map".parse().unwrap(),
                    kind: EdgeKind::References,
                    certainty: Some(Certainty::Likely),
                    reason: Some(Reason::ValueRef),
                    lang: None,
                    sites: vec![],
                    count: None,
                    by_certainty: None,
                },
                Edge {
                    from: "clj:.//app.core/f".parse().unwrap(),
                    to: "ext:clj//clojure.core/map".parse().unwrap(),
                    kind: EdgeKind::Calls,
                    certainty: Some(Certainty::Certain),
                    reason: Some(Reason::Direct),
                    lang: Some(aldente_model::Lang::Cljs),
                    sites: vec![site("src/b.clj", 9, 1), site("src/a.clj", 3, 2)],
                    count: None,
                    by_certainty: None,
                },
                Edge {
                    from: "unit:.".parse().unwrap(),
                    to: "clj:.//app.core".parse().unwrap(),
                    kind: EdgeKind::Contains,
                    certainty: None,
                    reason: None,
                    lang: None,
                    sites: vec![],
                    count: None,
                    by_certainty: None,
                },
            ],
            truncated: false,
            aggregated: None,
        };
        assert_eq!(
            render_graph(&graph),
            "\
# nodes 2
node clj:.//app.core namespace
node clj:.//app.core/f symbol/fn [private entry-point]
# edges 3
edge clj:.//app.core/f -calls(certain direct cljs)-> ext:clj//clojure.core/map @ src/a.clj:3:2 src/b.clj:9:1
edge clj:.//app.core/f -references(likely value-ref)-> ext:clj//clojure.core/map
edge unit:. -contains-> clj:.//app.core
# truncated false
"
        );
    }

    #[test]
    fn hostile_strings_cannot_break_lines() {
        let id = "hole:dynamic-eval@evil\nname\u{1b}[31m.clj:1:1";
        let graph = GraphPayload {
            nodes: vec![node(id, NodeKind::Hole)],
            ..GraphPayload::default()
        };
        let text = render_graph(&graph);
        assert_eq!(text.lines().count(), 4, "{text}");
        assert!(text.contains(r"evil\u{a}name\u{1b}[31m.clj"), "{text}");
    }
}
