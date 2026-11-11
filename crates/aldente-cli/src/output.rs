//! Writing results to stdout.

use std::io::Write;

use serde::Serialize;

/// Writes `value` to stdout as pretty JSON followed by a newline.
///
/// Example: `print_json(&json!({"ok": true}))` prints `{\n  "ok": true\n}\n`.
pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}

/// Writes each line to stdout.
///
/// Example: `print_lines(["a", "b"])` prints `a\nb\n`.
pub fn print_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    for line in lines {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

/// Whether `err` means the reader of stdout went away (e.g. `aldente … | head`).
/// That is not a failure of the command, so `main` exits quietly.
///
/// Example: an `io::Error` of kind `BrokenPipe` gives `true`; any other error `false`.
pub fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        let kind = cause
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind)
            .or_else(|| {
                cause
                    .downcast_ref::<serde_json::Error>()
                    .and_then(serde_json::Error::io_error_kind)
            });
        kind == Some(std::io::ErrorKind::BrokenPipe)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn broken_pipe_is_recognized() {
        let err = anyhow::Error::from(io::Error::from(io::ErrorKind::BrokenPipe));
        assert!(is_broken_pipe(&err));
        let wrapped = anyhow::Error::from(serde_json::Error::io(io::Error::from(
            io::ErrorKind::BrokenPipe,
        )));
        assert!(is_broken_pipe(&wrapped));
    }

    #[test]
    fn other_errors_are_not_broken_pipe() {
        let err = anyhow::Error::from(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(!is_broken_pipe(&err));
        assert!(!is_broken_pipe(&anyhow::anyhow!("boom")));
    }
}
