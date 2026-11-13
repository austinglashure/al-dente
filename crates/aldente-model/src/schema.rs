//! JSON Schemas generated from the model types, as printed by
//! `aldente schema <name>`.

use schemars::Schema;
use serde_json::Value;

use crate::analyze::{AnalyzeSummary, ServeInfo, SnapshotOverview};
use crate::cache::{CacheRemoval, CacheReport};
use crate::crater::{Checklist, CraterResult};
use crate::doctor::DoctorReport;
use crate::envelope::Envelope;
use crate::graph::GraphPayload;
use crate::query::{BlindspotReport, FindResult, ShowResult};
use crate::units::UnitsReport;

/// Names `schema_by_name` understands, sorted.
pub const SCHEMA_NAMES: &[&str] = &[
    "analyze",
    "blindspots",
    "cache",
    "cache-removal",
    "checklist",
    "crater",
    "doctor",
    "envelope",
    "find",
    "graph",
    "serve",
    "show",
    "snapshot",
    "units",
];

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
/// The `data` shapes are schemas of `data` alone, not of the whole envelope.
///
/// Example: `schema_by_name("envelope")` is `Some(..)`; `schema_by_name("x")` is `None`.
pub fn schema_by_name(name: &str) -> Option<Value> {
    match name {
        "analyze" => Some(schemars::schema_for!(AnalyzeSummary).to_value()),
        "blindspots" => Some(schemars::schema_for!(BlindspotReport).to_value()),
        "find" => Some(schemars::schema_for!(FindResult).to_value()),
        "graph" => Some(schemars::schema_for!(GraphPayload).to_value()),
        "serve" => Some(schemars::schema_for!(ServeInfo).to_value()),
        "show" => Some(schemars::schema_for!(ShowResult).to_value()),
        "snapshot" => Some(schemars::schema_for!(SnapshotOverview).to_value()),
        "cache" => Some(schemars::schema_for!(CacheReport).to_value()),
        "cache-removal" => Some(schemars::schema_for!(CacheRemoval).to_value()),
        "doctor" => Some(schemars::schema_for!(DoctorReport).to_value()),
        "crater" => Some(schemars::schema_for!(CraterResult).to_value()),
        "checklist" => Some(schemars::schema_for!(Checklist).to_value()),
        "envelope" => Some(envelope_schema()),
        "units" => Some(schemars::schema_for!(UnitsReport).to_value()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::{CacheCheck, DoctorReport, Platform, ToolCheck, ToolStatus};
    use crate::envelope::{ErrorCode, SnapshotInfo, Warning, WarningCode};
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

    fn sample_report() -> DoctorReport {
        DoctorReport {
            tools: vec![ToolCheck {
                name: "git".into(),
                status: ToolStatus::TooOld,
                path: Some("/usr/bin/git".into()),
                version: Some("2.20.0".into()),
                minimum: "2.30.0".into(),
                detail: None,
                hint: Some("upgrade git".into()),
            }],
            cache: CacheCheck {
                path: None,
                writable: false,
                detail: Some("no cache directory".into()),
            },
            platform: Platform {
                os: "linux".into(),
                arch: "x86_64".into(),
            },
        }
    }

    #[test]
    fn doctor_schema_is_valid_and_matches_a_report() {
        let schema = schema_by_name("doctor").unwrap();
        assert!(jsonschema::meta::is_valid(&schema), "{schema:#}");
        let validator = jsonschema::validator_for(&schema).unwrap();
        let value = serde_json::to_value(sample_report()).unwrap();
        assert!(validator.is_valid(&value), "{value:#}");
        assert_eq!(value["tools"][0]["status"], "too-old");

        let mut missing_hint = value.clone();
        missing_hint["tools"][0]
            .as_object_mut()
            .unwrap()
            .remove("hint");
        assert!(!validator.is_valid(&missing_hint));
    }

    #[test]
    fn all_ok_needs_every_tool_ok_and_a_writable_cache() {
        let mut report = sample_report();
        assert!(!report.all_ok());
        report.tools[0].status = ToolStatus::Ok;
        assert!(!report.all_ok());
        report.cache.writable = true;
        assert!(report.all_ok());
    }

    #[test]
    fn units_schema_is_valid_and_matches_a_report() {
        use crate::diagnostic::{Diagnostic, DiagnosticCode};
        use crate::units::{RootKind, SourceRoot, UnitInfo, UnitsReport};
        use crate::{Lang, NodeId, RepoPath};
        let report = UnitsReport {
            git: false,
            units: vec![UnitInfo {
                id: NodeId::Unit(RepoPath::root()),
                dir: RepoPath::root(),
                manifests: vec!["deps.edn".into()],
                langs: [Lang::Clj].into(),
                roots: vec![SourceRoot {
                    path: RepoPath::parse("test").unwrap(),
                    kind: RootKind::Test,
                    manifest: Some("deps.edn".into()),
                    alias: Some("test".into()),
                }],
                service: false,
                entry_points: Vec::new(),
                deps: Vec::new(),
                files: 1,
            }],
            diagnostics: vec![Diagnostic::new(None, DiagnosticCode::LimitReached, "cap")],
        };
        let schema = schema_by_name("units").unwrap();
        assert!(jsonschema::meta::is_valid(&schema), "{schema:#}");
        let value = serde_json::to_value(&report).unwrap();
        assert!(
            jsonschema::validator_for(&schema).unwrap().is_valid(&value),
            "{value:#}"
        );
        assert_eq!(value["units"][0]["id"], "unit:.");
        assert_eq!(value["units"][0]["roots"][0]["kind"], "test");
        assert_eq!(value["diagnostics"][0]["code"], "limit-reached");
        let back: UnitsReport = serde_json::from_value(value).unwrap();
        assert_eq!(back, report);
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
