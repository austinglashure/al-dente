//! Progress messages on stderr while a target is analyzed, so a long run
//! on a large repository visibly moves: each pipeline step, units as they
//! finish, and a "still working" line every so often during a long stage.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use aldente_graph::pipeline::{Progress, Step};

/// Quiet time after which the heartbeat says what is still running.
const HEARTBEAT: Duration = Duration::from_secs(15);
/// With at most this many units, every unit's stages are printed.
const STAGES_UP_TO: usize = 4;
/// Least time between two "units done" lines (the last one always prints).
const DONE_EVERY: Duration = Duration::from_secs(2);

/// What is running now, shared with the heartbeat thread.
#[derive(Debug)]
struct State {
    started: Instant,
    /// When a line was last printed.
    last: Instant,
    /// The step outside the units (walking, building, …), if any.
    step: Option<String>,
    /// Units in flight: unit → "stage (N files)".
    running: BTreeMap<String, String>,
    jobs: usize,
    /// When the last "units done" line was printed.
    last_done: Option<Instant>,
}

/// Prints pipeline steps to stderr with the `aldente:` prefix.
#[derive(Debug)]
pub struct StderrProgress {
    state: Arc<Mutex<State>>,
    /// Dropping it wakes and ends the heartbeat thread at once.
    stop: Option<Sender<()>>,
    heartbeat: Option<JoinHandle<()>>,
}

impl StderrProgress {
    /// Starts the reporter and its heartbeat thread.
    ///
    /// Example: `let p = StderrProgress::start(); analyze_cached(…, &p)`.
    pub fn start() -> StderrProgress {
        let now = Instant::now();
        let state = Arc::new(Mutex::new(State {
            started: now,
            last: now,
            step: None,
            running: BTreeMap::new(),
            jobs: 0,
            last_done: None,
        }));
        let (stop, stopped) = mpsc::channel();
        let heartbeat = {
            let state = Arc::clone(&state);
            std::thread::Builder::new()
                .name("aldente-progress".into())
                .spawn(move || beat(&state, &stopped))
                .ok()
        };
        StderrProgress {
            state,
            stop: Some(stop),
            heartbeat,
        }
    }

    /// Prints how long the analysis took.
    ///
    /// Example: prints `aldente: analysis done in 42.1s`.
    pub fn finish(&self) {
        let s = lock(&self.state);
        eprintln!("aldente: analysis done in {}", elapsed(s.started.elapsed()));
    }
}

impl Drop for StderrProgress {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(h) = self.heartbeat.take() {
            let _ = h.join();
        }
    }
}

impl Progress for StderrProgress {
    fn step(&self, step: Step<'_>) {
        let mut s = lock(&self.state);
        if let Some(line) = line_for(&mut s, step, Instant::now()) {
            eprintln!("aldente: {line}");
            s.last = Instant::now();
        }
    }
}

/// Locks the state; a panic elsewhere does not stop progress messages.
///
/// Example: `lock(&state).jobs`.
fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Updates the state for `step` and returns the line to print, if any.
///
/// Example: `Step::Discovered { files: 1200, units: 3 }` gives
/// `"1,200 source files in 3 units"`.
fn line_for(s: &mut State, step: Step<'_>, now: Instant) -> Option<String> {
    match step {
        Step::Walking => {
            s.step = Some("reading the repository".into());
            Some("reading the repository and hashing files…".into())
        }
        Step::Discovered { files, units } => {
            s.step = None;
            Some(format!(
                "{} source files in {} unit{}",
                thousands(files),
                thousands(units),
                if units == 1 { "" } else { "s" }
            ))
        }
        Step::CacheHit => None,
        Step::Analyzing { jobs } => {
            s.jobs = jobs;
            Some(format!(
                "analyzing {} unit{} (clj-kondo, then the form index and resolution)",
                thousands(jobs),
                if jobs == 1 { "" } else { "s" }
            ))
        }
        Step::UnitStage { unit, files, stage } => {
            let what = format!("{stage} ({} files)", thousands(files));
            s.running.insert(unit.to_owned(), what.clone());
            (s.jobs <= STAGES_UP_TO).then(|| format!("{}: {what}", unit_name(unit)))
        }
        Step::UnitDone { unit, done, total } => {
            s.running.remove(unit);
            let due = done == total || s.last_done.is_none_or(|t| now - t >= DONE_EVERY);
            if !due || total == 1 {
                return None;
            }
            s.last_done = Some(now);
            Some(format!("{done} of {total} units analyzed"))
        }
        Step::Building => {
            s.step = Some("building the graph".into());
            Some("building the graph…".into())
        }
        Step::Saving => {
            s.step = Some("saving the snapshot".into());
            Some("saving the snapshot…".into())
        }
    }
}

