//! The stack trace normalizer: raw traces in, canonical frames mapped to
//! nodes out. See "Stack trace normalizer" in
//! `docs/plan/08-crater-and-traces.md`.

pub mod demunge;
mod edn;
pub mod jvm;
mod mapping;
#[cfg(test)]
mod tests;

use aldente_graph::Graph;
use aldente_model::trace::{Frame, Trace, TraceFormat, TraceReport};
use aldente_rules::RulesError;
use thiserror::Error;

pub use mapping::TraceIndex;

/// Why a trace could not be normalized.
#[derive(Debug, Error)]
pub enum TraceError {
    #[error("no stack trace found in the input")]
    NoTrace,
    #[error(transparent)]
    Rules(#[from] RulesError),
}

/// Detects the input format, parses every trace in it, and maps each frame
/// to a node of `graph`.
///
/// Example: a `printStackTrace` from clj-mini's `checkout` gives one trace
/// whose `shop.*` frames map `exact`.
pub fn normalize(text: &str, graph: &Graph) -> Result<TraceReport, TraceError> {
    let (format, mut traces) = parse(text)?;
    let index = TraceIndex::new(graph);
    let noise = aldente_rules::trace::noise()?;
    for t in &mut traces {
        for_each_frames(t, &mut |frames| mapping::map_frames(&index, &noise, frames));
    }
    Ok(TraceReport { format, traces })
}

/// The format and the traces, before mapping.
///
/// Example: text starting with `{` or `#error` is tried as EDN first.
pub fn parse(text: &str) -> Result<(TraceFormat, Vec<Trace>), TraceError> {
    let start = text.trim_start();
    if (start.starts_with('{') || start.starts_with("#error"))
        && let Some(traces) = edn::parse(text)
    {
        return Ok((TraceFormat::Edn, traces));
    }
    let traces = jvm::parse(text);
    if traces.is_empty() {
        return Err(TraceError::NoTrace);
    }
    Ok((TraceFormat::JvmText, traces))
}

/// Calls `f` on the frames of `t` and of every cause.
///
/// Example: a trace with one cause calls `f` twice.
pub fn for_each_frames(t: &mut Trace, f: &mut dyn FnMut(&mut [Frame])) {
    f(&mut t.frames);
    for c in &mut t.causes {
        for_each_frames(c, f);
    }
}
