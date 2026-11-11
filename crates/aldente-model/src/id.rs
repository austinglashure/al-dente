//! Public node identifiers: readable strings, stable across runs and
//! machines. Formats are in "Identifiers" in `docs/plan/03-graph-model.md`.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::kinds::{BlindSpotCategory, Lang, NodeKind};
use crate::path::{PathError, RepoPath};

/// Why a string or a set of parts is not a valid ID.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("ID has no `<prefix>:`: {0:?}")]
    NoPrefix(String),
    #[error("unknown ID prefix {0:?}")]
    UnknownPrefix(String),
    #[error("malformed {kind} ID {id:?}: {why}")]
    Malformed {
        kind: &'static str,
        id: String,
        why: &'static str,
    },
    #[error("invalid {part}: {value:?} ({why})")]
    BadPart {
        part: &'static str,
        value: String,
        why: &'static str,
    },
    #[error("bad path in ID: {0}")]
    Path(#[from] PathError),
}

/// `<lang>:<unit>//<ns>`, e.g. `clj:services/billing//billing.core`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamespaceId {
    lang: Lang,
    unit: RepoPath,
    ns: String,
}

/// `<lang>:<unit>//<ns>/<name>`, e.g. `clj:.//app.orders/submit!`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolId {
    ns: NamespaceId,
    name: String,
}

/// `<symbol id>@<discriminator>`, e.g. `clj:.//app.core/handle@:order/paid`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImplId {
    symbol: SymbolId,
    discriminator: String,
}

/// `ext:<lang>//<ns>/<name>`, e.g. `ext:clj//clojure.core/map`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalId {
    lang: Lang,
    ns: String,
    name: String,
}

/// `lib:<ecosystem>:<coordinate>`, e.g. `lib:maven:reagent/reagent`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LibraryId {
    ecosystem: String,
    coordinate: String,
}

/// `java:<class>[#member]` or `js:<path>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlatformId {
    Java {
        class: String,
        member: Option<String>,
    },
    Js {
        path: String,
    },
}

/// `hole:<category>@<file>:<line>:<col>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HoleId {
    category: BlindSpotCategory,
    file: RepoPath,
    line: u32,
    col: u32,
}

/// Any node's public identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeId {
    /// `repo:.`
    Repo,
    /// `unit:<dir>`, e.g. `unit:services/billing`; the root unit is `unit:.`.
    Unit(RepoPath),
    Namespace(NamespaceId),
    Symbol(SymbolId),
    Impl(ImplId),
    External(ExternalId),
    Library(LibraryId),
    Platform(PlatformId),
    Hole(HoleId),
    /// `endpoint:<text>`; structure defined in Phase 6.
    Endpoint(String),
    /// `datastore:<text>`; structure defined in Phase 6.
    Datastore(String),
    /// `host:<text>`; structure defined in Phase 6.
    Host(String),
    /// `state:<text>`; structure defined in Phase 8.
    State(String),
}

/// Rejects an empty part.
///
/// Example: `non_empty("ns", "")` is an error; `non_empty("ns", "a")` is ok.
fn non_empty(part: &'static str, value: &str) -> Result<(), IdError> {
    if value.is_empty() {
        return Err(IdError::BadPart {
            part,
            value: value.to_owned(),
            why: "empty",
        });
    }
    Ok(())
}

/// Rejects a part that is empty or contains `forbidden`.
///
/// Example: `plain_part("namespace", "a/b", '/')` is an error.
fn plain_part(part: &'static str, value: &str, forbidden: char) -> Result<(), IdError> {
    non_empty(part, value)?;
    if value.contains(forbidden) {
        return Err(IdError::BadPart {
            part,
            value: value.to_owned(),
            why: "contains a reserved character",
        });
    }
    Ok(())
}

impl NamespaceId {
    /// Builds a namespace ID. `ns` must be non-empty and contain no `/`.
    ///
    /// Example: `NamespaceId::new(Lang::Clj, RepoPath::root(), "app.core")`
    /// formats as `clj:.//app.core`.
    pub fn new(lang: Lang, unit: RepoPath, ns: &str) -> Result<NamespaceId, IdError> {
        plain_part("namespace", ns, '/')?;
        Ok(NamespaceId {
            lang,
            unit,
            ns: ns.to_owned(),
        })
    }

