//! The form index: snapshots of three representative clj-mini files, broken
//! input, and hostile nesting.

// Helpers outside #[test] fns are not covered by clippy.toml's test allowance.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use aldente_clj::forms::{FileIndex, Span, index_files, index_source};
use aldente_model::{DiagnosticCode, RepoPath};

fn mini() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/clj-mini")
        .canonicalize()
        .unwrap()
}

fn span(s: &Span) -> String {
    format!(
        "{}:{}-{}:{}",
        s.start.line, s.start.col, s.end.line, s.end.col
    )
}

/// A stable, readable rendering of an index for snapshots.
///
/// Example: `form 1:1-3:2 ns name=shop.pricing`.
fn render(ix: &FileIndex) -> String {
    let mut out = String::new();
    for f in &ix.forms {
        let _ = write!(
            out,
            "form {} {}",
            span(&f.span),
            f.head.as_deref().unwrap_or("-")
        );
        if let Some(n) = &f.name {
            let _ = write!(out, " name={n}");
        }
        if let Some(d) = &f.dispatch {
            let _ = write!(out, " dispatch={d}");
        }
        out.push('\n');
    }
    for i in &ix.impls {
        let _ = writeln!(
            out,
            "impl {} {} type={} protocol={} method={}",
            span(&i.span),
            i.owner,
            i.type_name.as_deref().unwrap_or("-"),
            i.protocol.as_deref().unwrap_or("-"),
            i.method
        );
    }
    for c in &ix.calls {
        let args: Vec<String> = c.args.iter().map(span).collect();
        let _ = writeln!(
            out,
            "call {} {} args=[{}]",
            span(&c.span),
            c.head,
            args.join(" ")
        );
    }
    for l in &ix.literals {
        let _ = writeln!(
            out,
            "literal {} {:?} {}",
            span(&l.span),
            l.kind,
            l.text.escape_debug()
        );
    }
    for b in &ix.branches {
        let _ = writeln!(out, "branch {} {}", span(&b.span), b.feature);
    }
    for q in &ix.quoted {
        let _ = writeln!(out, "quoted {}", span(q));
    }
    for d in &ix.diagnostics {
        let _ = writeln!(out, "diagnostic {:?} {}", d.code, d.message);
    }
    out
}

fn index_one(rel: &str) -> FileIndex {
    let mut v = index_files(&mini(), &[RepoPath::parse(rel).unwrap()]);
    assert_eq!(v.len(), 1);
    v.remove(0)
}

#[test]
fn snapshot_pricing_multimethod() {
    insta::assert_snapshot!(render(&index_one("src/shop/pricing.clj")));
}

#[test]
fn snapshot_payment_protocols_and_records() {
    insta::assert_snapshot!(render(&index_one("src/shop/payment.clj")));
}

#[test]
fn snapshot_model_reader_conditionals() {
    insta::assert_snapshot!(render(&index_one("src/shop/model.cljc")));
}

#[test]
fn indexes_every_file_in_order() {
    let files: Vec<RepoPath> = ["src/shop/core.clj", "src/shop/util.clj"]
        .iter()
        .map(|p| RepoPath::parse(p).unwrap())
        .collect();
    let ixs = index_files(&mini(), &files);
    assert_eq!(
        ixs.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
        ["src/shop/core.clj", "src/shop/util.clj"]
    );
    assert!(ixs.iter().all(|i| i.diagnostics.is_empty()));
}

#[test]
fn a_truncated_file_keeps_earlier_forms_and_reports() {
    let src = "(ns t)\n(defn ok [] 1)\n(defn broken [x\n";
    let ix = index_source(RepoPath::parse("t.clj").unwrap(), src);
    let names: Vec<_> = ix.forms.iter().filter_map(|f| f.name.as_deref()).collect();
    assert_eq!(names, ["t", "ok"]);
    assert_eq!(ix.diagnostics.len(), 1);
    assert_eq!(ix.diagnostics[0].code, DiagnosticCode::ParseError);
}

/// Hostile case 8 (nesting part): deep nesting does not overflow the stack.
/// Runs on a small thread stack to make an accidental recursion fail loudly.
#[test]
fn hostile_8_deep_nesting_does_not_overflow() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let depth = 100_000;
            let src = format!("(ns deep)\n{}x{}", "(f ".repeat(depth), ")".repeat(depth));
            let ix = index_source(RepoPath::parse("deep.clj").unwrap(), &src);
            (ix.forms.len(), ix.calls.len())
        })
        .unwrap();
    let (forms, calls) = handle.join().expect("indexing deep input must not crash");
    assert_eq!(forms, 2);
    assert!(calls >= 100_000, "{calls}");
}

#[test]
fn anonymous_fn_bodies_are_call_sites_at_their_paren() {
    let ix = index_source(RepoPath::parse("a.clj").unwrap(), "(f #(g %))");
    let g = ix.calls.iter().find(|c| c.head == "g").expect("call to g");
    assert_eq!(
        (g.span.start.line, g.span.start.col),
        (1, 5),
        "the `(` after `#`"
    );
}

#[test]
fn call_arguments_carry_kind_text_and_map_entries() {
    let ix = index_source(
        RepoPath::parse("a.clj").unwrap(),
        "(f {:shape :circle :r 2} x \"s\" (g))",
    );
    let call = ix.calls.iter().find(|c| c.head == "f").unwrap();
    let kinds: Vec<&str> = call.arg_info.iter().map(|a| a.kind.as_str()).collect();
    assert_eq!(kinds, ["map_lit", "sym_lit", "str_lit", "list_lit"]);
    assert_eq!(call.arg_info[1].text, "x");
    assert_eq!(
        call.arg_info[0].entries,
        [
            (":shape".to_owned(), ":circle".to_owned()),
            (":r".to_owned(), "2".to_owned())
        ]
    );
    assert!(call.arg_info[1].entries.is_empty());
}

#[test]
fn syntax_quotes_are_recorded() {
    let src = "(defmacro m [x] `(let [t# 1] (f ~x)))";
    let ix = index_source(RepoPath::parse("m.clj").unwrap(), src);
    assert_eq!(ix.quoted.len(), 1);
    assert_eq!((ix.quoted[0].start.col, ix.quoted[0].end.col), (17, 37));
}

#[test]
fn columns_count_characters_not_bytes() {
    let ix = index_source(RepoPath::parse("u.clj").unwrap(), "(def é \"ü\") (f :k)");
    let call = ix.calls.iter().find(|c| c.head == "f").unwrap();
    assert_eq!((call.span.start.line, call.span.start.col), (1, 13));
}
