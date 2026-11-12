//! Graph payloads as text, DOT, Mermaid and GraphML. Every repository string
//! is escaped for its format, so a hostile name cannot break out of a label
//! or a line.

use std::collections::BTreeMap;
use std::fmt::Write;

use aldente_model::{Certainty, Edge, GraphPayload, Node, NodeId};

use crate::output::{printable, token};

/// A node's kind as `kind` or `kind/subkind`.
///
/// Example: `symbol/fn`.
fn kind(n: &Node) -> String {
    match &n.subkind {
        Some(sub) => format!("{}/{}", token(&n.kind), token(sub)),
        None => token(&n.kind),
    }
}

/// An edge's label: kind, then certainty and reason, then `xN` when rolled up.
///
/// Example: `calls certain direct x3`.
fn edge_label(e: &Edge) -> String {
    let mut parts = vec![token(&e.kind)];
    parts.extend(e.certainty.as_ref().map(token));
    parts.extend(e.reason.as_ref().map(token));
    parts.extend(e.lang.as_ref().map(token));
    if let Some(n) = e.count {
        parts.push(format!("x{n}"));
    }
    parts.join(" ")
}

/// Plain text: a line per node, then a line per edge.
///
/// Example: `node clj:.//a  namespace` and `edge clj:.//a -> clj:.//b  calls certain direct x2`.
pub fn text(p: &GraphPayload) -> Vec<String> {
    let mut lines = vec![format!("nodes ({}):", p.nodes.len())];
    lines.extend(
        p.nodes
            .iter()
            .map(|n| printable(&format!("  {}  {}", n.id, kind(n)))),
    );
    lines.push(format!("edges ({}):", p.edges.len()));
    lines.extend(
        p.edges
            .iter()
            .map(|e| printable(&format!("  {} -> {}  {}", e.from, e.to, edge_label(e)))),
    );
    if p.truncated {
        lines.push("(truncated)".into());
    }
    lines
}

/// Escapes a DOT double-quoted string.
///
/// Example: `dot_quote("a\"b\nc")` is `"\"a\\\"b\\nc\""`.
fn dot_quote(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Line style per certainty: solid, dashed, dotted.
///
/// Example: `style(Some(Certainty::Possible))` is `"dotted"`.
fn style(c: Option<Certainty>) -> &'static str {
    match c {
        Some(Certainty::Possible) => "dotted",
        Some(Certainty::Likely) => "dashed",
        _ => "solid",
    }
}

/// Graphviz DOT.
///
/// Example: `digraph aldente { "clj:.//a" [label="a"]; "clj:.//a" -> "clj:.//b" [label="calls", style=solid]; }`.
pub fn dot(p: &GraphPayload) -> String {
    let mut out = String::from("digraph aldente {\n  rankdir=LR;\n  node [shape=box];\n");
    for n in &p.nodes {
        let _ = writeln!(
            out,
            "  {} [label={}, tooltip={}];",
            dot_quote(&n.id.to_string()),
            dot_quote(&n.name),
            dot_quote(&kind(n))
        );
    }
    for e in &p.edges {
        let _ = writeln!(
            out,
            "  {} -> {} [label={}, style={}];",
            dot_quote(&e.from.to_string()),
            dot_quote(&e.to.to_string()),
            dot_quote(&edge_label(e)),
            style(e.certainty)
        );
    }
    out.push_str("}\n");
    out
}

/// Short, safe Mermaid node names (`n0`, `n1`, …) in node order.
///
/// Example: two nodes give `{a: "n0", b: "n1"}`.
fn mermaid_names(p: &GraphPayload) -> BTreeMap<&NodeId, String> {
    p.nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (&n.id, format!("n{i}")))
        .collect()
}

/// Escapes text for a Mermaid quoted label.
///
/// Example: `mermaid_text("a\"b")` is `"a#quot;b"`.
fn mermaid_text(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '"' => "#quot;".to_owned(),
            '<' => "#lt;".to_owned(),
            '>' => "#gt;".to_owned(),
            '|' => "#124;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// Mermaid flowchart.
///
/// Example: `graph LR`, `n0["a"]`, `n0 -->|"calls"| n1`.
pub fn mermaid(p: &GraphPayload) -> String {
    let names = mermaid_names(p);
    let mut out = String::from("graph LR\n");
    for n in &p.nodes {
        let _ = writeln!(out, "  {}[\"{}\"]", names[&n.id], mermaid_text(&n.name));
    }
    for e in &p.edges {
        let (Some(a), Some(b)) = (names.get(&e.from), names.get(&e.to)) else {
            continue;
        };
        let arrow = match e.certainty {
            Some(Certainty::Possible) | Some(Certainty::Likely) => "-.->",
            _ => "-->",
        };
        let _ = writeln!(
            out,
            "  {a} {arrow}|\"{}\"| {b}",
            mermaid_text(&edge_label(e))
        );
    }
    out
}

/// Escapes XML text and attribute values; drops characters XML 1.0 forbids.
///
/// Example: `xml("a<b&\"")` is `"a&lt;b&amp;&quot;"`.
fn xml(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// GraphML with `name` and `kind` on nodes and `kind`, `certainty`,
/// `reason` and `count` on edges.
///
/// Example: `<node id="clj:.//a"><data key="name">a</data>…</node>`.
pub fn graphml(p: &GraphPayload) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n\
         \x20 <key id=\"name\" for=\"node\" attr.name=\"name\" attr.type=\"string\"/>\n\
         \x20 <key id=\"nkind\" for=\"node\" attr.name=\"kind\" attr.type=\"string\"/>\n\
         \x20 <key id=\"ekind\" for=\"edge\" attr.name=\"kind\" attr.type=\"string\"/>\n\
         \x20 <key id=\"certainty\" for=\"edge\" attr.name=\"certainty\" attr.type=\"string\"/>\n\
         \x20 <key id=\"reason\" for=\"edge\" attr.name=\"reason\" attr.type=\"string\"/>\n\
         \x20 <key id=\"count\" for=\"edge\" attr.name=\"count\" attr.type=\"long\"/>\n\
         \x20 <graph id=\"aldente\" edgedefault=\"directed\">\n",
    );
    for n in &p.nodes {
        let _ = writeln!(
            out,
            "    <node id=\"{}\"><data key=\"name\">{}</data><data key=\"nkind\">{}</data></node>",
            xml(&n.id.to_string()),
            xml(&n.name),
            xml(&kind(n))
        );
    }
    for e in &p.edges {
        let mut data = format!("<data key=\"ekind\">{}</data>", token(&e.kind));
        if let Some(c) = &e.certainty {
            let _ = write!(data, "<data key=\"certainty\">{}</data>", token(c));
        }
        if let Some(r) = &e.reason {
            let _ = write!(data, "<data key=\"reason\">{}</data>", token(r));
        }
        if let Some(n) = e.count {
            let _ = write!(data, "<data key=\"count\">{n}</data>");
        }
        let _ = writeln!(
            out,
            "    <edge source=\"{}\" target=\"{}\">{data}</edge>",
            xml(&e.from.to_string()),
            xml(&e.to.to_string())
        );
    }
    out.push_str("  </graph>\n</graphml>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_strings_are_escaped_per_format() {
        assert_eq!(dot_quote("a\"b\\c\nd\u{1b}"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(mermaid_text("x\"]-->evil<"), "x#quot;]--#gt;evil#lt;");
        assert_eq!(xml("<script>&\u{1b}"), "&lt;script&gt;&amp;");
    }
}
