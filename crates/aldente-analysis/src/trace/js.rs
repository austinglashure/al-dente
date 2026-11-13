//! JavaScript stack traces from ClojureScript: V8 (Chrome, Node) and the
//! `name@url:line:col` form of Firefox and Safari.

use aldente_model::trace::{Frame, FrameLang, MatchLevel, ThrownException, Trace};

use super::demunge::demunge;

/// Whether `text` looks like a JavaScript trace: it has frames with a
/// line and a column, and no JVM frames.
///
/// Example: a Node trace (`at app$core$f (/x/core.js:7:8)`) is JavaScript.
pub fn looks_like_js(text: &str) -> bool {
    let (mut js, mut jvm) = (0, 0);
    for line in text.lines() {
        if parse_frame(line).is_some() {
            js += 1;
        } else if super::jvm::parse_frame(line).is_some() {
            jvm += 1;
        }
    }
    js > 0 && jvm == 0
}

/// Every trace in `text`. A header line (`Error: …`, `TypeError: …`) starts a
/// trace; frames before any header form a trace without an exception
/// (Firefox and Safari print no header in `.stack`).
///
/// Example: a Node trace gives one trace whose first frame is the innermost.
pub fn parse(text: &str) -> Vec<Trace> {
    let mut traces: Vec<Trace> = Vec::new();
    let mut current: Option<Trace> = None;
    for line in text.lines() {
        if let Some(f) = parse_frame(line) {
            current.get_or_insert_with(Trace::default).frames.push(f);
        } else if let Some(ex) = header(line) {
            traces.extend(current.take());
            current = Some(Trace {
                exception: ex,
                ..Trace::default()
            });
        }
    }
    traces.extend(current);
    traces.retain(|t| !t.frames.is_empty());
    for t in &mut traces {
        for (i, f) in t.frames.iter_mut().enumerate() {
            f.index = i as u32;
        }
    }
    traces
}

/// `Error: message` or `TypeError: message` (any `…Error` name).
///
/// Example: `header("Error: bad qty: x")` gives class `Error`, message `bad qty: x`.
fn header(line: &str) -> Option<ThrownException> {
    let line = line.trim();
    let (class, message) = match line.split_once(": ") {
        Some((c, m)) => (c, Some(m)),
        None => (line, None),
    };
    let ok = class.ends_with("Error")
        && class
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$');
    ok.then(|| ThrownException {
        class: Some(class.to_owned()),
        message: message.map(str::to_owned),
        data: None,
    })
}

/// One JavaScript frame in V8 or Firefox/Safari form.
///
/// Example: `"    at app$core$parse_qty (/srv/app/out/app/core.js:7:8)"` gives
/// namespace `app.core`, name `parse-qty`, file `/srv/app/out/app/core.js`, 7:8.
pub fn parse_frame(raw: &str) -> Option<Frame> {
    let line = raw.trim();
    let (name, loc) = if let Some(rest) = line.strip_prefix("at ") {
        match rest.strip_suffix(')').and_then(|r| r.rsplit_once(" (")) {
            Some((name, loc)) => (Some(name), loc),
            None => (None, rest),
        }
    } else {
        let (name, loc) = line.rsplit_once('@')?;
        (Some(name).filter(|n| !n.is_empty()), loc)
    };
    let (file, line_no, col) = location(loc)?;
    let mut frame = Frame {
        index: 0,
        lang: FrameLang::Js,
        namespace: None,
        name: None,
        anonymous: true,
        file: Some(file),
        line: Some(line_no),
        col: Some(col),
        raw: line.to_owned(),
        noise: false,
        node: None,
        match_level: MatchLevel::None,
    };
    if let Some((ns, var)) = name.and_then(js_name) {
        frame.namespace = ns;
        frame.name = Some(var);
        frame.anonymous = false;
    }
    Some(frame)
}

