//! `rules/clj-keyword-dispatch.toml`: keyword-keyed dispatch (R11).

use serde::Deserialize;

use crate::RulesError;

const SOURCE: &str = include_str!("../../../rules/clj-keyword-dispatch.toml");

/// The whole catalog.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default, rename = "family")]
    pub families: Vec<Family>,
    #[serde(default, rename = "registration")]
    pub registrations: Vec<Registration>,
    #[serde(default, rename = "dispatch")]
    pub dispatches: Vec<Dispatch>,
    #[serde(default, rename = "effect")]
    pub effects: Vec<Effect>,
    #[serde(default, rename = "effect_map")]
    pub effect_maps: Vec<EffectMap>,
    #[serde(default, rename = "alias")]
    pub aliases: Vec<Alias>,
}

/// Inside a handler of family `inside`, a map key (or an `:fx`-style
/// `[key value]` pair) that names a handler of `family` dispatches to it.
/// Keys that name nothing are plain data: no blind spot.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectMap {
    pub inside: String,
    pub family: String,
}

/// A namespace whose functions mean the same as another's for this catalog.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub ns: String,
    pub same_as: String,
}

/// A set of handlers keyed by keyword, and the edge a dispatch to one gets.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub name: String,
    pub edge: EdgeKind,
    pub certainty: Certainty,
    /// Qualified multimethods whose `defmethod`s are handlers too.
    #[serde(default)]
    pub multimethods: Vec<String>,
}

/// The edge kinds a family may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeKind {
    Calls,
    References,
}

/// The certainties a family may use (never certain: the key is a runtime value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Certainty {
    Likely,
    Possible,
}

/// A function that registers a handler under its first argument.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    #[serde(rename = "fn")]
    pub function: String,
    pub family: String,
    /// An argument marker after which a vector `[:k …]` dispatches into the
    /// same family (re-frame's `:<-` signals).
    pub signal_marker: Option<String>,
}

/// A function whose first argument names a handler.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dispatch {
    #[serde(rename = "fn")]
    pub function: String,
    pub family: String,
    pub arg: Arg,
}

/// How a dispatch names its handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arg {
    /// `[:k …]`.
    Vector,
    /// `:k`.
    Keyword,
}

/// A map entry that dispatches, such as re-frame's `:dispatch` effect.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub key: String,
    pub family: String,
    pub value: EffectValue,
}

/// The shape of an effect's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectValue {
    /// `[:k …]`.
    Event,
    /// `[[:k …] [:j …]]`.
    Events,
    /// `[[:dispatch [:k]] …]`: pairs read as entries of the effect table.
    Effects,
}

/// The catalog compiled into the binary.
///
/// Example: `builtin()?.registration("re-frame.core", "reg-sub")` is the
/// `re-frame/sub` registration.
pub fn builtin() -> Result<Catalog, RulesError> {
    parse(SOURCE)
}

/// Parses a catalog and checks that every family it names exists.
///
/// Example: `parse("")` gives an empty catalog; a dispatch naming an
/// undefined family is an error.
pub fn parse(text: &str) -> Result<Catalog, RulesError> {
    let err = |message: String| RulesError {
        name: "clj-keyword-dispatch",
        message,
    };
    let c: Catalog = toml::from_str(text).map_err(|e| err(e.to_string()))?;
    let named = c
        .registrations
        .iter()
        .map(|r| &r.family)
        .chain(c.dispatches.iter().map(|d| &d.family))
        .chain(c.effects.iter().map(|e| &e.family))
        .chain(c.effect_maps.iter().flat_map(|m| [&m.inside, &m.family]));
    for f in named {
        if c.family(f).is_none() {
            return Err(err(format!("unknown family {f:?}")));
        }
    }
    Ok(c)
}

/// Splits `ns/name`. Example: `split("a.b/c")` is `Some(("a.b", "c"))`.
fn split(q: &str) -> Option<(&str, &str)> {
    q.split_once('/')
}

impl Catalog {
    /// The family called `name`.
    ///
    /// Example: `c.family("re-frame/sub")`.
    pub fn family(&self, name: &str) -> Option<&Family> {
        self.families.iter().find(|f| f.name == name)
    }

    /// The registration function `ns/name`, if it is one.
    ///
    /// Example: `c.registration("re-frame.core", "reg-event-db")`.
    pub fn registration(&self, ns: &str, name: &str) -> Option<&Registration> {
        let ns = self.canonical(ns);
        self.registrations
            .iter()
            .find(|r| split(&r.function) == Some((ns, name)))
    }

    /// The namespace `ns` stands for (itself unless an alias says otherwise).
    ///
    /// Example: `c.canonical("re-frame.alpha")` is `"re-frame.core"`.
    pub fn canonical<'s>(&'s self, ns: &'s str) -> &'s str {
        self.aliases
            .iter()
            .find(|a| a.ns == ns)
            .map_or(ns, |a| a.same_as.as_str())
    }

    /// The effect maps that apply inside handlers of `family`.
    ///
    /// Example: `c.effect_maps_inside("re-frame/event")` gives the `re-frame/fx` one.
    pub fn effect_maps_inside<'s>(
        &'s self,
        family: &'s str,
    ) -> impl Iterator<Item = &'s EffectMap> {
        self.effect_maps.iter().filter(move |m| m.inside == family)
    }

    /// The dispatch function `ns/name`, if it is one.
    ///
    /// Example: `c.dispatch("re-frame.core", "dispatch")`.
    pub fn dispatch(&self, ns: &str, name: &str) -> Option<&Dispatch> {
        let ns = self.canonical(ns);
        self.dispatches
            .iter()
            .find(|d| split(&d.function) == Some((ns, name)))
    }

    /// The effect keyed `key` (as written, with the colon).
    ///
    /// Example: `c.effect(":dispatch")`.
    pub fn effect(&self, key: &str) -> Option<&Effect> {
        self.effects.iter().find(|e| e.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_has_re_frame_and_integrant() {
        let c = builtin().unwrap();
        let reg = c.registration("re-frame.core", "reg-event-fx").unwrap();
        assert_eq!(reg.family, "re-frame/event");
        let d = c.dispatch("re-frame.core", "subscribe").unwrap();
        assert_eq!((d.family.as_str(), d.arg), ("re-frame/sub", Arg::Vector));
        let r = c.dispatch("integrant.core", "ref").unwrap();
        assert_eq!(r.arg, Arg::Keyword);
        let ig = c.family("integrant/key").unwrap();
        assert!(
            ig.multimethods
                .iter()
                .any(|m| m == "integrant.core/init-key")
        );
        assert_eq!(
            (ig.edge, ig.certainty),
            (EdgeKind::References, Certainty::Possible)
        );
        let fx = c.effect(":fx").unwrap();
        assert_eq!(fx.value, EffectValue::Effects);
        assert!(c.registration("re-frame.core", "dispatch").is_none());
        assert!(c.dispatch("re-frame.alpha", "dispatch").is_some());
        assert!(
            c.registration("re-frame.core-instrumented", "reg-event-db")
                .is_some()
        );
        let m: Vec<&str> = c
            .effect_maps_inside("re-frame/event")
            .map(|m| m.family.as_str())
            .collect();
        assert_eq!(m, ["re-frame/fx"]);
    }

    #[test]
    fn unknown_family_is_rejected() {
        let e =
            parse("[[dispatch]]\nfn = \"a/b\"\nfamily = \"nope\"\narg = \"vector\"").unwrap_err();
        assert!(e.message.contains("nope"), "{e}");
        assert!(parse("").unwrap().families.is_empty());
    }
}
