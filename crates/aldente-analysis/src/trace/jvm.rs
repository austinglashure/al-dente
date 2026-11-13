//! The JVM text parser: Java's `printStackTrace` (with `Caused by:` chains
//! and `... N more`), `clojure.repl/pst`, `clojure.stacktrace`, REPL
//! "Execution error" lines, and all of these inside log files. Lines that
//! are none of these are ignored.

use aldente_model::trace::{Frame, FrameLang, MatchLevel, ThrownException, Trace};

use super::demunge::{class_name, demunge};

/// One recognized line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Header {
        exception: ThrownException,
        cause: bool,
        frame: Option<Frame>,
    },
    Frame(Frame),
    More(u32),
    Other,
}

/// Every trace in `text`, in input order. A new exception line that is not
/// a `Caused by:` starts a new trace; traces without any frame are dropped.
///
/// Example: a `printStackTrace` of an `IllegalStateException` caused by an
/// `ArithmeticException` gives one trace with one cause.
pub fn parse(text: &str) -> Vec<Trace> {
    let mut done: Vec<Vec<Trace>> = Vec::new();
    // The trace being read: the outermost exception first, then its causes.
    let mut chain: Vec<Trace> = Vec::new();
    let mut awaiting_message = false;
    for raw in text.lines() {
        match classify(raw) {
            Line::Header {
                exception,
                cause,
                frame,
            } => {
                if !cause && !chain.is_empty() {
                    done.push(std::mem::take(&mut chain));
                }
                awaiting_message = exception.message.is_none();
                chain.push(Trace {
                    exception,
                    frames: frame.into_iter().collect(),
                    omitted: 0,
                    causes: Vec::new(),
                });
            }
            Line::Frame(f) => {
                awaiting_message = false;
                if chain.is_empty() {
                    chain.push(Trace::default());
                }
                if let Some(t) = chain.last_mut() {
                    t.frames.push(f);
                }
            }
            Line::More(n) => {
                awaiting_message = false;
                if let Some(t) = chain.last_mut() {
                    t.omitted = n;
                }
            }
            Line::Other => {
                // The REPL prints an exception's message on the line after
                // "Execution error (…) at …".
                let text = raw.trim();
                if awaiting_message
                    && !text.is_empty()
                    && let Some(t) = chain.last_mut()
                {
                    t.exception.message = Some(text.to_owned());
                }
                awaiting_message = false;
            }
        }
    }
    if !chain.is_empty() {
        done.push(chain);
    }
    done.into_iter()
        .filter(|c| c.iter().any(|t| !t.frames.is_empty()))
        .map(nest)
        .collect()
}

/// Folds `[outer, cause, cause-of-cause]` into nested `causes` and numbers
/// each trace's frames.
///
/// Example: `[a, b]` gives `a` with `causes: [b]`.
fn nest(chain: Vec<Trace>) -> Trace {
    let mut inner: Option<Trace> = None;
    for mut t in chain.into_iter().rev() {
        for (i, f) in t.frames.iter_mut().enumerate() {
            f.index = i as u32;
        }
        t.causes = inner.into_iter().collect();
        inner = Some(t);
    }
    inner.unwrap_or_default()
}

/// Recognizes one input line.
///
/// Example: `"\tat a.b$c.invoke(b.clj:3)"` is a frame; `"Caused by: java.lang.X: m"`
/// is a cause header; `"\t... 12 more"` is `More(12)`.
fn classify(raw: &str) -> Line {
    let line = raw.trim();
    if let Some(n) = line
        .strip_prefix("... ")
        .and_then(|r| r.strip_suffix(" more"))
        .and_then(|n| n.trim().parse().ok())
    {
        return Line::More(n);
    }
    if let Some(f) = parse_frame(raw) {
        return Line::Frame(f);
    }
    if let Some(rest) = line.strip_prefix("Execution error (") {
        return execution_error(rest).unwrap_or(Line::Other);
    }
    let (body, cause) = match line
        .strip_prefix("Caused by: ")
        .or_else(|| line.strip_prefix("Suppressed: "))
    {
        Some(b) => (b, true),
        None => (line, false),
    };
    let body = strip_thread_prefix(body);
    let body = body.strip_prefix("actual: ").unwrap_or(body);
    match parse_exception(body) {
        Some(exception) => Line::Header {
            exception,
            cause,
            frame: None,
        },
        None => Line::Other,
    }
}

/// Removes `Exception in thread "main" `.
///
/// Example: `strip_thread_prefix("Exception in thread \"main\" java.lang.X")` is `"java.lang.X"`.
fn strip_thread_prefix(s: &str) -> &str {
    s.strip_prefix("Exception in thread \"")
        .and_then(|r| r.split_once("\" "))
        .map_or(s, |(_, rest)| rest)
}