    /// The language prefix, e.g. `Lang::Clj`.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// The owning unit's directory, e.g. `services/billing`.
    pub fn unit(&self) -> &RepoPath {
        &self.unit
    }

    /// The namespace name, e.g. `app.core`.
    pub fn ns(&self) -> &str {
        &self.ns
    }
}

impl SymbolId {
    /// Builds a symbol ID. `name` must be non-empty; it may contain anything.
    ///
    /// Example: a symbol `/` in `clojure.core` of the root unit formats as
    /// `clj:.//clojure.core//`.
    pub fn new(ns: NamespaceId, name: &str) -> Result<SymbolId, IdError> {
        non_empty("symbol name", name)?;
        Ok(SymbolId {
            ns,
            name: name.to_owned(),
        })
    }

    /// The enclosing namespace's ID.
    pub fn namespace(&self) -> &NamespaceId {
        &self.ns
    }

    /// The unescaped symbol name, e.g. `submit!`.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl ImplId {
    /// Builds an impl ID. `discriminator` must be non-empty.
    ///
    /// Example: symbol `clj:.//app.core/<top>` with discriminator `L42`
    /// formats as `clj:.//app.core/<top>@L42`.
    pub fn new(symbol: SymbolId, discriminator: &str) -> Result<ImplId, IdError> {
        non_empty("impl discriminator", discriminator)?;
        Ok(ImplId {
            symbol,
            discriminator: discriminator.to_owned(),
        })
    }

    /// The symbol this body belongs to.
    pub fn symbol(&self) -> &SymbolId {
        &self.symbol
    }

    /// The discriminator, e.g. `:order/paid` or `L42`.
    pub fn discriminator(&self) -> &str {
        &self.discriminator
    }
}

impl ExternalId {
    /// Builds an external symbol ID.
    ///
    /// Example: `ExternalId::new(Lang::Clj, "clojure.core", "map")` formats as
    /// `ext:clj//clojure.core/map`.
    pub fn new(lang: Lang, ns: &str, name: &str) -> Result<ExternalId, IdError> {
        plain_part("namespace", ns, '/')?;
        non_empty("symbol name", name)?;
        Ok(ExternalId {
            lang,
            ns: ns.to_owned(),
            name: name.to_owned(),
        })
    }

    /// The language, e.g. `Lang::Clj`.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// The namespace, e.g. `clojure.core`.
    pub fn ns(&self) -> &str {
        &self.ns
    }

    /// The unescaped name, e.g. `map`.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl LibraryId {
    /// Builds a library ID. `ecosystem` must not contain `:`.
    ///
    /// Example: `LibraryId::new("npm", "react")` formats as `lib:npm:react`.
    pub fn new(ecosystem: &str, coordinate: &str) -> Result<LibraryId, IdError> {
        plain_part("ecosystem", ecosystem, ':')?;
        non_empty("coordinate", coordinate)?;
        Ok(LibraryId {
            ecosystem: ecosystem.to_owned(),
            coordinate: coordinate.to_owned(),
        })
    }

    /// The ecosystem, e.g. `maven`.
    pub fn ecosystem(&self) -> &str {
        &self.ecosystem
    }

    /// The coordinate, e.g. `reagent/reagent`.
    pub fn coordinate(&self) -> &str {
        &self.coordinate
    }
}

impl PlatformId {
    /// Builds a Java platform ID. `class` must not contain `#`.
    ///
    /// Example: `PlatformId::java("java.net.Socket", Some("connect"))` formats
    /// as `java:java.net.Socket#connect`.
    pub fn java(class: &str, member: Option<&str>) -> Result<PlatformId, IdError> {
        plain_part("java class", class, '#')?;
        if let Some(m) = member {
            non_empty("java member", m)?;
        }
        Ok(PlatformId::Java {
            class: class.to_owned(),
            member: member.map(str::to_owned),
        })
    }

