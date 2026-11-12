//! The form index: Al Dente's own syntactic pass over each file with
//! tree-sitter (`04-clojure-analysis.md`, "The form index"). Purely
//! syntactic and tolerant: a broken file yields the forms before the break
//! and a diagnostic. Traversal is iterative, so deep nesting cannot overflow
//! the stack.

use std::path::Path;

use aldente_model::{Diagnostic, DiagnosticCode, RepoPath};
use rayon::prelude::*;
use tree_sitter::{Node, Parser};

/// A 1-based line and column; columns count characters, as clj-kondo does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

/// A source range: bytes and positions, end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl Span {
    /// Whether `pos` lies inside this span (start inclusive, end exclusive).
    ///
    /// Example: span 3:1–5:10 contains 4:2 and 3:1 but not 5:10.
    pub fn contains(&self, pos: Pos) -> bool {
        self.start <= pos && pos < self.end
    }
}

/// A top-level form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopForm {
    pub span: Span,
    /// The head symbol of a list form, as written (`defn`, `s/defn`).
    pub head: Option<String>,
    /// For definition-like heads (`def…`, `ns`, `extend-type`, …): the
    /// second element when it is a symbol, without metadata.
    pub name: Option<String>,
    /// For `defmethod`: the dispatch value as written.
    pub dispatch: Option<String>,
}

/// A method body inside `extend-type`, `extend-protocol`, `defrecord`,
/// `deftype` or `reify`, anywhere in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplBody {
    pub span: Span,
    /// The form that holds it, e.g. `extend-type`.
    pub owner: String,
    /// The type (`extend-type`, `extend-protocol`, `defrecord`, `deftype`).
    pub type_name: Option<String>,
    pub protocol: Option<String>,
    pub method: String,
}

/// A list whose head is a symbol or keyword: a call site candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSite {
    pub span: Span,
    pub head: String,
    pub args: Vec<Span>,
}

/// Literal kinds harvested for later phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LiteralKind {
    Str,
    Keyword,
    Number,
    Regex,
}

/// A literal as written (capped at [`MAX_LITERAL_CHARS`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    pub span: Span,
    pub kind: LiteralKind,
    pub text: String,
}

/// One branch of a reader conditional: the feature (`:clj`) and its form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CondBranch {
    pub span: Span,
    pub feature: String,
}

/// Everything the index records for one file, each list in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIndex {
    pub path: RepoPath,
    pub forms: Vec<TopForm>,
    pub impls: Vec<ImplBody>,
    pub calls: Vec<CallSite>,
    pub literals: Vec<Literal>,
    pub branches: Vec<CondBranch>,
    /// Syntax-quoted forms (`` `(…) ``), outermost and nested.
    pub quoted: Vec<Span>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Literal text longer than this is cut, so a huge string cannot bloat the index.
pub const MAX_LITERAL_CHARS: usize = 1000;

/// Indexes one file's source.
///
/// Example: `index_source(path, "(ns a)\n(defn f [] 1)")` gives two forms,
/// heads `ns` and `defn`, names `a` and `f`.
pub fn index_source(path: RepoPath, src: &str) -> FileIndex {
    match new_parser() {
        Some(mut parser) => index_with(&mut parser, path, src),
        None => empty(
            path.clone(),
            vec![diag(&path, "cannot load the Clojure grammar".into())],
        ),
    }
}

