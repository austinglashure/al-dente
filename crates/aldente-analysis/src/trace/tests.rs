//! The normalizer on the trace corpus and on traces from clj-mini.

use std::fmt::Write;
use std::path::PathBuf;

use aldente_graph::Graph;
use aldente_model::trace::{MatchLevel, Trace, TraceFormat};
use aldente_testkit::golden::{analyze_fixture, fixture_dir};

use super::normalize;

fn traces_dir(sub: &str) -> PathBuf {
    let dir = fixture_dir("traces").join(sub);
    assert!(dir.is_dir());
    dir
}

/// One line per trace and cause: exception, frame counts, first non-noise frame.
fn summary(t: &Trace, depth: usize, out: &mut String) {
    let ex = &t.exception;
    let noise = t.frames.iter().filter(|f| f.noise).count();
    let first = t
        .frames
        .iter()
        .find(|f| !f.noise)
        .map(|f| {
            format!(
                "{}/{}{} {}:{}",
                f.namespace.as_deref().unwrap_or("?"),
                f.name.as_deref().unwrap_or("?"),
                if f.anonymous { " (anon)" } else { "" },
                f.file.as_deref().unwrap_or("?"),
                f.line.map_or("?".into(), |l| l.to_string())
            )
        })
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{}{} | {} | frames {} noise {} omitted {} | {}",
        "  ".repeat(depth + 1),
        ex.class.as_deref().unwrap_or("-"),
        ex.message.as_deref().unwrap_or("-"),
        t.frames.len(),
        noise,
        t.omitted,
        first
    );
    for c in &t.causes {
        summary(c, depth + 1, out);
    }
}

#[test]
fn jvm_corpus_parses() {
    let dir = traces_dir("jvm");
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "txt" || x == "edn"))
        .collect();
    names.sort();
    assert!(names.len() >= 12, "corpus has {} traces", names.len());
    let empty = Graph::default();
    let mut out = String::new();
    for path in names {
        let text = std::fs::read_to_string(&path).unwrap();
        let report = normalize(&text, &empty).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let name = path.file_name().unwrap().to_string_lossy();
        let _ = writeln!(
            out,
            "{name} ({:?}, {} traces)",
            report.format,
            report.traces.len()
        );
        for t in &report.traces {
            summary(t, 0, &mut out);
        }
    }
    insta::assert_snapshot!("jvm_corpus", out);
}

#[test]
fn log_lines_inside_a_trace_do_not_break_it() {
    let dir = traces_dir("jvm");
    let plain = normalize(
        &std::fs::read_to_string(dir.join("01-plain.txt")).unwrap(),
        &Graph::default(),
    )
    .unwrap();
    let log = normalize(
        &std::fs::read_to_string(dir.join("18-log-interleaved.txt")).unwrap(),
        &Graph::default(),
    )
    .unwrap();
    assert_eq!(log.traces.len(), 2);
    assert_eq!(log.traces[0].frames, plain.traces[0].frames);
}

#[test]
fn clj_mini_traces_map_every_repository_frame_exactly() {
    let graph = analyze_fixture(&fixture_dir("clj-mini")).graph;
    let dir = traces_dir("jvm/clj-mini");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt" || x == "edn"))
        .collect();
    files.sort();
    assert!(files.len() >= 6);
    let mut out = String::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let report = normalize(&text, &graph).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.ends_with(".edn") {
            assert_eq!(report.format, TraceFormat::Edn);
        }
        let _ = writeln!(out, "{name}");
        let mut stack: Vec<&Trace> = report.traces.iter().collect();
        let mut repo_frames = 0;
        while let Some(t) = stack.pop() {
            stack.extend(t.causes.iter());
            for f in &t.frames {
                let repo = f
                    .namespace
                    .as_deref()
                    .is_some_and(|ns| ns.starts_with("shop."));
                if repo {
                    repo_frames += 1;
                    assert!(!f.noise, "{name}: repository frame marked noise: {}", f.raw);
                    assert_eq!(f.match_level, MatchLevel::Exact, "{name}: {}", f.raw);
                    let _ = writeln!(
                        out,
                        "  {}  ->  {}",
                        f.raw,
                        f.node.as_ref().map(ToString::to_string).unwrap_or_default()
                    );
                }
            }
        }
        assert!(repo_frames > 0, "{name} has no repository frames");
    }
    insta::assert_snapshot!("clj_mini_mapping", out);
}

#[test]
fn corpus_frames_map_exactly_against_the_corpus_source() {
    let graph = analyze_fixture(&fixture_dir("traces/jvm/gen")).graph;
    let dir = traces_dir("jvm");
    let mut out = String::new();
    for name in [
        "06-anonymous.txt",
        "07-anonymous-literal.txt",
        "08-protocol-record.txt",
        "09-protocol-extend.txt",
        "10-multimethod.txt",
        "11-munged-names.txt",
        "12-repl.txt",
        "13-test-runner.txt",
        "14-throwable-map.edn",
    ] {
        let report = normalize(&std::fs::read_to_string(dir.join(name)).unwrap(), &graph).unwrap();
        let _ = writeln!(out, "{name}");
        let mut stack: Vec<&Trace> = report.traces.iter().collect();
        while let Some(t) = stack.pop() {
            stack.extend(t.causes.iter());
            for f in t.frames.iter().filter(|f| {
                f.namespace
                    .as_deref()
                    .is_some_and(|n| n.starts_with("corpus."))
            }) {
                assert_eq!(f.match_level, MatchLevel::Exact, "{name}: {}", f.raw);
                let _ = writeln!(
                    out,
                    "  {}  ->  {}",
                    f.raw,
                    f.node.as_ref().map(ToString::to_string).unwrap_or_default()
                );
            }
        }
    }
    insta::assert_snapshot!("corpus_mapping", out);
}
