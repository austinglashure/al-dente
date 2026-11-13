//! `rules/trace-noise.toml`: which stack frames are runtime machinery.

use serde::Deserialize;

use crate::RulesError;

const NOISE: &str = include_str!("../../../rules/trace-noise.toml");

/// `rules/trace-noise.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Noise {
    pub exact: Vec<String>,
    pub prefixes: Vec<String>,
}

impl Noise {
    /// Whether a frame's namespace or class is noise.
    ///
    /// Example: `is_noise("clojure.lang.Numbers")` is `true`;
    /// `is_noise("clojure.core.async")` is `false` (only `clojure.core` exactly).
    pub fn is_noise(&self, namespace_or_class: &str) -> bool {
        self.exact.iter().any(|e| e == namespace_or_class)
            || self
                .prefixes
                .iter()
                .any(|p| namespace_or_class.starts_with(p.as_str()))
    }
}

/// The built-in noise catalog.
///
/// Example: `noise()?.is_noise("java.util.concurrent.FutureTask")` is `true`.
pub fn noise() -> Result<Noise, RulesError> {
    toml::from_str(NOISE).map_err(|e| RulesError {
        name: "trace-noise",
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_parses_and_matches() {
        let n = noise().unwrap();
        assert!(n.is_noise("clojure.lang.Numbers"));
        assert!(n.is_noise("clojure.core"));
        assert!(n.is_noise("java.base/java.lang.Thread") || n.is_noise("java.lang.Thread"));
        assert!(n.is_noise("nrepl.middleware.session"));
        assert!(!n.is_noise("clojure.core.async"));
        assert!(!n.is_noise("shop.core"));
        assert!(!n.is_noise("user"));
    }
}