/// Indexes `files` under `root` in parallel, reading each with the walker's
/// inside-the-root check. Results are in the order of `files`.
///
/// Example: on clj-mini's 13 sources gives 13 indexes.
pub fn index_files(root: &Path, files: &[RepoPath]) -> Vec<FileIndex> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let max = aldente_acquire::walk::Limits::default().max_file_bytes;
    files
        .par_iter()
        .map_init(new_parser, |parser, file| {
            let bytes = match aldente_acquire::walk::read_inside(&root, file, max) {
                Ok(bytes) => bytes,
                Err(e) => {
                    let d = Diagnostic::new(
                        Some(file.clone()),
                        DiagnosticCode::Unreadable,
                        format!("cannot read: {e:?}"),
                    );
                    return empty(file.clone(), vec![d]);
                }
            };
            let src = String::from_utf8_lossy(&bytes);
            let mut ix = match parser {
                Some(p) => index_with(p, file.clone(), &src),
                None => empty(
                    file.clone(),
                    vec![diag(file, "cannot load the Clojure grammar".into())],
                ),
            };
            if std::str::from_utf8(&bytes).is_err() {
                ix.diagnostics.push(Diagnostic::new(
                    Some(file.clone()),
                    DiagnosticCode::Unreadable,
                    "not valid UTF-8; invalid bytes were replaced",
                ));
            }
            ix
        })
        .collect()
}

/// A parser for the Clojure grammar, or `None` if the grammar fails to load.
///
/// Example: `new_parser().is_some()` is `true`.
fn new_parser() -> Option<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_clojure_orchard::LANGUAGE.into())
        .ok()?;
    Some(parser)
}

/// An index with nothing but diagnostics.
///
/// Example: `empty(path, vec![])` has no forms.
fn empty(path: RepoPath, diagnostics: Vec<Diagnostic>) -> FileIndex {
    FileIndex {
        path,
        forms: Vec::new(),
        impls: Vec::new(),
        calls: Vec::new(),
        literals: Vec::new(),
        branches: Vec::new(),
        quoted: Vec::new(),
        diagnostics,
    }
}

/// Indexes `src` with an existing parser.
///
/// Example: see [`index_source`].
fn index_with(parser: &mut Parser, path: RepoPath, src: &str) -> FileIndex {
    let Some(tree) = parser.parse(src, None) else {
        return empty(path.clone(), vec![diag(&path, "the parser gave up".into())]);
    };
    let ctx = Ctx {
        src,
        lines: line_starts(src),
    };
    let root = tree.root_node();
    let mut ix = empty(path, Vec::new());
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if matches!(child.kind(), "comment" | "dis_expr" | "ERROR") || child.is_extra() {
            continue;
        }
        ix.forms.push(ctx.top_form(child));
    }
    ctx.walk(root, &mut ix);
    if root.has_error() {
        let at = first_error(root).map(|n| ctx.pos(n.start_byte()));
        let message = match at {
            Some(p) => format!(
                "syntax error at {}:{}; forms after it may be missing",
                p.line, p.col
            ),
            None => "syntax error; some forms may be missing".to_owned(),
        };
        ix.diagnostics.push(diag(&ix.path, message));
    }
    ix.impls.sort_by_key(|i| i.span);
    ix.calls.sort_by_key(|c| c.span);
    ix.literals.sort_by_key(|l| l.span);
    ix.branches.sort_by_key(|b| b.span);
    ix.quoted.sort();
    ix
}