    /// Builds a JavaScript platform ID.
    ///
    /// Example: `PlatformId::js("fetch")` formats as `js:fetch`.
    pub fn js(path: &str) -> Result<PlatformId, IdError> {
        non_empty("js path", path)?;
        Ok(PlatformId::Js {
            path: path.to_owned(),
        })
    }
}

impl HoleId {
    /// Builds a hole ID.
    ///
    /// Example: `HoleId::new(BlindSpotCategory::DynamicEval, path, 12, 3)` with
    /// path `src/a.clj` formats as `hole:dynamic-eval@src/a.clj:12:3`.
    pub fn new(category: BlindSpotCategory, file: RepoPath, line: u32, col: u32) -> HoleId {
        HoleId {
            category,
            file,
            line,
            col,
        }
    }

    /// The blind-spot category.
    pub fn category(&self) -> BlindSpotCategory {
        self.category
    }

    /// The file containing the hole.
    pub fn file(&self) -> &RepoPath {
        &self.file
    }

    /// The 1-based line.
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The 1-based column.
    pub fn col(&self) -> u32 {
        self.col
    }
}

impl NodeId {
    /// Builds an opaque Phase 6/8 ID (`endpoint`, `datastore`, `host`, `state`).
    ///
    /// Example: `NodeId::opaque(NodeKind::Host, "api.stripe.com")` formats as
    /// `host:api.stripe.com`. Other kinds are an error.
    pub fn opaque(kind: NodeKind, text: &str) -> Result<NodeId, IdError> {
        non_empty("opaque ID text", text)?;
        let text = text.to_owned();
        match kind {
            NodeKind::Endpoint => Ok(NodeId::Endpoint(text)),
            NodeKind::Datastore => Ok(NodeId::Datastore(text)),
            NodeKind::Host => Ok(NodeId::Host(text)),
            NodeKind::State => Ok(NodeId::State(text)),
            _ => Err(IdError::BadPart {
                part: "opaque ID kind",
                value: format!("{kind:?}"),
                why: "only endpoint, datastore, host and state IDs are opaque",
            }),
        }
    }

    /// The kind of node this ID names.
    ///
    /// Example: the ID `ext:clj//clojure.core/map` has kind `NodeKind::External`.
    pub fn kind(&self) -> NodeKind {
        match self {
            NodeId::Repo => NodeKind::Repo,
            NodeId::Unit(_) => NodeKind::Unit,
            NodeId::Namespace(_) => NodeKind::Namespace,
            NodeId::Symbol(_) => NodeKind::Symbol,
            NodeId::Impl(_) => NodeKind::Impl,
            NodeId::External(_) => NodeKind::External,
            NodeId::Library(_) => NodeKind::Library,
            NodeId::Platform(_) => NodeKind::Platform,
            NodeId::Hole(_) => NodeKind::Hole,
            NodeId::Endpoint(_) => NodeKind::Endpoint,
            NodeId::Datastore(_) => NodeKind::Datastore,
            NodeId::Host(_) => NodeKind::Host,
            NodeId::State(_) => NodeKind::State,
        }
    }
}

/// Escapes a symbol name for an ID: `%` becomes `%25` and `@` becomes `%40`,
/// so a bare `@` always starts an impl discriminator.
///
/// Example: `escape_name("x@y%")` is `"x%40y%25"`.
fn escape_name(name: &str) -> Cow<'_, str> {
    if name.contains(['%', '@']) {
        Cow::Owned(name.replace('%', "%25").replace('@', "%40"))
    } else {
        Cow::Borrowed(name)
    }
}

/// Reverses [`escape_name`], rejecting bare `@` and any other `%` sequence so
/// that each name has exactly one written form.
///
/// Example: `unescape_name("x%40y")` is `Ok("x@y")`; `unescape_name("x@y")`
/// and `unescape_name("%41")` are errors.
fn unescape_name(raw: &str) -> Result<String, IdError> {
    let bad = |why| IdError::BadPart {
        part: "symbol name",
        value: raw.to_owned(),
        why,
    };
    non_empty("symbol name", raw)?;
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find(['%', '@']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(after) = tail.strip_prefix("%25") {
            out.push('%');
            rest = after;
        } else if let Some(after) = tail.strip_prefix("%40") {
            out.push('@');
            rest = after;
        } else {
            return Err(bad("`@` and `%` must be written %40 and %25"));
        }
    }
    out.push_str(rest);
    Ok(out)
}

