//! `rules/clj-namespaces.toml`, `rules/clj-tests.toml` and
//! `rules/clj-entry-points.toml`: library attribution (R10), tests (R14)
//! and server-start entry points (R13).

use serde::Deserialize;

use crate::RulesError;

const NAMESPACES: &str = include_str!("../../../rules/clj-namespaces.toml");
const TESTS: &str = include_str!("../../../rules/clj-tests.toml");
const ENTRY_POINTS: &str = include_str!("../../../rules/clj-entry-points.toml");

/// `rules/clj-namespaces.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Namespaces {
    #[serde(default)]
    pub core: Vec<PrefixRule>,
    #[serde(default)]
    pub prefix: Vec<PrefixRule>,
}

/// A namespace prefix and the artifact that provides it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefixRule {
    pub prefix: String,
    pub coord: String,
}

/// `rules/clj-tests.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tests {
    pub namespace_names: Vec<String>,
    pub namespace_suffixes: Vec<String>,
    pub definers: Vec<String>,
    pub top_level: Vec<String>,
}

/// `rules/clj-entry-points.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryPoints {
    #[serde(default)]
    pub server_start: Vec<ServerStart>,
}

/// A function whose first argument is a ring handler.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerStart {
    #[serde(rename = "fn")]
    pub function: String,
}

/// Parses a built-in catalog.
///
/// Example: `load::<Tests>("clj-tests", TESTS)`.
fn load<T: serde::de::DeserializeOwned>(name: &'static str, text: &str) -> Result<T, RulesError> {
    toml::from_str(text).map_err(|e| RulesError {
        name,
        message: e.to_string(),
    })
}

/// The built-in namespace catalog.
///
/// Example: `namespaces()?.core_for("clojure.set")` is `Some("org.clojure/clojure")`.
pub fn namespaces() -> Result<Namespaces, RulesError> {
    load("clj-namespaces", NAMESPACES)
}

/// The built-in test catalog.
///
/// Example: `tests()?.is_test_namespace("a.b-test")` is `true`.
pub fn tests() -> Result<Tests, RulesError> {
    load("clj-tests", TESTS)
}

/// The built-in entry-point catalog.
///
/// Example: `entry_points()?.is_server_start("org.httpkit.server", "run-server")` is `true`.
pub fn entry_points() -> Result<EntryPoints, RulesError> {
    load("clj-entry-points", ENTRY_POINTS)
}

/// Whether `prefix` is `ns` or a leading run of its dot-separated parts.
///
/// Example: `prefix_matches("a.b.c", "a.b")` is `true`; `("a.bc", "a.b")` is `false`.
pub fn prefix_matches(ns: &str, prefix: &str) -> bool {
    ns == prefix
        || ns
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// The longest rule whose prefix matches `ns`.
///
/// Example: rules `clojure` and `clojure.core.async` pick the second for
/// `clojure.core.async.impl`.
fn longest<'r>(rules: &'r [PrefixRule], ns: &str) -> Option<&'r str> {
    rules
        .iter()
        .filter(|r| prefix_matches(ns, &r.prefix))
        .max_by_key(|r| r.prefix.len())
        .map(|r| r.coord.as_str())
}

impl Namespaces {
    /// Rule 4: the core artifact for a language namespace.
    ///
    /// Example: `core_for("cljs.reader")` is `Some("org.clojure/clojurescript")`.
    pub fn core_for(&self, ns: &str) -> Option<&str> {
        longest(&self.core, ns)
    }

    /// Rule 3's table: the artifact for a known prefix.
    ///
    /// Example: `table_for("taoensso.timbre.appenders")` is `Some("com.taoensso/timbre")`.
    pub fn table_for(&self, ns: &str) -> Option<&str> {
        longest(&self.prefix, ns)
    }
}

impl Tests {
    /// Whether a namespace name marks it as a test namespace.
    ///
    /// Example: `is_test_namespace("shop.pricing-test")` is `true`.
    pub fn is_test_namespace(&self, ns: &str) -> bool {
        let last = ns.rsplit('.').next().unwrap_or(ns);
        self.namespace_names.iter().any(|n| n == last)
            || self
                .namespace_suffixes
                .iter()
                .any(|s| last.ends_with(s.as_str()))
    }
}

impl EntryPoints {
    /// Whether `ns/name` starts a server with a handler argument.
    ///
    /// Example: `is_server_start("aleph.http", "start-server")` is `true`.
    pub fn is_server_start(&self, ns: &str, name: &str) -> bool {
        self.server_start
            .iter()
            .any(|s| s.function.split_once('/') == Some((ns, name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_catalog() {
        let c = namespaces().unwrap();
        assert_eq!(c.core_for("clojure.string"), Some("org.clojure/clojure"));
        assert_eq!(c.core_for("cljs.core"), Some("org.clojure/clojurescript"));
        assert_eq!(c.core_for("clojurex.y"), None, "whole components only");
        assert_eq!(
            c.table_for("clojure.core.async.impl"),
            Some("org.clojure/core.async")
        );
        assert_eq!(c.table_for("clojure.string"), None);
    }

    #[test]
    fn tests_and_entry_points_catalogs() {
        let t = tests().unwrap();
        assert!(t.is_test_namespace("shop.pricing-test"));
        assert!(t.is_test_namespace("shop.test"));
        assert!(!t.is_test_namespace("shop.testing"));
        assert!(!t.is_test_namespace("shop.latest"));
        assert!(t.definers.iter().any(|d| d == "defspec"));
        assert!(t.top_level.iter().any(|d| d == "facts"));
        let e = entry_points().unwrap();
        assert!(e.is_server_start("ring.adapter.jetty", "run-jetty"));
        assert!(!e.is_server_start("ring.adapter.jetty", "stop"));
    }

    #[test]
    fn prefix_match_takes_whole_components() {
        assert!(prefix_matches("a.b", "a.b"));
        assert!(prefix_matches("a.b.c", "a.b"));
        assert!(!prefix_matches("a.bc", "a.b"));
    }
}
