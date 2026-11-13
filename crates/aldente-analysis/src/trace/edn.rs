//! Traces as Clojure data: `Throwable->map`, a printed `#error {…}`, and a
//! `clojure.main` error report (`{:clojure.main/trace {…}}`). Read with the
//! data-only EDN reader; nothing is evaluated.

use aldente_acquire::edn::{Edn, read_all};
use aldente_model::trace::{Frame, ThrownException, Trace};

use super::jvm::parse_frame;

/// The traces in `text`, or `None` when it is not such data.
///
/// `:via` lists the exceptions outermost first; each gets its `:at` frame,
/// and the innermost gets the full `:trace` (which belongs to the root cause).
///
/// Example: a `Throwable->map` of an `ex-info` wrapping another gives one
/// trace with one cause; the cause has every `:trace` frame.
pub fn parse(text: &str) -> Option<Vec<Trace>> {
    let forms = read_all(text).ok()?;
    let traces: Vec<Trace> = forms.iter().filter_map(one).collect();
    (!traces.is_empty()).then_some(traces)
}

/// One top-level form as a trace.
///
/// Example: see [`parse`].
fn one(form: &Edn) -> Option<Trace> {
    let mut map = form;
    if let Edn::Tagged(tag, inner) = map {
        if tag != "error" {
            return None;
        }
        map = inner;
    }
    if let Some(inner) = map.get_kw("clojure.main/trace") {
        map = inner;
    }
    let frames: Vec<Frame> = map
        .get_kw("trace")
        .and_then(Edn::items)
        .map(|items| items.iter().filter_map(frame).collect())
        .unwrap_or_default();
    let via: Vec<&Edn> = map
        .get_kw("via")
        .and_then(Edn::items)
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    if via.is_empty() && frames.is_empty() {
        return None;
    }
    let mut inner: Option<Trace> = None;
    let last = via.len().saturating_sub(1);
    for (i, v) in via.iter().enumerate().rev() {
        let own = if i == last {
            frames.clone()
        } else {
            v.get_kw("at").and_then(frame).into_iter().collect()
        };
        inner = Some(trace(exception(v), own, inner));
    }
    Some(inner.unwrap_or_else(|| {
        let ex = ThrownException {
            class: None,
            message: map.get_kw("cause").and_then(Edn::as_str).map(str::to_owned),
            data: map.get_kw("data").map(print),
        };
        trace(ex, frames, None)
    }))
}

/// A trace with numbered frames.
///
/// Example: `trace(ex, [f, g], None)` gives indexes 0 and 1.
fn trace(exception: ThrownException, mut frames: Vec<Frame>, cause: Option<Trace>) -> Trace {
    for (i, f) in frames.iter_mut().enumerate() {
        f.index = i as u32;
    }
    Trace {
        exception,
        frames,
        omitted: 0,
        causes: cause.into_iter().collect(),
    }
}

/// `{:type … :message … :data …}` from `:via`.
///
/// Example: `{:type clojure.lang.ExceptionInfo :message "x"}` gives that class and message.
fn exception(v: &Edn) -> ThrownException {
    ThrownException {
        class: v.get_kw("type").map(print),
        message: v.get_kw("message").and_then(Edn::as_str).map(str::to_owned),
        data: v.get_kw("data").map(print),
    }
}

/// `[class method "file" line]` as a frame, parsed like the text form.
///
/// Example: `[shop.pricing$half_off invokeStatic "pricing.clj" 10]` gives
/// namespace `shop.pricing`, name `half-off`, line 10.
fn frame(v: &Edn) -> Option<Frame> {
    let [class, method, file, line] = v.items()? else {
        return None;
    };
    let (Edn::Symbol(class), Edn::Symbol(method)) = (class, method) else {
        return None;
    };
    let file = file.as_str().unwrap_or("Unknown Source");
    let loc = match line {
        Edn::Number(n) if file != "Unknown Source" => format!("{file}:{n}"),
        _ => file.to_owned(),
    };
    parse_frame(&format!("{class}.{method}({loc})"))
}

/// A short printed form of data (enough for display; strings quoted).
///
/// Example: `{:id 7}` prints as `{:id 7}`.
fn print(v: &Edn) -> String {
    let seq = |open: &str, items: &[Edn], close: &str| {
        format!(
            "{open}{}{close}",
            items.iter().map(print).collect::<Vec<_>>().join(" ")
        )
    };
    match v {
        Edn::Nil => "nil".into(),
        Edn::Bool(b) => b.to_string(),
        Edn::Number(n) => n.clone(),
        Edn::Str(s) => format!("{s:?}"),
        Edn::Char(c) => format!("\\{c}"),
        Edn::Keyword(k) => format!(":{k}"),
        Edn::Symbol(s) => s.clone(),
        Edn::List(xs) => seq("(", xs, ")"),
        Edn::Vector(xs) => seq("[", xs, "]"),
        Edn::Set(xs) => seq("#{", xs, "}"),
        Edn::Map(kvs) => format!(
            "{{{}}}",
            kvs.iter()
                .map(|(k, v)| format!("{} {}", print(k), print(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Edn::Tagged(t, x) => format!("#{t} {}", print(x)),
        Edn::Regex(r) => format!("#\"{r}\""),
        Edn::AnonFn(xs) => seq("#(", xs, ")"),
        Edn::ReaderCond { .. } | Edn::ReadEval(_) => "…".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throwable_map_nests_via_and_gives_the_root_the_trace() {
        let text = r#"{:via [{:type clojure.lang.ExceptionInfo, :message "outer", :data {:step :charge}, :at [a.b$c invokeStatic "b.clj" 26]}
                              {:type clojure.lang.ExceptionInfo, :message "inner", :at [a.b$d invokeStatic "b.clj" 20]}],
                       :trace [[a.b$d invokeStatic "b.clj" 20] [a.b$c invoke "b.clj" 22] [clojure.lang.Var applyTo "Var.java" 707]],
                       :cause "inner"}"#;
        let t = &parse(text).unwrap()[0];
        assert_eq!(t.exception.message.as_deref(), Some("outer"));
        assert_eq!(t.exception.data.as_deref(), Some("{:step :charge}"));
        assert_eq!(t.frames.len(), 1);
        assert_eq!(t.causes[0].frames.len(), 3);
        assert_eq!(
            t.causes[0].frames[2].namespace.as_deref(),
            Some("clojure.lang.Var")
        );
    }

    #[test]
    fn printed_error_and_main_report_unwrap() {
        let e = r#"#error {:cause "x" :via [{:type java.lang.Error :message "x" :at [a.b$c invoke "b.clj" 1]}] :trace [[a.b$c invoke "b.clj" 1]]}"#;
        assert_eq!(parse(e).unwrap()[0].frames.len(), 1);
        let r = format!(
            "{{:clojure.main/message \"m\" :clojure.main/trace {}}}",
            e.trim_start_matches("#error ")
        );
        assert_eq!(parse(&r).unwrap()[0].frames[0].name.as_deref(), Some("c"));
        assert!(parse("{:a 1}").is_none());
        assert!(parse("not edn (").is_none());
    }
}