/// The heartbeat line for the current state: what is running and for how
/// long the analysis has been going.
///
/// Example: `"still working (2m05s): clj-kondo (1,200 files) on services/billing"`.
fn heartbeat_line(s: &State) -> String {
    let what = if s.running.is_empty() {
        s.step.clone().unwrap_or_else(|| "analyzing".into())
    } else {
        let mut parts: Vec<String> = s
            .running
            .iter()
            .take(3)
            .map(|(u, w)| format!("{w} on {}", unit_name(u)))
            .collect();
        if s.running.len() > 3 {
            parts.push(format!("{} more units", s.running.len() - 3));
        }
        parts.join(", ")
    };
    format!("still working ({}): {what}", elapsed(s.started.elapsed()))
}

/// Every 200 ms, prints a heartbeat when nothing was printed for a while.
///
/// Example: runs on the `aldente-progress` thread until the `stop` sender is dropped.
fn beat(state: &Mutex<State>, stopped: &mpsc::Receiver<()>) {
    // Waiting on the channel, not sleeping, so a drop ends this at once.
    while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(Duration::from_millis(200)) {
        let mut s = lock(state);
        if s.last.elapsed() >= HEARTBEAT {
            eprintln!("aldente: {}", heartbeat_line(&s));
            s.last = Instant::now();
        }
    }
}

/// A unit's display name: its directory, `.` for the root.
///
/// Example: `unit_name("")` is `"(root)"`; `unit_name("services/billing")` is itself.
fn unit_name(dir: &str) -> &str {
    if dir.is_empty() || dir == "." {
        "(root)"
    } else {
        dir
    }
}

/// A count with thousands separators.
///
/// Example: `thousands(1234567)` is `"1,234,567"`.
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A duration as `42.1s` or `2m05s`.
///
/// Example: `elapsed(Duration::from_secs(125))` is `"2m05s"`.
fn elapsed(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}m{:02}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dropping the reporter must not wait out the heartbeat's poll
    /// interval: it used to sleep 200 ms per beat, so every analysis ended
    /// on the next 200 ms tick.
    #[test]
    fn drop_does_not_wait_for_the_heartbeat_tick() {
        for _ in 0..5 {
            let p = StderrProgress::start();
            std::thread::sleep(Duration::from_millis(20));
            let t = Instant::now();
            drop(p);
            assert!(
                t.elapsed() < Duration::from_millis(50),
                "drop took {:?}",
                t.elapsed()
            );
        }
    }

    fn state() -> State {
        let now = Instant::now();
        State {
            started: now,
            last: now,
            step: None,
            running: BTreeMap::new(),
            jobs: 0,
            last_done: None,
        }
    }

    #[test]
    fn formats() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(elapsed(Duration::from_millis(4200)), "4.2s");
        assert_eq!(elapsed(Duration::from_secs(125)), "2m05s");
    }

    #[test]
    fn few_units_print_stages_many_units_print_throttled_counts() {
        let mut s = state();
        let t0 = Instant::now();
        assert_eq!(
            line_for(
                &mut s,
                Step::Discovered {
                    files: 1200,
                    units: 1
                },
                t0
            )
            .as_deref(),
            Some("1,200 source files in 1 unit")
        );
        line_for(&mut s, Step::Analyzing { jobs: 1 }, t0);
        let stage = Step::UnitStage {
            unit: "",
            files: 1200,
            stage: "clj-kondo",
        };
        assert_eq!(
            line_for(&mut s, stage, t0).as_deref(),
            Some("(root): clj-kondo (1,200 files)")
        );
        assert!(heartbeat_line(&s).ends_with("clj-kondo (1,200 files) on (root)"));
        let done = Step::UnitDone {
            unit: "",
            done: 1,
            total: 1,
        };
        assert_eq!(line_for(&mut s, done, t0), None, "one unit: no count line");

        let mut s = state();
        line_for(&mut s, Step::Analyzing { jobs: 50 }, t0);
        let stage = Step::UnitStage {
            unit: "a",
            files: 3,
            stage: "resolve",
        };
        assert_eq!(
            line_for(&mut s, stage, t0),
            None,
            "many units: stages only in the heartbeat"
        );
        let d = |done| Step::UnitDone {
            unit: "a",
            done,
            total: 50,
        };
        assert!(line_for(&mut s, d(1), t0).is_some());
        assert!(line_for(&mut s, d(2), t0 + Duration::from_millis(500)).is_none());
        assert!(line_for(&mut s, d(3), t0 + Duration::from_secs(3)).is_some());
        assert_eq!(
            line_for(&mut s, d(50), t0 + Duration::from_secs(3)).as_deref(),
            Some("50 of 50 units analyzed")
        );
    }

    #[test]
    fn heartbeat_names_at_most_three_units() {
        let mut s = state();
        for u in ["a", "b", "c", "d", "e"] {
            s.running.insert(u.into(), "clj-kondo (1 files)".into());
        }
        let line = heartbeat_line(&s);
        assert!(
            line.contains("on a, ") && line.contains("on c") && line.ends_with("2 more units"),
            "{line}"
        );
        s.running.clear();
        s.step = Some("building the graph".into());
        assert!(heartbeat_line(&s).ends_with(": building the graph"));
    }
}