impl fmt::Display for NamespaceId {
    /// Writes `<lang>:<unit>//<ns>`, e.g. `clj:.//app.core`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}//{}", self.lang.as_str(), self.unit, self.ns)
    }
}

impl fmt::Display for SymbolId {
    /// Writes `<namespace id>/<escaped name>`, e.g. `clj:.//app.core/run!`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.ns, escape_name(&self.name))
    }
}

impl fmt::Display for NodeId {
    /// Writes the canonical string form described in the module docs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeId::Repo => f.write_str("repo:."),
            NodeId::Unit(dir) => write!(f, "unit:{dir}"),
            NodeId::Namespace(ns) => write!(f, "{ns}"),
            NodeId::Symbol(sym) => write!(f, "{sym}"),
            NodeId::Impl(i) => write!(f, "{}@{}", i.symbol, i.discriminator),
            NodeId::External(e) => {
                write!(
                    f,
                    "ext:{}//{}/{}",
                    e.lang.as_str(),
                    e.ns,
                    escape_name(&e.name)
                )
            }
            NodeId::Library(l) => write!(f, "lib:{}:{}", l.ecosystem, l.coordinate),
            NodeId::Platform(PlatformId::Java {
                class,
                member: None,
            }) => write!(f, "java:{class}"),
            NodeId::Platform(PlatformId::Java {
                class,
                member: Some(m),
            }) => write!(f, "java:{class}#{m}"),
            NodeId::Platform(PlatformId::Js { path }) => write!(f, "js:{path}"),
            NodeId::Hole(h) => write!(
                f,
                "hole:{}@{}:{}:{}",
                h.category.as_str(),
                h.file,
                h.line,
                h.col
            ),
            NodeId::Endpoint(t) => write!(f, "endpoint:{t}"),
            NodeId::Datastore(t) => write!(f, "datastore:{t}"),
            NodeId::Host(t) => write!(f, "host:{t}"),
            NodeId::State(t) => write!(f, "state:{t}"),
        }
    }
}

/// Builds a `Malformed` error for `kind` IDs.
///
/// Example: `malformed("hole", "hole:x", "missing `@`")`.
fn malformed(kind: &'static str, id: &str, why: &'static str) -> IdError {
    IdError::Malformed {
        kind,
        id: id.to_owned(),
        why,
    }
}

/// Parses the part after `<lang>:` of a namespace, symbol or impl ID.
///
/// Example: `parse_local(Lang::Clj, "clj:.//a/b@L1", ".//a/b@L1")` is the impl
/// `b@L1` in namespace `a` of the root unit.
fn parse_local(lang: Lang, id: &str, rest: &str) -> Result<NodeId, IdError> {
    let (unit, tail) = rest
        .split_once("//")
        .ok_or_else(|| malformed("namespace or symbol", id, "missing `//` after the unit"))?;
    let unit = RepoPath::parse(unit)?;
    let Some((ns, name_part)) = tail.split_once('/') else {
        return Ok(NodeId::Namespace(NamespaceId::new(lang, unit, tail)?));
    };
    let ns = NamespaceId::new(lang, unit, ns)?;
    match name_part.split_once('@') {
        None => Ok(NodeId::Symbol(SymbolId::new(
            ns,
            &unescape_name(name_part)?,
        )?)),
        Some((name, disc)) => {
            let symbol = SymbolId::new(ns, &unescape_name(name)?)?;
            Ok(NodeId::Impl(ImplId::new(symbol, disc)?))
        }
    }
}

