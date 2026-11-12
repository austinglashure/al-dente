//! A small reader for EDN and for Clojure source read as data
//! (`deps.edn`, `bb.edn`, `shadow-cljs.edn`, `project.clj`).
//!
//! It only builds a tree. Nothing is evaluated or resolved: `#=(...)` becomes
//! [`Edn::ReadEval`], tags stay [`Edn::Tagged`], quote forms stay lists.
//! Metadata (`^...`) is read and dropped.

use thiserror::Error;

/// A form read from source.
#[derive(Debug, Clone, PartialEq)]
pub enum Edn {
    Nil,
    Bool(bool),
    /// The number as written, e.g. `42`, `1.5M`, `1/2`.
    Number(String),
    Str(String),
    Char(char),
    /// Without the leading colon: `:a/b` is `Keyword("a/b")`, `::x` is `Keyword(":x")`.
    Keyword(String),
    Symbol(String),
    List(Vec<Edn>),
    Vector(Vec<Edn>),
    /// Entries in source order; duplicates are kept.
    Map(Vec<(Edn, Edn)>),
    Set(Vec<Edn>),
    /// `#tag form`, e.g. `#inst "2020"` or `#shadow/env "X"`.
    Tagged(String, Box<Edn>),
    /// `#"..."`, the pattern text as written.
    Regex(String),
    /// `#(...)`.
    AnonFn(Vec<Edn>),
    /// `#?(...)` or, when `splicing`, `#?@(...)`.
    ReaderCond {
        splicing: bool,
        forms: Vec<Edn>,
    },
    /// `#=form`: read-eval in Clojure. Kept so it can be reported, never run.
    ReadEval(Box<Edn>),
}

/// Why the source could not be read. Lines and columns are 1-based.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{line}:{col}: {message}")]
pub struct EdnError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

/// Deeper nesting than this is refused, so hostile input cannot overflow the stack.
pub const MAX_DEPTH: usize = 256;

/// Reads every top-level form in `src`.
///
/// Example: `read_all("{:paths [\"src\"]} 1")` gives
/// `[Map([(Keyword("paths"), Vector([Str("src")]))]), Number("1")]`.
pub fn read_all(src: &str) -> Result<Vec<Edn>, EdnError> {
    let mut reader = Reader::new(src.strip_prefix('\u{feff}').unwrap_or(src));
    let mut forms = Vec::new();
    loop {
        reader.skip_ignorable()?;
        if reader.peek().is_none() {
            return Ok(forms);
        }
        forms.push(reader.read_form()?);
    }
}

impl Edn {
    /// The value under keyword `key` if `self` is a map.
    ///
    /// Example: `{:paths ["src"]}.get_kw("paths")` gives `Some(Vector([Str("src")]))`.
    pub fn get_kw(&self, key: &str) -> Option<&Edn> {
        let Edn::Map(entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(k, _)| matches!(k, Edn::Keyword(name) if name == key))
            .map(|(_, v)| v)
    }

    /// The string if `self` is a string.
    ///
    /// Example: `Str("src").as_str()` gives `Some("src")`; `Nil.as_str()` gives `None`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Edn::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The items if `self` is a vector, list or set.
    ///
    /// Example: `Vector([Str("a")]).items()` gives `Some(&[Str("a")])`.
    pub fn items(&self) -> Option<&[Edn]> {
        match self {
            Edn::Vector(v) | Edn::List(v) | Edn::Set(v) => Some(v),
            _ => None,
        }
    }
}

/// Characters that end a symbol, keyword or number token.
///
/// Example: `is_terminator('(')` is `true`; `is_terminator('#')` is `false`.
fn is_terminator(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            ',' | '"' | ';' | '@' | '^' | '`' | '~' | '(' | ')' | '[' | ']' | '{' | '}' | '\\'
        )
}

/// A cursor over the source with line and column tracking.
struct Reader {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    depth: usize,
}