/// `Execution error (ArithmeticException) at corpus.core/divide (core.clj:5).`
/// (after the prefix) as a header with one frame.
///
/// Example: gives class `ArithmeticException` and a frame for `corpus.core/divide`.
fn execution_error(rest: &str) -> Option<Line> {
    let (class, after) = rest.split_once(')')?;
    let at = after.trim().strip_prefix("at ")?.trim_end_matches('.');
    Some(Line::Header {
        exception: ThrownException {
            class: Some(class.to_owned()),
            message: None,
            data: None,
        },
        cause: false,
        frame: parse_frame(at),
    })
}

/// `java.lang.ArithmeticException: Divide by zero`, or `pst`'s
/// `ArithmeticException Divide by zero`.
///
/// Example: `parse_exception("clojure.lang.ExceptionInfo: order rejected {:id 7}")`
/// gives that class and message.
fn parse_exception(s: &str) -> Option<ThrownException> {
    let end = s.find([':', ' ']).unwrap_or(s.len());
    let class = &s[..end];
    if !is_exception_class(class) {
        return None;
    }
    let message = s[end..].trim_start_matches(':').trim().to_owned();
    Some(ThrownException {
        class: Some(class.to_owned()),
        message: (!message.is_empty()).then_some(message),
        data: None,
    })
}

/// A dotted Java identifier whose last part names an exception type.
///
/// Example: `is_exception_class("java.io.IOException")` is `true`; `"Error"` alone is `false`.
fn is_exception_class(s: &str) -> bool {
    let simple = s.rsplit('.').next().unwrap_or(s);
    let ident = |p: &str| {
        let mut cs = p.chars();
        cs.next()
            .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
            && cs.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
    };
    s.split('.').all(ident)
        && (simple.len() > "Error".len() || s.contains('.'))
        && ["Exception", "Error", "Throwable", "ExceptionInfo"]
            .iter()
            .any(|suffix| simple.ends_with(suffix))
}

/// Parses a frame in any of the JVM forms:
/// `at a.b$c.invokeStatic(b.clj:3)`, `at java.base/java.lang.Thread.run(Thread.java:833)`,
/// `a.b$c.invoke (b.clj:3)` (clojure.stacktrace), `a.b/c (b.clj:3)` (pst).
///
/// Example: `parse_frame("\tat shop.pricing$half_off.invokeStatic(pricing.clj:10)")`
/// gives namespace `shop.pricing`, name `half-off`, line 10.
pub fn parse_frame(raw: &str) -> Option<Frame> {
    let line = raw.trim();
    let line = line.strip_prefix("at ").unwrap_or(line);
    let open = line.rfind('(')?;
    let loc = line[open + 1..].strip_suffix(')')?;
    let symbol = line[..open].trim_end();
    if symbol.is_empty() || symbol.contains(char::is_whitespace) || loc.contains('(') {
        return None;
    }
    let (file, line_no) = location(loc)?;
    let mut frame = Frame {
        index: 0,
        lang: FrameLang::Java,
        namespace: None,
        name: None,
        anonymous: false,
        file: file.clone(),
        line: line_no,
        col: None,
        raw: raw.trim().to_owned(),
        noise: false,
        node: None,
        match_level: MatchLevel::None,
    };
    match symbol.rsplit_once('/') {
        // A module or class loader prefix: `java.base/java.lang.Thread.run`.
        Some((_, after)) if after.contains('.') => fill_dotted(&mut frame, after)?,
        // pst's demunged `ns/var` or `ns/var/fn--12`.
        Some(_) => fill_slashed(&mut frame, symbol)?,
        None => fill_dotted(&mut frame, symbol)?,
    }
    if frame.lang != FrameLang::Java {
        frame.lang = match file.as_deref() {
            Some(f) if f.ends_with(".cljs") => FrameLang::Cljs,
            _ => FrameLang::Clj,
        };
    }
    Some(frame)
}

/// `File.ext:12`, `File.ext`, `Native Method`, `Unknown Source`.
///
/// Example: `location("core.clj:5")` is `Some((Some("core.clj"), Some(5)))`.
fn location(loc: &str) -> Option<(Option<String>, Option<u32>)> {
    if loc == "Native Method" || loc == "Unknown Source" {
        return Some((None, None));
    }
    let (file, line) = match loc.rsplit_once(':') {
        Some((f, n)) => (f, Some(n.parse::<u32>().ok()?)),
        None => (loc, None),
    };
    if file.is_empty() || file.contains(char::is_whitespace) {
        return None;
    }
    let file = (file != "NO_SOURCE_FILE").then(|| file.to_owned());
    Some((file, line))
}

