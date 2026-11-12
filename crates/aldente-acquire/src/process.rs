//! Running an external tool as an argument vector (never through a shell),
//! with a timeout, from a neutral working directory.

use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

/// What a finished process printed.
#[derive(Debug)]
pub struct Captured {
    pub status: ExitStatus,
    /// stdout, lossily decoded as UTF-8.
    pub stdout: String,
    /// stderr, lossily decoded as UTF-8.
    pub stderr: String,
}

/// Why a tool could not be run to completion.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("could not start: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("timed out after {0:?}")]
    Timeout(Duration),
    #[error("could not wait for the process: {0}")]
    Wait(#[source] std::io::Error),
}

/// How often a running child is polled for exit.
const POLL: Duration = Duration::from_millis(5);

/// Runs `program` with `args`, stdin closed and the working directory set to
/// the system temp directory (so nothing in the current directory, such as an
/// analyzed repository, is picked up). Kills it after `timeout`.
///
/// Example: `run_captured(Path::new("/usr/bin/git"), &["--version"], 10s)`
/// gives `Captured { stdout: "git version 2.34.1\n", .. }`.
pub fn run_captured(
    program: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<Captured, RunError> {
    let mut command = Command::new(program);
    command.args(args).current_dir(std::env::temp_dir());
    run_command(command, timeout)
}

/// Runs a prepared command (its program, arguments, environment and working
/// directory already set) with stdin closed, capturing output, killing it
/// after `timeout`.
///
/// Example: `run_command(git_command, Duration::from_secs(600))` gives the
/// captured output of a `git fetch`.
pub fn run_command(mut command: Command, timeout: Duration) -> Result<Captured, RunError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RunError::Spawn)?;
    // Drain both pipes on threads so a chatty child cannot block on a full pipe.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = wait_with_timeout(&mut child, timeout)?;
    Ok(Captured {
        status,
        stdout: collect(stdout),
        stderr: collect(stderr),
    })
}

/// Waits for `child` to exit, killing it once `timeout` has passed.
///
/// Example: a child that exits at once gives `Ok(status)`; one that sleeps
/// past `timeout` is killed and gives `Err(RunError::Timeout(timeout))`.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus, RunError> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(RunError::Wait)? {
            return Ok(status);
        }
        if start.elapsed() >= timeout {
            // Ignore errors: the child may have exited between the checks.
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunError::Timeout(timeout));
        }
        thread::sleep(POLL);
    }
}

/// Reads a pipe to the end on its own thread.
///
/// Example: `drain(Some(stdout))` later yields the child's whole stdout.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> Option<JoinHandle<Vec<u8>>> {
    pipe.map(|mut pipe| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            // A read error leaves what was read so far, which is all we can report.
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    })
}

/// Joins a [`drain`] thread and decodes its bytes.
///
/// Example: a thread that read `b"hi\n"` gives `"hi\n"`.
fn collect(handle: Option<JoinHandle<Vec<u8>>>) -> String {
    let bytes = handle
        .map(|h| h.join().unwrap_or_default())
        .unwrap_or_default();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_and_status() {
        let out = run_captured(Path::new("/bin/echo"), &["hi"], Duration::from_secs(5)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, "hi\n");
    }

    #[test]
    fn kills_a_process_that_runs_too_long() {
        let started = Instant::now();
        let err =
            run_captured(Path::new("/bin/sleep"), &["5"], Duration::from_millis(100)).unwrap_err();
        assert!(matches!(err, RunError::Timeout(_)), "{err}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let err =
            run_captured(Path::new("/nonexistent/tool"), &[], Duration::from_secs(1)).unwrap_err();
        assert!(matches!(err, RunError::Spawn(_)), "{err}");
    }
}