/// `url:line:col` (the URL may itself contain `:`).
///
/// Example: `location("http://h:8080/out/app/core.js:7:8")` gives that URL, 7 and 8.
fn location(loc: &str) -> Option<(String, u32, u32)> {
    let (rest, col) = loc.rsplit_once(':')?;
    let (file, line) = rest.rsplit_once(':')?;
    let (line, col) = (line.parse().ok()?, col.parse().ok()?);
    (!file.is_empty() && !file.contains(char::is_whitespace)).then(|| (file.to_owned(), line, col))
}

/// A compiled ClojureScript function name as namespace and var.
/// `app$core$parse_qty` (`:none`), `app.core.parse_qty` (`:simple`), or a bare
/// `parse_qty` (Safari) without a namespace. `Object.` and `new ` prefixes,
/// ` [as x]` suffixes and multi-arity method suffixes are dropped.
///
/// Example: `js_name("cljs$core$seq_reduce.cljs$core$IFn$_invoke$arity$2")`
/// is `(Some("cljs.core"), "seq-reduce")`.
pub fn js_name(name: &str) -> Option<(Option<String>, String)> {
    let mut n = name.trim();
    n = n.strip_prefix("new ").unwrap_or(n);
    n = n.strip_prefix("Object.").unwrap_or(n);
    if let Some((head, _)) = n.split_once(" [as ") {
        n = head;
    }
    if n.is_empty() || n == "<anonymous>" || n.contains(' ') {
        return None;
    }
    let base = match n.split_once('.') {
        Some((head, _)) if head.contains('$') => head,
        _ => n,
    };
    let parts: Vec<&str> = if base.contains('$') {
        base.split('$').collect()
    } else {
        base.split('.').collect()
    };
    let (var, ns) = parts.split_last()?;
    if var.is_empty() {
        return None;
    }
    let ns = (!ns.is_empty()).then(|| ns.iter().map(|s| demunge(s)).collect::<Vec<_>>().join("."));
    Some((ns, demunge(var)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v8_firefox_and_safari_frames() {
        let f = parse_frame("    at app$core$parse_qty (/srv/app/out/app/core.js:7:8)").unwrap();
        assert_eq!(
            (f.namespace.as_deref(), f.name.as_deref(), f.line, f.col),
            (Some("app.core"), Some("parse-qty"), Some(7), Some(8))
        );
        let f = parse_frame("    at /srv/app/out/cljs/core.js:18041:89").unwrap();
        assert!(f.anonymous && f.name.is_none());
        let f =
            parse_frame("    at Object.cljs$core$seq [as seq] (/srv/app/out/cljs/core.js:4864:13)")
                .unwrap();
        assert_eq!(
            (f.namespace.as_deref(), f.name.as_deref()),
            (Some("cljs.core"), Some("seq"))
        );
        let f = parse_frame(
            "app$core$valid_QMARK___GT_ok_BANG_@http://localhost:8080/out/app/core.js:20:29",
        )
        .unwrap();
        assert_eq!(f.name.as_deref(), Some("valid?->ok!"));
        assert_eq!(
            f.file.as_deref(),
            Some("http://localhost:8080/out/app/core.js")
        );
        let f = parse_frame("parse_qty@http://localhost:8080/out/app/core.js:7:8").unwrap();
        assert_eq!((f.namespace, f.name.as_deref()), (None, Some("parse-qty")));
        let f = parse_frame("    at Module._compile (node:internal/modules/cjs/loader:1934:14)")
            .unwrap();
        assert_eq!(f.namespace.as_deref(), Some("Module"));
        assert!(parse_frame("\tat shop.core$checkout.invoke(core.clj:12)").is_none());
    }

    #[test]
    fn js_names() {
        assert_eq!(
            js_name("app.core.parse_qty"),
            Some((Some("app.core".into()), "parse-qty".into()))
        );
        assert_eq!(
            js_name("app$core_test$it_works"),
            Some((Some("app.core-test".into()), "it-works".into()))
        );
        assert_eq!(js_name("<anonymous>"), None);
    }

    #[test]
    fn detection() {
        assert!(looks_like_js("Error: x\n    at f (/a/b.js:1:2)\n"));
        assert!(!looks_like_js(
            "java.lang.Error: x\n\tat a.b$c.invoke(b.clj:1)\n"
        ));
    }
}