/// `class.method`: a Clojure function class (`ns$var…`) or a Java class.
///
/// Example: `corpus.core$divide.invokeStatic` gives namespace `corpus.core`, name `divide`.
fn fill_dotted(frame: &mut Frame, symbol: &str) -> Option<()> {
    let (class, method) = symbol.rsplit_once('.')?;
    if let Some(c) = class_name(class) {
        frame.lang = FrameLang::Clj;
        frame.namespace = Some(c.namespace);
        frame.name = c.var;
        frame.anonymous = c.anonymous;
    } else {
        frame.namespace = Some(class.to_owned());
        frame.name = Some(method.to_owned());
    }
    Some(())
}

/// pst's `ns/var`, `ns/var/fn--12` or `ns/eval12`.
///
/// Example: `corpus.core/wrap-once` gives namespace `corpus.core`, name `wrap-once`.
fn fill_slashed(frame: &mut Frame, symbol: &str) -> Option<()> {
    let mut parts = symbol.split('/');
    let ns = parts.next()?;
    let first = parts.next()?;
    let nested = parts.next().is_some();
    let generated = first == "fn"
        || first
            .strip_prefix("eval")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
        || first.starts_with("fn--");
    frame.lang = FrameLang::Clj;
    frame.namespace = Some(demunge(ns));
    frame.name = (!generated).then(|| first.to_owned());
    frame.anonymous = generated || nested;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_forms() {
        let f = parse_frame("\tat shop.pricing$half_off.invokeStatic(pricing.clj:10)").unwrap();
        assert_eq!(
            (
                f.namespace.as_deref(),
                f.name.as_deref(),
                f.file.as_deref(),
                f.line,
                f.lang
            ),
            (
                Some("shop.pricing"),
                Some("half-off"),
                Some("pricing.clj"),
                Some(10),
                FrameLang::Clj
            )
        );
        let f = parse_frame("\tat java.base/java.lang.Thread.run(Thread.java:833)").unwrap();
        assert_eq!(
            (f.namespace.as_deref(), f.name.as_deref(), f.lang),
            (Some("java.lang.Thread"), Some("run"), FrameLang::Java)
        );
        let f = parse_frame("    corpus.core$divide.invokeStatic (core.clj:5)").unwrap();
        assert_eq!(f.name.as_deref(), Some("divide"));
        let f = parse_frame("\tcorpus.core/wrap-once (core.clj:9)").unwrap();
        assert_eq!(
            (f.namespace.as_deref(), f.name.as_deref()),
            (Some("corpus.core"), Some("wrap-once"))
        );
        let f = parse_frame("\tuser/eval236 (NO_SOURCE_FILE:1)").unwrap();
        assert!(f.anonymous && f.file.is_none());
        let f = parse_frame("at sun.misc.Unsafe.park(Native Method)").unwrap();
        assert_eq!((f.file, f.line), (None, None));
        assert!(parse_frame("Execution error (X) at a/b (c.clj:1).").is_none());
        assert!(parse_frame("2026-11-13 INFO app (main) started").is_none());
    }

    #[test]
    fn headers() {
        assert!(matches!(
            classify("java.lang.ArithmeticException: Divide by zero"),
            Line::Header { cause: false, .. }
        ));
        assert!(matches!(
            classify("Caused by: java.lang.RuntimeException: x"),
            Line::Header { cause: true, .. }
        ));
        assert!(matches!(
            classify("  actual: java.lang.ArithmeticException: Divide by zero"),
            Line::Header { .. }
        ));
        assert!(matches!(
            classify("Exception in thread \"main\" java.lang.Error: boom"),
            Line::Header { .. }
        ));
        assert!(matches!(
            classify("ArithmeticException Divide by zero"),
            Line::Header { .. }
        ));
        assert_eq!(classify("\t... 17 more"), Line::More(17));
        assert_eq!(classify("Error connecting"), Line::Other);
        assert_eq!(classify("2026-11-13 ERROR app - failed"), Line::Other);
    }

    #[test]
    fn chains_and_splitting() {
        let text = "java.lang.IllegalStateException: outer\n\tat a.b$c.invoke(b.clj:1)\nCaused by: java.lang.RuntimeException: inner\n\tat a.b$d.invoke(b.clj:2)\n\t... 1 more\nnoise line\njava.lang.Error2Exception: second\n\tat a.b$e.invoke(b.clj:3)\n";
        let traces = parse(text);
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].causes.len(), 1);
        assert_eq!(traces[0].causes[0].omitted, 1);
        assert_eq!(
            traces[0].causes[0].exception.message.as_deref(),
            Some("inner")
        );
        assert_eq!(traces[1].frames[0].name.as_deref(), Some("e"));
    }
}
