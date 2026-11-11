//! JSON Schemas generated from the model types, as printed by
//! `aldente schema <name>`.

use schemars::Schema;
use serde_json::Value;

use crate::envelope::Envelope;

/// Names `schema_by_name` understands, sorted.
pub const SCHEMA_NAMES: &[&str] = &["envelope"];

/// The JSON Schema (draft 2020-12) for the envelope with `data` of any shape.
///
/// Example: `envelope_schema()["required"]` lists `schema`, `ok`, `data`, ….
pub fn envelope_schema() -> Value {
    schemars::schema_for!(Envelope<Value>).to_value()
}

/// Marks every property as required while leaving nullable types nullable.
/// Used on output types whose keys are always present, even when null.
///
/// Example: `{ "properties": { "a": {}, "b": {} } }` gains
/// `"required": ["a", "b"]`.
pub(crate) fn all_keys_required(schema: &mut Schema) {
    let keys: Vec<Value> = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|props| props.keys().cloned().map(Value::String).collect())
        .unwrap_or_default();
    schema.insert("required".to_owned(), Value::Array(keys));
}

/// The schema called `name`, or `None` if there is no such schema.
///
/// Example: `schema_by_name("envelope")` is `Some(..)`; `schema_by_name("x")` is `None`.
pub fn schema_by_name(name: &str) -> Option<Value> {
    match name {
        "envelope" => Some(envelope_schema()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{ErrorCode, SnapshotInfo, Warning, WarningCode};
    use crate::graph::GraphPayload;
    use crate::strategies::arb_payload;
    use proptest::prelude::*;

    #[test]
    fn envelope_schema_is_valid_json_schema() {
        let schema = envelope_schema();
        assert!(jsonschema::meta::is_valid(&schema), "{schema:#}");
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
    }

    #[test]
    fn every_name_resolves() {
        for name in SCHEMA_NAMES {
            assert!(schema_by_name(name).is_some(), "{name}");
        }
        assert!(schema_by_name("nope").is_none());
    }

    #[test]
    fn null_keys_stay_nullable_and_required() {
        let schema = envelope_schema();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let minimal = serde_json::json!({
            "schema": "aldente/x/v1", "ok": true, "snapshot": null, "data": null,
            "warnings": [], "error": null
        });
        assert!(validator.is_valid(&minimal));
    }

    #[test]
    fn schema_rejects_envelopes_missing_keys() {
        let validator = jsonschema::validator_for(&envelope_schema()).unwrap();
        let missing_error = serde_json::json!({
            "schema": "aldente/x/v1", "ok": true, "snapshot": null, "data": {}, "warnings": []
        });
        assert!(!validator.is_valid(&missing_error));
    }

    fn arb_envelope() -> impl Strategy<Value = Envelope<GraphPayload>> {
        (
            arb_payload(),
            any::<bool>(),
            proptest::option::of((
                "\\PC{0,8}",
                proptest::option::of("[0-9a-f]{7}"),
                any::<bool>(),
            )),
            prop::sample::select(ErrorCode::ALL),
            "\\PC{0,12}",
        )
            .prop_map(|(payload, ok, snap, code, message)| {
                let mut env = if ok {
                    Envelope::success("graph", None, payload)
                } else {
                    Envelope::failure("graph", code, &message)
                };
                env.snapshot = snap.map(|(repo, rev, dirty)| SnapshotInfo {
                    key: "b3:00".into(),
                    repo,
                    rev,
                    dirty,
                    analyzer: "0.1.0".into(),
                });
                env.warnings.push(Warning {
                    code: WarningCode::Truncated,
                    message,
                });
                env
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]

        #[test]
        fn envelope_round_trips_and_matches_its_schema(env in arb_envelope()) {
            let value = serde_json::to_value(&env).unwrap();
            let back: Envelope<GraphPayload> = serde_json::from_value(value.clone()).unwrap();
            prop_assert_eq!(&back, &env);
            let validator = jsonschema::validator_for(&envelope_schema()).unwrap();
            prop_assert!(validator.is_valid(&value), "{}", value);
        }
    }
}
