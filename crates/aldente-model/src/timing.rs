//! Wall-clock timings of pipeline stages and requests, for `--timings`.
//! Never part of a JSON `data` section: timings are not deterministic.

use std::time::{Duration, Instant};

/// The label prefix that marks a detail entry.
pub const DETAIL: &str = "· ";

/// Labelled durations in the order they were recorded. A label's first word
/// is its stage (`clj-kondo unit:a` is stage `clj-kondo`). Detail entries
/// (labels starting with [`DETAIL`]) break a stage down: they are shown
/// indented and left out of the stage sums and the total.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timings {
    entries: Vec<(String, Duration)>,
}

impl Timings {
    /// Runs `f`, records how long it took under `label`, returns its value.
    ///
    /// Example: `t.time("walk", || walk(root))` records `walk`.
    pub fn time<T>(&mut self, label: &str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.record(label, started.elapsed());
        value
    }

    /// Records a duration measured elsewhere.
    ///
    /// Example: `t.record("build", Duration::from_millis(12))`.
    pub fn record(&mut self, label: &str, took: Duration) {
        self.entries.push((label.to_owned(), took));
    }

    /// Records a detail: part of a stage recorded separately.
    ///
    /// Example: `t.detail("usages", d)` shows as `    · usages`.
    pub fn detail(&mut self, label: &str, took: Duration) {
        self.record(&format!("{DETAIL}{label}"), took);
    }

    /// Runs `f` and records its time as a detail.
    ///
    /// Example: `t.time_detail("usages", || r.usages())`.
    pub fn time_detail<T>(&mut self, label: &str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = f();
        self.detail(label, started.elapsed());
        value
    }

    /// Appends `other`'s entries with ` <suffix>` added to each label.
    ///
    /// Example: merging `resolve` under `unit:x` gives `resolve unit:x`.
    pub fn merge(&mut self, suffix: &str, other: Timings) {
        for (label, took) in other.entries {
            self.entries.push((format!("{label} {suffix}"), took));
        }
    }

    /// The entries, in recording order.
    ///
    /// Example: `[("walk", 3ms), ("build", 9ms)]`.
    pub fn entries(&self) -> &[(String, Duration)] {
        &self.entries
    }

    /// The sum of all entries. Stages that nest would count twice, so record
    /// only disjoint stages.
    ///
    /// Example: 1.5 s + 0.5 s gives 2 s.
    pub fn total(&self) -> Duration {
        self.entries
            .iter()
            .filter(|(l, _)| !l.starts_with(DETAIL))
            .map(|(_, d)| *d)
            .sum()
    }

    /// A plain-text table: every entry, then per-stage sums (with counts)
    /// slowest first, then the total.
    ///
    /// Example: `"  clj-kondo unit:a   1.500s\n…\n  total   2.000s"`.
    pub fn report(&self) -> String {
        let mut stages: Vec<(String, usize, Duration)> = Vec::new();
        for (label, took) in self.entries.iter().filter(|(l, _)| !l.starts_with(DETAIL)) {
            let stage = label.split_whitespace().next().unwrap_or(label);
            match stages.iter_mut().find(|(s, _, _)| s == stage) {
                Some((_, n, d)) => {
                    *n += 1;
                    *d += *took;
                }
                None => stages.push((stage.to_owned(), 1, *took)),
            }
        }
        stages.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        let rows: Vec<(String, Duration)> = self
            .entries
            .iter()
            .cloned()
            .chain(std::iter::once(("--- by stage".to_owned(), Duration::ZERO)))
            .chain(
                stages
                    .into_iter()
                    .map(|(s, n, d)| (format!("{s} ({n})"), d)),
            )
            .chain(std::iter::once(("total".to_owned(), self.total())))
            .collect();
        let width = rows
            .iter()
            .map(|(l, _)| l.chars().count())
            .max()
            .unwrap_or(0);
        rows.iter()
            .map(|(label, took)| {
                if label.starts_with("---") {
                    format!("  {label}")
                } else if label.starts_with(DETAIL) {
                    format!("    {label:<w$}{:>9.3}s", took.as_secs_f64(), w = width)
                } else {
                    format!("  {label:<width$}  {:>9.3}s", took.as_secs_f64())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn records_stages_in_order_and_returns_values() {
        let mut t = Timings::default();
        let v = t.time("walk", || 41 + 1);
        assert_eq!(v, 42);
        t.record("clj-kondo unit:a", Duration::from_millis(1500));
        t.record("clj-kondo unit:b", Duration::from_millis(500));
        let labels: Vec<&str> = t.entries().iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["walk", "clj-kondo unit:a", "clj-kondo unit:b"]);
        assert!(t.total() >= Duration::from_millis(2000));
    }

    #[test]
    fn report_aligns_and_sums_by_stage_kind() {
        let mut t = Timings::default();
        t.record("clj-kondo unit:a", Duration::from_millis(1500));
        t.record("resolve unit:a", Duration::from_millis(250));
        t.record("clj-kondo unit:b", Duration::from_millis(500));
        let r = t.report();
        assert!(r.contains("clj-kondo unit:a"), "{r}");
        assert!(r.contains("1.500s"), "{r}");
        // Per-stage sums: the first word of each label.
        assert!(r.contains("clj-kondo (2)"), "{r}");
        assert!(r.contains("2.000s"), "{r}");
        assert!(r.lines().last().unwrap().contains("total"), "{r}");
    }

    #[test]
    fn details_are_shown_indented_but_not_counted() {
        let mut t = Timings::default();
        t.record("resolve", Duration::from_millis(100));
        t.detail("usages", Duration::from_millis(70));
        t.detail("libraries", Duration::from_millis(30));
        assert_eq!(t.total(), Duration::from_millis(100));
        let r = t.report();
        assert!(r.contains("    · usages"), "{r}");
        assert!(!r.contains("usages (1)"), "details have no stage sum: {r}");
        let v = t.time_detail("finish", || 5);
        assert_eq!(v, 5);
        let mut outer = Timings::default();
        outer.merge("unit:x", t);
        assert!(
            outer.report().contains("    · usages unit:x"),
            "{}",
            outer.report()
        );
        assert_eq!(outer.total(), Duration::from_millis(100));
    }

    #[test]
    fn merge_appends_with_a_prefix() {
        let mut a = Timings::default();
        let mut b = Timings::default();
        b.record("resolve", Duration::from_millis(3));
        a.merge("unit:x", b);
        assert_eq!(a.entries()[0].0, "resolve unit:x");
    }
}
