//! `aldente schema [name]`: prints JSON Schemas generated from the model.

use aldente_model::schema::{SCHEMA_NAMES, schema_by_name};
use aldente_model::{Envelope, ErrorCode, ExitCode};
use clap::Args;

use crate::output::{print_json, print_lines};

/// Arguments for `aldente schema`.
#[derive(Debug, Args)]
pub struct SchemaArgs {
    /// Schema to print. Omit to list the available names.
    name: Option<String>,
}

/// The envelope `schema` tag for this command's output.
const TAG: &str = "schema";

/// Prints the named schema, or the list of names when none is given.
///
/// Example: `aldente schema envelope` prints the envelope's JSON Schema and
/// returns `ExitCode::Success`; `aldente schema nope` returns `ExitCode::Usage`.
pub fn run(args: &SchemaArgs, json: bool) -> anyhow::Result<ExitCode> {
    let Some(name) = &args.name else {
        if json {
            print_json(&Envelope::success(TAG, None, SCHEMA_NAMES))?;
        } else {
            print_lines(SCHEMA_NAMES.iter().copied())?;
        }
        return Ok(ExitCode::Success);
    };
    match schema_by_name(name) {
        Some(schema) if json => print_json(&Envelope::success(TAG, None, schema))?,
        Some(schema) => print_json(&schema)?,
        None => return unknown(name, json),
    }
    Ok(ExitCode::Success)
}

/// Reports an unknown schema name as a usage error.
///
/// Example: `unknown("nope", false)` prints
/// `aldente: unknown schema "nope" (known: envelope)` on stderr.
fn unknown(name: &str, json: bool) -> anyhow::Result<ExitCode> {
    let message = format!(
        "unknown schema {name:?} (known: {})",
        SCHEMA_NAMES.join(", ")
    );
    if json {
        print_json(&Envelope::<()>::failure(TAG, ErrorCode::Usage, &message))?;
    } else {
        eprintln!("aldente: {message}");
    }
    Ok(ErrorCode::Usage.exit_code())
}
