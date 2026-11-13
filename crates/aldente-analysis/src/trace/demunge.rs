//! Turning JVM class names back into Clojure names (the inverse of the
//! compiler's `munge`), and splitting a class name into namespace, var and
//! anonymous parts.

/// The compiler's character map (`clojure.lang.Compiler/CHAR_MAP`), plus
/// `_DOT_`, which some tools emit.
const TOKENS: &[(&str, char)] = &[
    ("_DOUBLEQUOTE_", '"'),
    ("_SINGLEQUOTE_", '\''),
    ("_AMPERSAND_", '&'),
    ("_PERCENT_", '%'),
    ("_LBRACE_", '{'),
    ("_RBRACE_", '}'),
    ("_LBRACK_", '['),
    ("_RBRACK_", ']'),
    ("_BSLASH_", '\\'),
    ("_COLON_", ':'),
    ("_CIRCA_", '@'),
    ("_CARET_", '^'),
    ("_SHARP_", '#'),
    ("_SLASH_", '/'),
    ("_TILDE_", '~'),
    ("_QMARK_", '?'),
    ("_PLUS_", '+'),
    ("_BANG_", '!'),
    ("_STAR_", '*'),
    ("_DOT_", '.'),
    ("_BAR_", '|'),
    ("_GT_", '>'),
    ("_LT_", '<'),
    ("_EQ_", '='),
];

/// Reverses `munge`: each token becomes its character, any other `_`
/// becomes `-`. Like the compiler's own `demunge`, a name that contained
/// `_` comes back with `-`.
///
/// Example: `demunge("valid_QMARK___GT_ok_BANG_")` is `"valid?->ok!"`.
pub fn demunge(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if c == '_' {
            if let Some((tok, ch)) = TOKENS.iter().find(|(t, _)| rest.starts_with(t)) {
                out.push(*ch);
                rest = &rest[tok.len()..];
                continue;
            }
            out.push('-');
        } else {
            out.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// `munge` as the compiler does it, for tests and for matching.
///
/// Example: `munge("valid?->ok!")` is `"valid_QMARK___GT_ok_BANG_"`.
pub fn munge(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '-' => "_".to_owned(),
            '.' => ".".to_owned(),
            _ => TOKENS
                .iter()
                .find(|(_, ch)| *ch == c && *ch != '.')
                .map(|(t, _)| (*t).to_owned())
                .unwrap_or_else(|| c.to_string()),
        })
        .collect()
}

/// What a Clojure function class name says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassName {
    pub namespace: String,
    /// The top-level var whose body holds the code, when named.
    pub var: Option<String>,
    /// The code is a nested or anonymous function (a `fn`, an `eval` body,
    /// a `reify`, a protocol dispatch function).
    pub anonymous: bool,
    /// The class is a protocol dispatch function (`…$G__123…`).
    pub protocol_dispatch: bool,
}

/// Splits a Clojure function class name (`ns$var$inner__12`). `None` for a
/// class without `$`.
///
/// Example: `class_name("corpus.core$outer$inner__159")` is
/// `ClassName { namespace: "corpus.core", var: Some("outer"), anonymous: true, .. }`;
/// `class_name("shop.pricing$eval334$fn__335")` has `var: None`.
pub fn class_name(class: &str) -> Option<ClassName> {
    let (ns, rest) = class.split_once('$')?;
    let mut parts = rest.split('$');
    let first = parts.next().unwrap_or("");
    let nested: Vec<&str> = parts.collect();
    let var = (!is_generated(first)).then(|| demunge(first));
    Some(ClassName {
        namespace: demunge(ns),
        anonymous: var.is_none() || !nested.is_empty(),
        protocol_dispatch: nested.iter().any(|p| p.starts_with("G__")),
        var,
    })
}

/// Whether a class-name segment is compiler-generated rather than a var:
/// `eval123`, `fn__123`, `reify__12`, `G__12`, `iter__12`.
///
/// Example: `is_generated("eval334")` is `true`; `is_generated("half_off")` is `false`.
fn is_generated(seg: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    seg.strip_prefix("eval").is_some_and(digits)
        || seg.rsplit_once("__").is_some_and(|(head, n)| {
            digits(n) && matches!(head, "fn" | "reify" | "G" | "iter" | "p")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn known_names() {
        assert_eq!(demunge("valid_QMARK___GT_ok_BANG_"), "valid?->ok!");
        assert_eq!(demunge("_STAR_star_STAR__LT__EQ__GT__PLUS_"), "*star*<=>+");
        assert_eq!(
            demunge("_PERCENT_pct_AMPERSAND__SINGLEQUOTE_quote"),
            "%pct&'quote"
        );
        assert_eq!(demunge("reserve_BANG_"), "reserve!");
        assert_eq!(demunge("corpus.core_test"), "corpus.core-test");
        assert_eq!(demunge("_"), "-");
    }

    #[test]
    fn class_names() {
        let c = class_name("corpus.core$outer$inner__159").unwrap();
        assert_eq!(
            (c.namespace.as_str(), c.var.as_deref(), c.anonymous),
            ("corpus.core", Some("outer"), true)
        );
        let c = class_name("shop.pricing$half_off").unwrap();
        assert_eq!((c.var.as_deref(), c.anonymous), (Some("half-off"), false));
        let c = class_name("shop.pricing$eval334$fn__335").unwrap();
        assert_eq!((c.var, c.anonymous), (None, true));
        let c = class_name("shop.payment$eval197$fn__198$G__188__205").unwrap();
        assert!(c.protocol_dispatch && c.anonymous);
        let c = class_name("clojure.core$future_call$reify__8574").unwrap();
        assert_eq!(c.var.as_deref(), Some("future-call"));
        assert!(class_name("clojure.lang.Numbers").is_none());
    }

    proptest! {
        #[test]
        fn demunge_inverts_munge(name in "[a-z0-9!?*+<>=&%'#/:@^|~{}\\[\\]\\\\\".-]{1,16}") {
            prop_assert_eq!(demunge(&munge(&name)), name);
        }
    }
}