/// Parses the part after `ext:`.
///
/// Example: `parse_external("ext:clj//clojure.core/map", "clj//clojure.core/map")`.
fn parse_external(id: &str, rest: &str) -> Result<NodeId, IdError> {
    let (lang, tail) = rest
        .split_once("//")
        .ok_or_else(|| malformed("external", id, "missing `//` after the language"))?;
    let lang =
        Lang::from_token(lang).ok_or_else(|| malformed("external", id, "unknown language"))?;
    let (ns, name) = tail
        .split_once('/')
        .ok_or_else(|| malformed("external", id, "missing `/<name>`"))?;
    Ok(NodeId::External(ExternalId::new(
        lang,
        ns,
        &unescape_name(name)?,
    )?))
}

/// Parses the part after `hole:`: category up to the first `@`, then line and
/// column from the right so the file may contain `:` and `@`.
///
/// Example: `parse_hole("hole:interop@a:b.clj:3:4", "interop@a:b.clj:3:4")`.
fn parse_hole(id: &str, rest: &str) -> Result<NodeId, IdError> {
    let (category, location) = rest
        .split_once('@')
        .ok_or_else(|| malformed("hole", id, "missing `@`"))?;
    let category = BlindSpotCategory::from_token(category)
        .ok_or_else(|| malformed("hole", id, "unknown category"))?;
    let (file_line, col) = location
        .rsplit_once(':')
        .ok_or_else(|| malformed("hole", id, "missing `:<col>`"))?;
    let (file, line) = file_line
        .rsplit_once(':')
        .ok_or_else(|| malformed("hole", id, "missing `:<line>`"))?;
    let number = |s: &str| -> Result<u32, IdError> {
        // Reject signs and leading zeros so each hole has one written form.
        let canonical = !s.is_empty()
            && s.bytes().all(|b| b.is_ascii_digit())
            && (s == "0" || !s.starts_with('0'));
        if !canonical {
            return Err(malformed(
                "hole",
                id,
                "line and column must be plain numbers",
            ));
        }
        s.parse()
            .map_err(|_| malformed("hole", id, "line or column out of range"))
    };
    Ok(NodeId::Hole(HoleId::new(
        category,
        RepoPath::parse(file)?,
        number(line)?,
        number(col)?,
    )))
}

impl FromStr for NodeId {
    type Err = IdError;

    /// Parses the canonical form. Only strings `to_string` could produce are
    /// accepted.
    ///
    /// Example: `"clj:.//app.orders/submit!".parse::<NodeId>()` is a
    /// `NodeId::Symbol`.
    fn from_str(s: &str) -> Result<NodeId, IdError> {
        let (prefix, rest) = s
            .split_once(':')
            .ok_or_else(|| IdError::NoPrefix(s.to_owned()))?;
        match prefix {
            "repo" if rest == "." => Ok(NodeId::Repo),
            "repo" => Err(malformed("repo", s, "the only repo ID is `repo:.`")),
            "unit" => Ok(NodeId::Unit(RepoPath::parse(rest)?)),
            "ext" => parse_external(s, rest),
            "lib" => {
                let (eco, coord) = rest
                    .split_once(':')
                    .ok_or_else(|| malformed("library", s, "missing `:<coordinate>`"))?;
                Ok(NodeId::Library(LibraryId::new(eco, coord)?))
            }
            "java" => {
                let platform = match rest.split_once('#') {
                    Some((class, member)) => PlatformId::java(class, Some(member))?,
                    None => PlatformId::java(rest, None)?,
                };
                Ok(NodeId::Platform(platform))
            }
            "js" => Ok(NodeId::Platform(PlatformId::js(rest)?)),
            "hole" => parse_hole(s, rest),
            "endpoint" => NodeId::opaque(NodeKind::Endpoint, rest),
            "datastore" => NodeId::opaque(NodeKind::Datastore, rest),
            "host" => NodeId::opaque(NodeKind::Host, rest),
            "state" => NodeId::opaque(NodeKind::State, rest),
            other => match Lang::from_token(other) {
                Some(lang) => parse_local(lang, s, rest),
                None => Err(IdError::UnknownPrefix(other.to_owned())),
            },
        }
    }
}