impl Reader {
    /// A reader at the start of `src`.
    ///
    /// Example: `Reader::new("x").peek()` is `Some('x')`.
    fn new(src: &str) -> Reader {
        Reader {
            chars: src.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            depth: 0,
        }
    }

    /// The next character without consuming it.
    ///
    /// Example: on `"ab"`, `peek()` is `Some('a')`.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// The character after the next one.
    ///
    /// Example: on `"ab"`, `peek2()` is `Some('b')`.
    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    /// Consumes and returns the next character.
    ///
    /// Example: on `"a\nb"`, two calls move to line 2, column 1.
    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    /// An error at the current position.
    ///
    /// Example: at line 2 column 3, `err("x")` gives `EdnError { line: 2, col: 3, .. }`.
    fn err(&self, message: impl Into<String>) -> EdnError {
        EdnError {
            line: self.line,
            col: self.col,
            message: message.into(),
        }
    }

    /// Consumes the next character, failing at end of input.
    ///
    /// Example: on `""`, gives `Err("unexpected end of input")`.
    fn next_or_eof(&mut self) -> Result<char, EdnError> {
        self.next()
            .ok_or_else(|| self.err("unexpected end of input"))
    }

    /// Skips whitespace, commas, `;` and `#!` comments, and `#_` discarded forms.
    ///
    /// Example: on `" ; c\n #_ 1 x"`, stops before `x`.
    fn skip_ignorable(&mut self) -> Result<(), EdnError> {
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ',' {
                self.next();
            } else if c == ';' || (c == '#' && self.peek2() == Some('!')) {
                while let Some(c) = self.next() {
                    if c == '\n' {
                        break;
                    }
                }
            } else if c == '#' && self.peek2() == Some('_') {
                self.next();
                self.next();
                self.skip_ignorable()?;
                self.read_form()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Reads one form, tracking depth so hostile nesting fails cleanly.
    ///
    /// Example: on `"[1]"`, gives `Vector([Number("1")])`.
    fn read_form(&mut self) -> Result<Edn, EdnError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err(format!("nested too deep (more than {MAX_DEPTH} levels)")));
        }
        let form = self.read_form_inner();
        self.depth -= 1;
        form
    }

    /// Reads one form at the current position (after ignorable input).
    ///
    /// Example: on `":a"`, gives `Keyword("a")`.
    fn read_form_inner(&mut self) -> Result<Edn, EdnError> {
        self.skip_ignorable()?;
        let c = self
            .peek()
            .ok_or_else(|| self.err("unexpected end of input"))?;
        match c {
            '(' => Ok(Edn::List(self.read_seq(')')?)),
            '[' => Ok(Edn::Vector(self.read_seq(']')?)),
            '{' => self.read_map(),
            ')' | ']' | '}' => Err(self.err(format!("unmatched `{c}`"))),
            '"' => {
                self.next();
                Ok(Edn::Str(self.read_string_body()?))
            }
            '\\' => self.read_char(),
            ':' => {
                self.next();
                let name = self.read_token();
                if name.is_empty() {
                    return Err(self.err("empty keyword"));
                }
                Ok(Edn::Keyword(name))
            }
            '\'' => self.wrap("quote"),
            '`' => self.wrap("syntax-quote"),
            '@' => self.wrap("deref"),
            '~' if self.peek2() == Some('@') => {
                self.next();
                self.wrap("unquote-splicing")
            }
            '~' => self.wrap("unquote"),
            '^' => {
                self.next();
                self.skip_metadata_then_read()
            }
            '#' => {
                self.next();
                self.read_dispatch()
            }
            _ => self.read_atom(),
        }
    }

    /// Consumes a one-character reader macro and wraps the next form as `(name form)`.
    ///
    /// Example: on `"'x"`, `wrap("quote")` gives `List([Symbol("quote"), Symbol("x")])`.
    fn wrap(&mut self, name: &str) -> Result<Edn, EdnError> {
        self.next();
        let form = self.read_form()?;
        Ok(Edn::List(vec![Edn::Symbol(name.to_owned()), form]))
    }

    /// Reads and drops a metadata form, then reads the form it annotates.
    ///
    /// Example: after `^`, on `":private x"` gives `Symbol("x")`.
    fn skip_metadata_then_read(&mut self) -> Result<Edn, EdnError> {
        self.read_form()?;
        self.read_form()
    }

    /// Reads forms after an opening delimiter up to `close`.
    ///
    /// Example: on `"(a b)"`, `read_seq(')')` gives `[Symbol("a"), Symbol("b")]`.
    fn read_seq(&mut self, close: char) -> Result<Vec<Edn>, EdnError> {
        self.next();
        let mut items = Vec::new();
        loop {
            self.skip_ignorable()?;
            match self.peek() {
                None => {
                    return Err(self.err(format!("unexpected end of input, expected `{close}`")));
                }
                Some(c) if c == close => {
                    self.next();
                    return Ok(items);
                }
                Some(c @ (')' | ']' | '}')) => {
                    return Err(self.err(format!("expected `{close}`, found `{c}`")));
                }
                Some(_) => items.push(self.read_form()?),
            }
        }
    }

    /// Reads a `{...}` map.
    ///
    /// Example: on `"{:a 1}"`, gives `Map([(Keyword("a"), Number("1"))])`.
    fn read_map(&mut self) -> Result<Edn, EdnError> {
        let (line, col) = (self.line, self.col);
        let items = self.read_seq('}')?;
        if items.len() % 2 != 0 {
            return Err(EdnError {
                line,
                col,
                message: "map literal needs an even number of forms".to_owned(),
            });
        }
        let mut entries = Vec::with_capacity(items.len() / 2);
        let mut iter = items.into_iter();
        while let (Some(k), Some(v)) = (iter.next(), iter.next()) {
            entries.push((k, v));
        }
        Ok(Edn::Map(entries))
    }

    /// Reads a string body after the opening quote, decoding escapes.
    ///
    /// Example: on `a\nb"` gives `"a\nb"` with a real newline.
    fn read_string_body(&mut self) -> Result<String, EdnError> {
        let mut out = String::new();
        loop {
            match self.next_or_eof()? {
                '"' => return Ok(out),
                '\\' => out.push(self.read_escape()?),
                c => out.push(c),
            }
        }
    }

    /// Decodes one escape after a backslash inside a string.
    ///
    /// Example: on `u00e9` gives `'é'`; on `q` gives an error.
    fn read_escape(&mut self) -> Result<char, EdnError> {
        let c = self.next_or_eof()?;
        Ok(match c {
            't' => '\t',
            'r' => '\r',
            'n' => '\n',
            'b' => '\u{8}',
            'f' => '\u{c}',
            '\\' | '"' | '\'' => c,
            'u' => {
                let hex: String = (0..4)
                    .map(|_| self.next_or_eof())
                    .collect::<Result<_, _>>()?;
                self.code_point(&hex, 16)?
            }
            '0'..='7' => {
                let mut digits = c.to_string();
                while digits.len() < 3 && matches!(self.peek(), Some('0'..='7')) {
                    digits.push(self.next_or_eof()?);
                }
                self.code_point(&digits, 8)?
            }
            other => return Err(self.err(format!("unsupported escape `\\{other}`"))),
        })
    }

    /// Parses `digits` in `radix` as a Unicode scalar value.
    ///
    /// Example: `code_point("00e9", 16)` gives `'é'`.
    fn code_point(&self, digits: &str, radix: u32) -> Result<char, EdnError> {
        u32::from_str_radix(digits, radix)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| self.err(format!("invalid character code `{digits}`")))
    }

    /// Reads a character literal starting at the backslash.
    ///
    /// Example: on `\newline` gives `Char('\n')`; on `\(` gives `Char('(')`.
    fn read_char(&mut self) -> Result<Edn, EdnError> {
        self.next();
        let first = self.next_or_eof()?;
        let mut name = first.to_string();
        if first.is_alphanumeric() {
            name.push_str(&self.read_token());
        }
        if name.chars().count() == 1 {
            return Ok(Edn::Char(first));
        }
        let c = match name.as_str() {
            "newline" => '\n',
            "space" => ' ',
            "tab" => '\t',
            "return" => '\r',
            "backspace" => '\u{8}',
            "formfeed" => '\u{c}',
            _ if name.starts_with('u') && name.len() == 5 => self.code_point(&name[1..], 16)?,
            _ if name.starts_with('o') && (2..=4).contains(&name.len()) => {
                self.code_point(&name[1..], 8)?
            }
            _ => return Err(self.err(format!("unknown character literal `\\{name}`"))),
        };
        Ok(Edn::Char(c))
    }

    /// Reads characters up to the next terminator.
    ///
    /// Example: on `"abc)"` gives `"abc"` and stops before `)`.
    fn read_token(&mut self) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if is_terminator(c) {
                break;
            }
            out.push(c);
            self.next();
        }
        out
    }

    /// Reads a number, `nil`, `true`, `false` or a symbol.
    ///
    /// Example: on `"-1.5M"` gives `Number("-1.5M")`; on `"foo/bar"` gives `Symbol("foo/bar")`.
    fn read_atom(&mut self) -> Result<Edn, EdnError> {
        let starts_number = match (self.peek(), self.peek2()) {
            (Some(c), _) if c.is_ascii_digit() => true,
            (Some('+' | '-'), Some(d)) => d.is_ascii_digit(),
            _ => false,
        };
        let token = self.read_token();
        if token.is_empty() {
            let c = self.peek().unwrap_or(' ');
            return Err(self.err(format!("unexpected `{c}`")));
        }
        Ok(match token.as_str() {
            _ if starts_number => Edn::Number(token),
            "nil" => Edn::Nil,
            "true" => Edn::Bool(true),
            "false" => Edn::Bool(false),
            _ => Edn::Symbol(token),
        })
    }

    /// Reads the form after `#`.
    ///
    /// Example: after `#`, on `"{1}"` gives `Set([Number("1")])`; on `"=(f)"`
    /// gives `ReadEval(List([Symbol("f")]))`.
    fn read_dispatch(&mut self) -> Result<Edn, EdnError> {
        let c = self
            .peek()
            .ok_or_else(|| self.err("unexpected end of input after `#`"))?;
        match c {
            '{' => Ok(Edn::Set(self.read_seq('}')?)),
            '(' => Ok(Edn::AnonFn(self.read_seq(')')?)),
            '"' => {
                self.next();
                self.read_regex()
            }
            '\'' => self.wrap("var"),
            '=' => {
                self.next();
                Ok(Edn::ReadEval(Box::new(self.read_form()?)))
            }
            '^' => {
                self.next();
                self.skip_metadata_then_read()
            }
            '_' => {
                self.next();
                self.read_form()?;
                self.read_form()
            }
            '?' => {
                self.next();
                let splicing = self.peek() == Some('@');
                if splicing {
                    self.next();
                }
                if self.peek() != Some('(') {
                    return Err(self.err("expected `(` after `#?`"));
                }
                Ok(Edn::ReaderCond {
                    splicing,
                    forms: self.read_seq(')')?,
                })
            }
            '#' => {
                self.next();
                Ok(Edn::Symbol(format!("##{}", self.read_token())))
            }
            ':' => {
                // Namespaced map `#:ns{...}`: the prefix is dropped, keys kept as written.
                self.next();
                self.read_token();
                if self.peek() != Some('{') {
                    return Err(self.err("expected `{` after namespaced map prefix"));
                }
                self.read_map()
            }
            c if c.is_alphabetic() => {
                let tag = self.read_token();
                Ok(Edn::Tagged(tag, Box::new(self.read_form()?)))
            }
            other => Err(self.err(format!("unsupported dispatch `#{other}`"))),
        }
    }

    /// Reads a regex body after `#"`, keeping escapes as written.
    ///
    /// Example: on `a\"b"` gives `Regex("a\\\"b")`.
    fn read_regex(&mut self) -> Result<Edn, EdnError> {
        let mut out = String::new();
        loop {
            match self.next_or_eof()? {
                '"' => return Ok(Edn::Regex(out)),
                '\\' => {
                    out.push('\\');
                    out.push(self.next_or_eof()?);
                }
                c => out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Edn {
        let mut forms = read_all(src).unwrap();
        assert_eq!(forms.len(), 1, "{src:?} gave {forms:?}");
        forms.remove(0)
    }

    fn kw(s: &str) -> Edn {
        Edn::Keyword(s.into())
    }

    fn sym(s: &str) -> Edn {
        Edn::Symbol(s.into())
    }

    fn st(s: &str) -> Edn {
        Edn::Str(s.into())
    }

    #[test]
    fn reads_scalars() {
        assert_eq!(one("nil"), Edn::Nil);
        assert_eq!(one("true"), Edn::Bool(true));
        assert_eq!(one("false"), Edn::Bool(false));
        assert_eq!(one("42"), Edn::Number("42".into()));
        assert_eq!(one("-1.5M"), Edn::Number("-1.5M".into()));
        assert_eq!(one("1/2"), Edn::Number("1/2".into()));
        assert_eq!(one(":a"), kw("a"));
        assert_eq!(one(":mvn/version"), kw("mvn/version"));
        assert_eq!(one("::local"), kw(":local"));
        assert_eq!(one("org.clojure/clojure"), sym("org.clojure/clojure"));
        assert_eq!(one("-main"), sym("-main"));
        assert_eq!(one("+"), sym("+"));
        assert_eq!(one("a'"), sym("a'"));
        assert_eq!(one("##Inf"), sym("##Inf"));
    }

    #[test]
    fn reads_strings_and_chars() {
        assert_eq!(one(r#""a\"b\\c\n\u00e9""#), st("a\"b\\c\né"));
        assert_eq!(one(r"\a"), Edn::Char('a'));
        assert_eq!(one(r"\newline"), Edn::Char('\n'));
        assert_eq!(one(r"\space"), Edn::Char(' '));
        assert_eq!(one(r"\u00e9"), Edn::Char('é'));
        assert_eq!(one(r"\("), Edn::Char('('));
        assert!(read_all(r#""unterminated"#).is_err());
        assert!(read_all(r#""bad \q escape""#).is_err());
    }

    #[test]
    fn reads_collections() {
        assert_eq!(
            one(r#"{:paths ["src" "resources"], :deps {}}"#),
            Edn::Map(vec![
                (kw("paths"), Edn::Vector(vec![st("src"), st("resources")])),
                (kw("deps"), Edn::Map(vec![])),
            ])
        );
        assert_eq!(one("(a b)"), Edn::List(vec![sym("a"), sym("b")]));
        assert_eq!(one("#{1}"), Edn::Set(vec![Edn::Number("1".into())]));
        assert!(read_all("{:a}").is_err());
        assert!(read_all("[1 2").is_err());
        assert!(read_all("[1 2)").is_err());
        assert!(read_all(")").is_err());
    }

    #[test]
    fn skips_comments_discards_and_metadata() {
        let src = "; comment\n#! shebang\n[1 #_ 2 #_#_ 3 4 ^:private x ^{:doc \"d\"} y #^String z]";
        assert_eq!(
            one(src),
            Edn::Vector(vec![Edn::Number("1".into()), sym("x"), sym("y"), sym("z")])
        );
    }

    #[test]
    fn reads_clojure_reader_macros_as_data() {
        assert_eq!(one("'x"), Edn::List(vec![sym("quote"), sym("x")]));
        assert_eq!(one("`x"), Edn::List(vec![sym("syntax-quote"), sym("x")]));
        assert_eq!(one("~x"), Edn::List(vec![sym("unquote"), sym("x")]));
        assert_eq!(
            one("~@x"),
            Edn::List(vec![sym("unquote-splicing"), sym("x")])
        );
        assert_eq!(one("@x"), Edn::List(vec![sym("deref"), sym("x")]));
        assert_eq!(one("#'x"), Edn::List(vec![sym("var"), sym("x")]));
        assert_eq!(one(r#"#"a\"b""#), Edn::Regex(r#"a\"b"#.into()));
        assert_eq!(one("#(inc %)"), Edn::AnonFn(vec![sym("inc"), sym("%")]));
        assert_eq!(
            one("#?(:clj 1 :cljs 2)"),
            Edn::ReaderCond {
                splicing: false,
                forms: vec![
                    kw("clj"),
                    Edn::Number("1".into()),
                    kw("cljs"),
                    Edn::Number("2".into())
                ]
            }
        );
        assert!(matches!(
            one("#?@(:clj [1])"),
            Edn::ReaderCond { splicing: true, .. }
        ));
        assert_eq!(
            one("#inst \"2020\""),
            Edn::Tagged("inst".into(), Box::new(st("2020")))
        );
        assert_eq!(
            one("#shadow/env \"X\""),
            Edn::Tagged("shadow/env".into(), Box::new(st("X")))
        );
        assert_eq!(
            one("#:a{:b 1}"),
            Edn::Map(vec![(kw("b"), Edn::Number("1".into()))])
        );
    }

    #[test]
    fn read_eval_is_kept_not_run() {
        let form = one(r#"#=(clojure.java.shell/sh "touch" "/tmp/pwned")"#);
        assert_eq!(
            form,
            Edn::ReadEval(Box::new(Edn::List(vec![
                sym("clojure.java.shell/sh"),
                st("touch"),
                st("/tmp/pwned"),
            ])))
        );
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let deep = "[".repeat(100_000);
        let err = read_all(&deep).unwrap_err();
        assert!(err.message.contains("deep"), "{err}");
        let ok = format!("{}{}", "[".repeat(MAX_DEPTH - 1), "]".repeat(MAX_DEPTH - 1));
        assert!(read_all(&ok).is_ok());
    }

    #[test]
    fn errors_carry_positions() {
        let err = read_all("{:a 1}\n  )").unwrap_err();
        assert_eq!((err.line, err.col), (2, 3));
    }

    #[test]
    fn helpers_look_into_maps_and_sequences() {
        let m = one(r#"{:paths ["src"] "k" 1}"#);
        let paths = m.get_kw("paths").unwrap();
        assert_eq!(paths.items().unwrap(), [st("src")]);
        assert_eq!(paths.items().unwrap()[0].as_str(), Some("src"));
        assert_eq!(m.get_kw("k"), None);
        assert_eq!(Edn::Nil.get_kw("paths"), None);
        assert_eq!(Edn::Nil.items(), None);
    }

    #[test]
    fn reads_a_whole_project_clj() {
        let src = r#"(defproject billing "0.1.0-SNAPSHOT"
  :description "x"
  :source-paths ["src"]
  :profiles {:dev {:source-paths ["dev"]}}
  :main ^:skip-aot billing.core)"#;
        let Edn::List(items) = one(src) else {
            panic!("not a list")
        };
        assert_eq!(items[0], sym("defproject"));
        assert_eq!(items.last(), Some(&sym("billing.core")));
    }

    #[test]
    fn a_leading_bom_is_ignored() {
        assert_eq!(one("\u{feff}{}"), Edn::Map(vec![]));
    }
}