/// The first error or missing node in document order, found iteratively.
///
/// Example: in `(defn f [x` gives the node where the missing `]` is.
fn first_error(root: Node<'_>) -> Option<Node<'_>> {
    let mut stack = vec![root];
    let mut best: Option<Node<'_>> = None;
    while let Some(node) = stack.pop() {
        if !node.has_error() && !node.is_missing() {
            continue;
        }
        if node.is_error() || node.is_missing() {
            if best.is_none_or(|b| node.start_byte() < b.start_byte()) {
                best = Some(node);
            }
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    best
}

/// Byte offsets where each line starts.
///
/// Example: `"a\nbc"` gives `[0, 2]`.
fn line_starts(src: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// Heads that hold protocol method bodies.
const IMPL_OWNERS: &[&str] = &[
    "extend-type",
    "extend-protocol",
    "defrecord",
    "deftype",
    "reify",
];

/// Source text and line table for one file.
struct Ctx<'s> {
    src: &'s str,
    lines: Vec<usize>,
}

impl Ctx<'_> {
    /// The position of a byte offset.
    ///
    /// Example: in `"a\néb"`, byte 4 (`b`) is line 2, column 2.
    fn pos(&self, byte: usize) -> Pos {
        let line = self
            .lines
            .partition_point(|&start| start <= byte)
            .saturating_sub(1);
        let start = self.lines.get(line).copied().unwrap_or(0);
        let col = self.src.get(start..byte).map_or(0, |s| s.chars().count());
        Pos {
            line: line as u32 + 1,
            col: col as u32 + 1,
        }
    }

    /// The span of a node.
    ///
    /// Example: the list `(f)` at the start of a file is 1:1–1:4.
    fn span(&self, node: Node<'_>) -> Span {
        Span {
            start: self.pos(node.start_byte()),
            end: self.pos(node.end_byte()),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        }
    }

    /// The source text of a node.
    ///
    /// Example: a `num_lit` node for `42` gives `"42"`.
    fn text(&self, node: Node<'_>) -> &str {
        self.src.get(node.byte_range()).unwrap_or("")
    }

    /// A symbol's text without metadata, e.g. `str/join`; `None` for non-symbols.
    ///
    /// Example: the node for `^:private f` gives `Some("f")`.
    fn sym(&self, node: Node<'_>) -> Option<String> {
        if node.kind() != "sym_lit" {
            return None;
        }
        let name = self.text(node.child_by_field_name("name")?);
        Some(match node.child_by_field_name("namespace") {
            Some(ns) => format!("{}/{name}", self.text(ns)),
            None => name.to_owned(),
        })
    }

    /// Summarizes a top-level form.
    ///
    /// Example: `(defmethod price :food [x] …)` gives head `defmethod`, name
    /// `price`, dispatch `:food`.
    fn top_form(&self, node: Node<'_>) -> TopForm {
        let mut form = TopForm {
            span: self.span(node),
            head: None,
            name: None,
            dispatch: None,
        };
        if node.kind() != "list_lit" {
            return form;
        }
        let vals = values(node);
        form.head = vals.first().and_then(|n| self.sym(*n));
        let local = form.head.as_deref().map(local_name).unwrap_or_default();
        let definition_like =
            local.starts_with("def") || matches!(local, "ns" | "extend-type" | "extend-protocol");
        if definition_like {
            form.name = vals.get(1).and_then(|n| self.sym(*n));
        }
        if local == "defmethod" {
            form.dispatch = vals.get(2).map(|n| self.text(*n).to_owned());
        }
        form
    }

    /// Walks the whole tree iteratively, collecting calls, impl bodies,
    /// literals and reader-conditional branches. Discarded forms (`#_`),
    /// comments and metadata are skipped.
    ///
    /// Example: `(f 1)` adds a call with head `f` and a number literal `1`.
    fn walk(&self, root: Node<'_>, ix: &mut FileIndex) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "dis_expr" | "comment" | "meta_lit" | "old_meta_lit" => continue,
                "list_lit" => self.list(node, ix, 0),
                // `#(…)`: the call starts at the `(`, one byte after `#`.
                "anon_fn_lit" => self.list(node, ix, 1),
                "str_lit" => self.literal(node, LiteralKind::Str, ix),
                "kwd_lit" => self.literal(node, LiteralKind::Keyword, ix),
                "num_lit" => self.literal(node, LiteralKind::Number, ix),
                "regex_lit" => self.literal(node, LiteralKind::Regex, ix),
                "syn_quoting_lit" => ix.quoted.push(self.span(node)),
                "read_cond_lit" | "splicing_read_cond_lit" => {
                    for pair in values(node).as_chunks::<2>().0 {
                        ix.branches.push(CondBranch {
                            span: self.span(pair[1]),
                            feature: self.text(pair[0]).to_owned(),
                        });
                    }
                }
                _ => {}
            }
            let mut cursor = node.walk();
            stack.extend(node.named_children(&mut cursor));
        }
    }

    /// Records a list as a call site (symbol or keyword head) and, for
    /// impl-holding heads, its method bodies. `skip` bytes are dropped from
    /// the start of the span (1 for `#(…)`, so the site is at the `(`).
    ///
    /// Example: `(extend-type Card Chargeable (charge [c a] …))` adds a call
    /// with head `extend-type` and an impl body for `charge`.
    fn list(&self, node: Node<'_>, ix: &mut FileIndex, skip: usize) {
        let vals = values(node);
        let Some(first) = vals.first() else {
            return;
        };
        let head = match first.kind() {
            "sym_lit" => self.sym(*first),
            "kwd_lit" => Some(self.text(*first).to_owned()),
            _ => None,
        };
        let Some(head) = head else {
            return;
        };
        let mut span = self.span(node);
        span.start_byte += skip;
        span.start = self.pos(span.start_byte);
        ix.calls.push(CallSite {
            span,
            head: head.clone(),
            args: vals[1..].iter().map(|n| self.span(*n)).collect(),
        });
        let owner = local_name(&head);
        if IMPL_OWNERS.contains(&owner) {
            self.impl_bodies(owner, &vals, ix);
        }
    }

    /// Method bodies of an impl-holding form. Symbols switch the current
    /// protocol (or type, for `extend-protocol`); lists are method bodies.
    ///
    /// Example: `(defrecord Card [n] Chargeable (charge [c a] a))` gives one
    /// body: type `Card`, protocol `Chargeable`, method `charge`.
    fn impl_bodies(&self, owner: &str, vals: &[Node<'_>], ix: &mut FileIndex) {
        let (fixed, rest) = match owner {
            "extend-type" | "extend-protocol" => {
                (vals.get(1).and_then(|n| self.sym(*n)), vals.get(2..))
            }
            "defrecord" | "deftype" => (vals.get(1).and_then(|n| self.sym(*n)), vals.get(3..)),
            _ => (None, vals.get(1..)),
        };
        let mut current: Option<String> = None;
        for node in rest.unwrap_or_default() {
            match node.kind() {
                "sym_lit" => current = self.sym(*node),
                "list_lit" => {
                    let Some(method) = values(*node).first().and_then(|n| self.sym(*n)) else {
                        continue;
                    };
                    let (type_name, protocol) = if owner == "extend-protocol" {
                        (current.clone(), fixed.clone())
                    } else {
                        (fixed.clone(), current.clone())
                    };
                    ix.impls.push(ImplBody {
                        span: self.span(*node),
                        owner: owner.to_owned(),
                        type_name,
                        protocol,
                        method,
                    });
                }
                _ => {}
            }
        }
    }

    /// Records a literal.
    ///
    /// Example: `:food` gives `Literal { kind: Keyword, text: ":food" }`.
    fn literal(&self, node: Node<'_>, kind: LiteralKind, ix: &mut FileIndex) {
        ix.literals.push(Literal {
            span: self.span(node),
            kind,
            text: self.text(node).chars().take(MAX_LITERAL_CHARS).collect(),
        });
    }
}

/// The `value` children of a collection node (its elements), skipping
/// metadata and comments.
///
/// Example: the list `(f ^:m x)` gives the nodes for `f` and `x`.
fn values(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children_by_field_name("value", &mut cursor).collect()
}

/// The part of a symbol after its namespace.
///
/// Example: `clojure.core/defn` gives `defn`; `defn` gives `defn`.
fn local_name(sym: &str) -> &str {
    sym.rsplit_once('/')
        .map_or(sym, |(_, n)| if n.is_empty() { sym } else { n })
}

/// Reports a problem with a file being indexed.
///
/// Example: `diag(path, "x")` is a `parse-error` diagnostic.
fn diag(path: &RepoPath, message: String) -> Diagnostic {
    Diagnostic::new(Some(path.clone()), DiagnosticCode::ParseError, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diag_is_a_parse_error() {
        let d = diag(&RepoPath::parse("a.clj").unwrap(), "x".into());
        assert_eq!(d.code, DiagnosticCode::ParseError);
    }
}