impl Serialize for NodeId {
    /// Serializes as the canonical string.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for NodeId {
    /// Deserializes from the canonical string, rejecting invalid IDs.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for NodeId {
    /// Names the schema `NodeId`.
    fn schema_name() -> Cow<'static, str> {
        "NodeId".into()
    }

    /// A string in one of the ID formats from the graph model.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Public node ID, e.g. `clj:.//app.orders/submit!` or `unit:.`. See docs/plan/03-graph-model.md."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::PathStyle;
    use crate::strategies::arb_node_id;
    use proptest::prelude::*;

    fn path(s: &str) -> RepoPath {
        RepoPath::parse(s).unwrap()
    }

    fn ns(lang: Lang, unit: &str, ns: &str) -> NamespaceId {
        NamespaceId::new(lang, path(unit), ns).unwrap()
    }

    fn sym(unit: &str, n: &str, name: &str) -> SymbolId {
        SymbolId::new(ns(Lang::Clj, unit, n), name).unwrap()
    }

    /// Every node kind with the example from the plan (or a chosen one for
    /// kinds the plan leaves opaque).
    fn examples() -> Vec<(NodeId, &'static str)> {
        vec![
            (NodeId::Repo, "repo:."),
            (NodeId::Unit(RepoPath::root()), "unit:."),
            (
                NodeId::Unit(path("services/billing")),
                "unit:services/billing",
            ),
            (
                NodeId::Namespace(ns(Lang::Clj, "services/billing", "billing.core")),
                "clj:services/billing//billing.core",
            ),
            (
                NodeId::Symbol(sym("services/billing", "billing.core", "charge!")),
                "clj:services/billing//billing.core/charge!",
            ),
            (
                NodeId::Impl(
                    ImplId::new(
                        sym("services/billing", "billing.core", "handle"),
                        ":order/paid",
                    )
                    .unwrap(),
                ),
                "clj:services/billing//billing.core/handle@:order/paid",
            ),
            (
                NodeId::Impl(
                    ImplId::new(sym("services/billing", "billing.core", "<top>"), "L42").unwrap(),
                ),
                "clj:services/billing//billing.core/<top>@L42",
            ),
            (
                NodeId::External(ExternalId::new(Lang::Clj, "clojure.core", "map").unwrap()),
                "ext:clj//clojure.core/map",
            ),
            (
                NodeId::Library(LibraryId::new("maven", "reagent/reagent").unwrap()),
                "lib:maven:reagent/reagent",
            ),
            (
                NodeId::Library(LibraryId::new("npm", "react").unwrap()),
                "lib:npm:react",
            ),
            (
                NodeId::Platform(PlatformId::java("java.net.Socket", Some("connect")).unwrap()),
                "java:java.net.Socket#connect",
            ),
            (
                NodeId::Platform(PlatformId::java("java.io.File", None).unwrap()),
                "java:java.io.File",
            ),
            (
                NodeId::Platform(PlatformId::js("fetch").unwrap()),
                "js:fetch",
            ),
            (
                NodeId::Hole(HoleId::new(
                    BlindSpotCategory::DynamicEval,
                    path("src/app/core.clj"),
                    12,
                    3,
                )),
                "hole:dynamic-eval@src/app/core.clj:12:3",
            ),
            (
                NodeId::opaque(NodeKind::Endpoint, "GET /orders/:id").unwrap(),
                "endpoint:GET /orders/:id",
            ),
            (
                NodeId::opaque(NodeKind::Datastore, "kafka:orders").unwrap(),
                "datastore:kafka:orders",
            ),
            (
                NodeId::opaque(NodeKind::Host, "api.stripe.com").unwrap(),
                "host:api.stripe.com",
            ),
            (
                NodeId::opaque(NodeKind::State, ":order/paid").unwrap(),
                "state::order/paid",
            ),
        ]
    }

    #[test]
    fn plan_examples_format_and_parse() {
        for (id, text) in examples() {
            assert_eq!(id.to_string(), text);
            assert_eq!(text.parse::<NodeId>().unwrap(), id, "parsing {text}");
        }
    }

    #[test]
    fn examples_cover_every_node_kind() {
        let mut kinds: Vec<_> = examples().iter().map(|(id, _)| id.kind()).collect();
        kinds.dedup();
        assert_eq!(kinds, NodeKind::ALL);
    }

    #[test]
    fn awkward_symbol_names_round_trip() {
        for name in [
            "/", "->Record", "valid?", "swap!", "a/b", "λ→é", "x@y", "100%", "<top>",
        ] {
            let id = NodeId::Symbol(sym(".", "app.core", name));
            let text = id.to_string();
            assert_eq!(text.parse::<NodeId>().unwrap(), id, "via {text}");
        }
        assert_eq!(
            NodeId::Symbol(sym(".", "clojure.core", "/")).to_string(),
            "clj:.//clojure.core//"
        );
    }

    #[test]
    fn at_and_percent_in_names_are_escaped() {
        let id = NodeId::Impl(ImplId::new(sym(".", "a", "x@y%"), "\"a@b\"").unwrap());
        assert_eq!(id.to_string(), "clj:.//a/x%40y%25@\"a@b\"");
        assert_eq!(id.to_string().parse::<NodeId>().unwrap(), id);
    }

    #[test]
    fn hole_file_may_contain_colons_and_at_signs() {
        let id = NodeId::Hole(HoleId::new(
            BlindSpotCategory::Interop,
            path("odd:dir/f@1:2.clj"),
            7,
            9,
        ));
        assert_eq!(id.to_string(), "hole:interop@odd:dir/f@1:2.clj:7:9");
        assert_eq!(id.to_string().parse::<NodeId>().unwrap(), id);
    }

    #[test]
    fn unit_named_ext_does_not_collide_with_externals() {
        let local = NodeId::Symbol(sym("ext", "clojure.core", "map"));
        let external = NodeId::External(ExternalId::new(Lang::Clj, "clojure.core", "map").unwrap());
        assert_ne!(local.to_string(), external.to_string());
        assert_eq!(local.to_string().parse::<NodeId>().unwrap(), local);
    }

    #[test]
    fn windows_unit_paths_normalize_in_ids() {
        let unit = RepoPath::from_native_str("services\\billing", PathStyle::Windows).unwrap();
        let id =
            NodeId::Namespace(NamespaceId::new(Lang::Clj, unit.clone(), "billing.core").unwrap());
        assert_eq!(id.to_string(), "clj:services/billing//billing.core");
        assert_eq!(NodeId::Unit(unit).to_string(), "unit:services/billing");
    }

    #[test]
    fn invalid_ids_are_rejected() {
        for s in [
            "",
            "nocolon",
            "java",
            "repo:",
            "repo:x",
            "unit:",
            "unit:./a",
            "unit:a//b",
            "py:.//a/b",
            "clj:.//",
            "clj:./a",
            "clj:.//a/",
            "clj:.//a/b@",
            "clj:.//a/b%41",
            "clj:a///b",
            "ext:clj//a",
            "ext:clj//a/x@y",
            "ext:java//a/b",
            "lib:npm",
            "lib::react",
            "lib:npm:",
            "java:",
            "java:#m",
            "java:A#",
            "js:",
            "hole:nope@a.clj:1:1",
            "hole:interop@a.clj:1",
            "hole:interop@a.clj:x:1",
            "hole:interop@:1:1",
            "host:",
        ] {
            assert!(s.parse::<NodeId>().is_err(), "{s:?} should be rejected");
        }
    }

    #[test]
    fn ids_serialize_as_strings() {
        let id: NodeId = "ext:clj//clojure.core/map".parse().unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"ext:clj//clojure.core/map\"");
        assert_eq!(serde_json::from_str::<NodeId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<NodeId>("\"bogus\"").is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2000))]

        #[test]
        fn every_id_formats_and_parses_back(id in arb_node_id()) {
            let text = id.to_string();
            let parsed: NodeId = text.parse().map_err(|e| TestCaseError::fail(format!("{text}: {e}")))?;
            prop_assert_eq!(&parsed, &id);
            prop_assert_eq!(parsed.to_string(), text);
        }
    }
}
