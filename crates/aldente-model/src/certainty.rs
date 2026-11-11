//! How sure the analyzer is about an edge, and why it drew it.
//! See "Certainty and reason" in `docs/plan/03-graph-model.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Confidence in an edge. Ordered: `Possible < Likely < Certain`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Certainty {
    Possible,
    Likely,
    Certain,
}

impl Certainty {
    /// Every level, weakest first.
    pub const ALL: &'static [Certainty] =
        &[Certainty::Possible, Certainty::Likely, Certainty::Certain];

    /// The default weight of an edge at this level in graph measures.
    ///
    /// Example: `Certainty::Likely.default_weight()` is `0.6`.
    pub fn default_weight(self) -> f64 {
        match self {
            Certainty::Certain => 1.0,
            Certainty::Likely => 0.6,
            Certainty::Possible => 0.25,
        }
    }
}

/// The machine-readable reason an edge exists.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Direct,
    ValueRef,
    MultimethodDispatch,
    ProtocolDispatch,
    ParamFlow,
    KeywordDispatch,
    QuotedResolve,
    NameMatch,
    Interop,
    TraceObserved,
    /// A macro to a var named in its syntax-quoted body; always `Possible`.
    MacroBody,
    /// A call into a `hole`: the call surely happens, the target is unknown.
    Dynamic,
}

impl Reason {
    /// Every reason, in declaration order.
    pub const ALL: &'static [Reason] = &[
        Reason::Direct,
        Reason::ValueRef,
        Reason::MultimethodDispatch,
        Reason::ProtocolDispatch,
        Reason::ParamFlow,
        Reason::KeywordDispatch,
        Reason::QuotedResolve,
        Reason::NameMatch,
        Reason::Interop,
        Reason::TraceObserved,
        Reason::MacroBody,
        Reason::Dynamic,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn levels_are_ordered_weakest_first() {
        assert!(Certainty::Possible < Certainty::Likely);
        assert!(Certainty::Likely < Certainty::Certain);
        assert_eq!(
            [Certainty::Likely, Certainty::Certain, Certainty::Possible]
                .into_iter()
                .max(),
            Some(Certainty::Certain)
        );
    }

    #[test]
    fn reasons_serialize_in_kebab_case() {
        assert_eq!(
            serde_json::to_string(&Reason::MultimethodDispatch).unwrap(),
            "\"multimethod-dispatch\""
        );
        assert_eq!(
            serde_json::to_string(&Reason::ValueRef).unwrap(),
            "\"value-ref\""
        );
    }

    #[test]
    fn macro_body_and_dynamic_reasons_exist() {
        for (reason, json) in [
            (Reason::MacroBody, "\"macro-body\""),
            (Reason::Dynamic, "\"dynamic\""),
        ] {
            assert_eq!(serde_json::to_string(&reason).unwrap(), json);
            assert!(Reason::ALL.contains(&reason));
        }
    }

    proptest! {
        #[test]
        fn ordering_agrees_with_default_weight(
            a in prop::sample::select(Certainty::ALL),
            b in prop::sample::select(Certainty::ALL),
        ) {
            prop_assert_eq!(
                a.cmp(&b),
                a.default_weight().total_cmp(&b.default_weight())
            );
        }
    }
}
