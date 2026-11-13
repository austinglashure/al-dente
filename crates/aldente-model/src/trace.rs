//! The `data` shape of `aldente trace normalize`: stack traces as canonical
//! frames, each mapped to a node when possible.
//! See "Stack trace normalizer" in `docs/plan/08-crater-and-traces.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::NodeId;

/// How well a frame was tied to a node.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum MatchLevel {
    /// Name and line fall inside the node's range (for anonymous functions:
    /// the namespace is known and the line falls inside the node).
    Exact,
    /// The name resolves but the line does not fit (for example stale code).
    Name,
    /// Only the file and line resolved, mapped to the enclosing node.
    Line,
    None,
}

/// The language a frame's code is written in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum FrameLang {
    Clj,
    Cljs,
    Java,
    Js,
}

/// One stack frame. For Java frames `namespace` is the class and `name` the
/// method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Frame {
    /// Position in its trace, innermost (where the exception was thrown) first.
    pub index: u32,
    pub lang: FrameLang,
    pub namespace: Option<String>,
    /// For anonymous functions: the enclosing var when the frame names one.
    pub name: Option<String>,
    pub anonymous: bool,
    /// As the trace gives it (often only the file name), or the
    /// repository-relative path after source mapping.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub col: Option<u32>,
    /// The frame as it appeared in the input.
    pub raw: String,
    /// Runtime, library or tooling machinery (`rules/trace-noise.toml`).
    /// Frames from the repository are never noise.
    pub noise: bool,
    pub node: Option<NodeId>,
    #[serde(rename = "match")]
    pub match_level: MatchLevel,
}

/// The exception a trace belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct ThrownException {
    /// The class, e.g. `java.lang.ArithmeticException` (or as short as the
    /// input gives it).
    #[serde(rename = "type")]
    pub class: Option<String>,
    pub message: Option<String>,
    /// `ex-info` data as printed, when the input carries it.
    pub data: Option<String>,
}

/// One exception with its frames and the exceptions that caused it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Trace {
    pub exception: ThrownException,
    pub frames: Vec<Frame>,
    /// Frames the input left out (`... 12 more`): shared with the enclosing trace.
    pub omitted: u32,
    pub causes: Vec<Trace>,
}

/// The input format a trace was read from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum TraceFormat {
    /// Java's `printStackTrace`, `clojure.repl/pst`, `clojure.stacktrace`
    /// and log files containing them.
    JvmText,
    /// `Throwable->map`, a printed `#error`, or a `clojure.main` report.
    Edn,
    /// Browser or Node JavaScript stack traces.
    Js,
}

/// `aldente trace normalize`: every trace found in the input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct TraceReport {
    pub format: TraceFormat,
    pub traces: Vec<Trace>,
}
