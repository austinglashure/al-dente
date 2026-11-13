//! `rules/hotspots.toml`: thresholds for hotspot findings and the risk
//! score's weights.

use serde::{Deserialize, Serialize};

use crate::RulesError;

const BUILTIN: &str = include_str!("../../../rules/hotspots.toml");

/// `[exclude]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Exclude {
    /// Path-segment patterns; `*` matches any run of characters.
    pub segments: Vec<String>,
}

/// `[god-object]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct GodObject {
    pub percentile: f64,
}

/// `[must-be-bulletproof]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bulletproof {
    pub pagerank_percentile: f64,
    pub dominator_share: f64,
}

/// `[bottleneck]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Bottleneck {
    pub betweenness_percentile: f64,
    pub articulation: bool,
}

/// `[cycle]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Cycle {
    pub symbol_min_size: u32,
}

/// `[hidden-coupling]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct HiddenCoupling {
    pub min_support: u32,
    pub min_confidence: f64,
}

/// `[risky-untested]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct RiskyUntested {
    pub risk_percentile: f64,
    pub statuses: Vec<String>,
    pub skip_entry_points: bool,
}

/// `[misplaced-code]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Misplaced {
    pub min_community: u32,
    pub min_share: f64,
}

/// `[dead-code]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DeadCode {
    pub enabled: bool,
}

/// `[risk]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Risk {
    pub pagerank: f64,
    pub fan_in: f64,
    pub dominator: f64,
    pub untested: f64,
    pub weak: f64,
    pub indirect: f64,
    pub direct: f64,
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct HotspotRules {
    pub exclude: Exclude,
    pub god_object: GodObject,
    pub must_be_bulletproof: Bulletproof,
    pub bottleneck: Bottleneck,
    pub cycle: Cycle,
    pub hidden_coupling: HiddenCoupling,
    pub risky_untested: RiskyUntested,
    pub misplaced_code: Misplaced,
    pub dead_code: DeadCode,
    pub risk: Risk,
}

impl HotspotRules {
    /// Parses a hotspot rules file (the user's copy, or the built-in one).
    ///
    /// Example: `HotspotRules::parse(&std::fs::read_to_string(path)?)`.
    pub fn parse(text: &str) -> Result<HotspotRules, RulesError> {
        toml::from_str(text).map_err(|e| RulesError {
            name: "hotspots",
            message: e.to_string(),
        })
    }

    /// The built-in rules.
    ///
    /// Example: `HotspotRules::builtin()?.god_object.percentile` is `99.0`.
    pub fn builtin() -> Result<HotspotRules, RulesError> {
        HotspotRules::parse(BUILTIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_parses_and_unknown_keys_are_refused() {
        let r = HotspotRules::builtin().unwrap();
        assert_eq!(r.god_object.percentile, 99.0);
        assert_eq!(r.risk.untested, 1.0);
        let bad = BUILTIN.replace("[dead-code]\nenabled = true", "[dead-code]\nenabld = true");
        assert!(HotspotRules::parse(&bad).is_err());
    }
}
