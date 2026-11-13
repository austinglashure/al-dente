//! The JSON envelope every `--json` command and API response uses, plus
//! error and exit codes. See `docs/plan/05-cli.md`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Process exit codes. The numbers are part of the CLI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    Internal = 1,
    Usage = 2,
    /// Ambiguous or unknown symbol.
    Symbol = 3,
    /// Findings at or above `--fail-on`.
    Findings = 4,
    ToolMissing = 5,
    ExecRefused = 6,
}

impl ExitCode {
    /// The numeric process exit code.
    ///
    /// Example: `ExitCode::ToolMissing.code()` is `5`.
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// The machine-readable kind of a failed command.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    Internal,
    Usage,
    AmbiguousSymbol,
    UnknownSymbol,
    ToolMissing,
    ExecRefused,
}

impl ErrorCode {
    /// Every error code, in declaration order.
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::Internal,
        ErrorCode::Usage,
        ErrorCode::AmbiguousSymbol,
        ErrorCode::UnknownSymbol,
        ErrorCode::ToolMissing,
        ErrorCode::ExecRefused,
    ];

    /// The exit code a command failing this way returns.
    ///
    /// Example: `ErrorCode::AmbiguousSymbol.exit_code()` is `ExitCode::Symbol` (3).
    pub fn exit_code(self) -> ExitCode {
        match self {
            ErrorCode::Internal => ExitCode::Internal,
            ErrorCode::Usage => ExitCode::Usage,
            ErrorCode::AmbiguousSymbol | ErrorCode::UnknownSymbol => ExitCode::Symbol,
            ErrorCode::ToolMissing => ExitCode::ToolMissing,
            ErrorCode::ExecRefused => ExitCode::ExecRefused,
        }
    }
}

/// The `error` field of a failed envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// Full IDs to choose from, for ambiguous symbols.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
}

/// Machine-readable warning kinds. Grows as commands need them.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum WarningCode {
    Truncated,
    /// The result is good but the snapshot could not be saved to the cache.
    NotCached,
    /// The graph is shown at a coarser level than asked, to fit the payload cap.
    Coarser,
    /// `crater --diff`: no snapshot of the base revision is cached, so the
    /// callers of removed definitions are unknown.
    NoBaseSnapshot,
}

/// A non-fatal note attached to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    pub code: WarningCode,
    pub message: String,
}

/// Which snapshot a result came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct SnapshotInfo {
    pub key: String,
    pub repo: String,
    pub rev: Option<String>,
    pub dirty: bool,
    pub analyzer: String,
}

/// The envelope around every JSON result. All keys are always present;
/// `snapshot` is null for commands that do not read one (e.g. `schema`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct Envelope<T> {
    /// `aldente/<name>/v<version>`, e.g. `aldente/crater/v1`.
    pub schema: String,
    pub ok: bool,
    pub snapshot: Option<SnapshotInfo>,
    pub data: Option<T>,
    pub warnings: Vec<Warning>,
    pub error: Option<ErrorBody>,
}

/// The `schema` value for a data shape at version 1.
///
/// Example: `schema_tag("crater")` is `"aldente/crater/v1"`.
pub fn schema_tag(name: &str) -> String {
    format!("aldente/{name}/v1")
}

impl<T> Envelope<T> {
    /// A successful result.
    ///
    /// Example: `Envelope::success("schema", None, data)` has `ok: true`,
    /// `schema: "aldente/schema/v1"` and `error: null`.
    pub fn success(name: &str, snapshot: Option<SnapshotInfo>, data: T) -> Envelope<T> {
        Envelope {
            schema: schema_tag(name),
            ok: true,
            snapshot,
            data: Some(data),
            warnings: Vec::new(),
            error: None,
        }
    }

    /// A failed result.
    ///
    /// Example: `Envelope::<()>::failure("schema", ErrorCode::Usage, "unknown schema")`
    /// has `ok: false`, `data: null` and `error.code: "usage"`.
    pub fn failure(name: &str, code: ErrorCode, message: &str) -> Envelope<T> {
        Envelope {
            schema: schema_tag(name),
            ok: false,
            snapshot: None,
            data: None,
            warnings: Vec::new(),
            error: Some(ErrorBody {
                code,
                message: message.to_owned(),
                candidates: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exit_codes_match_the_cli_contract() {
        let codes = [
            ExitCode::Success,
            ExitCode::Internal,
            ExitCode::Usage,
            ExitCode::Symbol,
            ExitCode::Findings,
            ExitCode::ToolMissing,
            ExitCode::ExecRefused,
        ];
        let numbers: Vec<u8> = codes.iter().map(|c| c.code()).collect();
        assert_eq!(numbers, vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn error_codes_map_to_exit_codes() {
        assert_eq!(ErrorCode::Usage.exit_code().code(), 2);
        assert_eq!(ErrorCode::UnknownSymbol.exit_code().code(), 3);
        assert_eq!(ErrorCode::ToolMissing.exit_code().code(), 5);
        assert_eq!(ErrorCode::ExecRefused.exit_code().code(), 6);
    }

    #[test]
    fn failure_has_the_plan_shape() {
        let env = Envelope::<()>::failure("show", ErrorCode::AmbiguousSymbol, "2 matches");
        assert_eq!(
            serde_json::to_value(&env).unwrap(),
            json!({
                "schema": "aldente/show/v1", "ok": false, "snapshot": null, "data": null,
                "warnings": [], "error": { "code": "ambiguous-symbol", "message": "2 matches" }
            })
        );
    }
}
