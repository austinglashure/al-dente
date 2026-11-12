//! `aldente show <symbol> [TARGET]`: one node with its callers and callees.

use aldente_graph::query::neighbors;
use aldente_model::query::{Neighbor, ShowResult};
use aldente_model::{Envelope, ExitCode};
use clap::Args;

use crate::commands::loaded::load;
use crate::commands::query::node_id;
use crate::commands::target::TargetArgs;
use crate::output::{fail, print_json, print_lines, printable, token};

/// Arguments for `aldente show`.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Full ID or an unambiguous suffix (`billing.core/charge!`, `charge!`).
    symbol: String,
    #[command(flatten)]
    target: TargetArgs,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "show";

/// Prints the node's details and direct callers and callees. An ambiguous
/// name exits 3 and lists the candidates.
///
/// Example: `aldente show checkout` prints its location, signature and callees.
pub fn run(args: &ShowArgs, json: bool) -> anyhow::Result<ExitCode> {
    let loaded = match load(&args.target, false) {
        Ok(l) => l,
        Err(f) => return fail(TAG, &f, json),
    };
    let graph = &loaded.outcome.snapshot.graph;
    let id = match node_id(graph, &args.symbol) {
        Ok(id) => id,
        Err(f) => return fail(TAG, &f, json),
    };
    let Some(node) = graph.nodes.get(&id).cloned() else {
        return fail(
            TAG,
            &crate::commands::target::Failure::new(
                aldente_model::ErrorCode::UnknownSymbol,
                "node vanished",
            ),
            json,
        );
    };
    let (callers, callees) = neighbors(graph, &id);
    let children = aldente_graph::query::children(graph, &id);
    let result = ShowResult {
        node,
        callers,
        callees,
        children,
    };
    if json {
        print_json(&Envelope::success(TAG, Some(loaded.info()), &result))?;
    } else {
        print_lines(render(&result).iter().map(String::as_str))?;
    }
    Ok(ExitCode::Success)
}

/// The text form.
///
/// Example: `["clj:.//a/f  symbol/fn", "  at src/a.clj:1:1", "callers (0)", "callees (1)", "  certain  calls direct  ext:clj//clojure.core/map  src/a.clj:2:3"]`.
fn render(r: &ShowResult) -> Vec<String> {
    let n = &r.node;
    let kind = match &n.subkind {
        Some(sub) => format!("{}/{}", token(&n.kind), token(sub)),
        None => token(&n.kind),
    };
    let mut lines = vec![format!("{}  {kind}", n.id)];
    if !n.flags.is_empty() {
        lines.push(format!(
            "  flags {}",
            n.flags.iter().map(token).collect::<Vec<_>>().join(" ")
        ));
    }
    if !n.langs.is_empty() {
        lines.push(format!(
            "  langs {}",
            n.langs.iter().map(token).collect::<Vec<_>>().join(" ")
        ));
    }
    if let Some(l) = &n.loc {
        lines.push(format!("  at {}:{}:{}", l.file, l.line, l.col));
    }
    if let Some(sig) = &n.signature {
        lines.push(format!("  signature {sig}"));
    }
    if let Some(doc) = &n.doc {
        lines.push(format!("  doc {}", doc.replace('\n', " ")));
    }
    for (title, list) in [("callers", &r.callers), ("callees", &r.callees)] {
        lines.push(format!("{title} ({})", list.len()));
        lines.extend(list.iter().map(neighbor_line));
    }
    if !r.children.is_empty() {
        lines.push(format!("contains ({})", r.children.len()));
        lines.extend(r.children.iter().map(|c| match &c.subkind {
            Some(sub) => format!("  {}  {}/{}", c.id, token(&c.kind), token(sub)),
            None => format!("  {}  {}", c.id, token(&c.kind)),
        }));
    }
    lines.iter().map(|l| printable(l)).collect()
}

/// One neighbor: certainty, edge kind and reason, ID, first site.
///
/// Example: `"  certain  calls direct  clj:.//a/g  src/a.clj:3:5 (+1)"`.
fn neighbor_line(n: &Neighbor) -> String {
    let cert = n
        .certainty
        .as_ref()
        .map(token)
        .unwrap_or_else(|| "-".into());
    let reason = n.reason.as_ref().map(token).unwrap_or_else(|| "-".into());
    let site = match n.sites.split_first() {
        Some((s, [])) => format!("  {}:{}:{}", s.file, s.line, s.col),
        Some((s, rest)) => format!("  {}:{}:{} (+{})", s.file, s.line, s.col, rest.len()),
        None => String::new(),
    };
    let count = n.count.map(|c| format!(" x{c}")).unwrap_or_default();
    format!(
        "  {cert}  {} {reason}  {}{count}{site}",
        token(&n.kind),
        n.id
    )
}
