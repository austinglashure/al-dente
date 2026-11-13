//! Clojure source for a smaller synthetic monorepo, for end-to-end timing
//! of the whole pipeline (WP5.1). Services depend on libraries through
//! `:local/root`; namespaces require earlier ones, so there are no cycles.

use std::collections::BTreeSet;
use std::path::Path;

use aldente_analysis::measures::paths::Rng;

/// What to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrcSpec {
    pub units: usize,
    pub namespaces: usize,
    pub fns_per_ns: usize,
    pub calls_per_fn: usize,
    pub seed: u64,
}

impl Default for SrcSpec {
    /// 4 units, 200 namespaces, 25 functions each, up to 4 calls per function.
    ///
    /// Example: `SrcSpec::default().namespaces` is 200.
    fn default() -> SrcSpec {
        SrcSpec {
            units: 4,
            namespaces: 200,
            fns_per_ns: 25,
            calls_per_fn: 4,
            seed: 1,
        }
    }
}

/// What was written: the file count and every repository call, as
/// `(caller ID, callee ID)`, sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    pub files: usize,
    pub symbols: usize,
    pub calls: BTreeSet<(String, String)>,
}

/// One unit of the corpus.
#[derive(Debug, Clone)]
struct Unit {
    dir: String,
    service: bool,
    /// Global namespace indices.
    namespaces: std::ops::Range<usize>,
}

/// One namespace: its name, the unit it is in, and the namespaces it
/// requires with their aliases.
#[derive(Debug, Clone)]
struct Ns {
    unit: usize,
    name: String,
    requires: Vec<usize>,
}

/// Writes the corpus under `out` (which must be empty or missing).
///
/// Example: `write_corpus(Path::new("target/bench/src"), &SrcSpec::default())?.files` is 200.
pub fn write_corpus(out: &Path, spec: &SrcSpec) -> anyhow::Result<Manifest> {
    anyhow::ensure!(
        spec.units >= 1 && spec.namespaces >= spec.units && spec.fns_per_ns >= 1,
        "need at least one unit, one namespace per unit and one function per namespace"
    );
    if out.exists() {
        anyhow::ensure!(
            std::fs::read_dir(out)?.next().is_none(),
            "{} is not empty; choose an empty or new directory",
            out.display()
        );
    }
    let mut rng = Rng::new(spec.seed);
    let units = plan_units(spec);
    let namespaces = plan_namespaces(&units, &mut rng);
    let mut manifest = Manifest::default();
    for unit in &units {
        write_file(
            &out.join(&unit.dir).join("deps.edn"),
            &deps_edn(unit, &units),
        )?;
    }
    for (i, ns) in namespaces.iter().enumerate() {
        let main = units[ns.unit].service && units[ns.unit].namespaces.end == i + 1;
        let (text, calls) = namespace_source(i, &namespaces, &units, spec, main, &mut rng);
        let path = out
            .join(&units[ns.unit].dir)
            .join("src")
            .join(ns.name.replace('.', "/").replace('-', "_") + ".clj");
        write_file(&path, &text)?;
        manifest.files += 1;
        manifest.symbols += spec.fns_per_ns + usize::from(main);
        manifest.calls.extend(calls);
    }
    Ok(manifest)
}

/// Services first (about three in five), then libraries; namespaces split
/// evenly.
///
/// Example: 3 units and 9 namespaces give `services/svc-00` (0..3), `libs/lib-00` (3..6), `libs/lib-01` (6..9).
fn plan_units(spec: &SrcSpec) -> Vec<Unit> {
    let services = (spec.units * 3 / 5).max(1);
    let mut start = 0;
    (0..spec.units)
        .map(|i| {
            let size = spec.namespaces / spec.units + usize::from(i < spec.namespaces % spec.units);
            let dir = if i < services {
                format!("services/svc-{i:02}")
            } else {
                format!("libs/lib-{:02}", i - services)
            };
            let unit = Unit {
                dir,
                service: i < services,
                namespaces: start..start + size,
            };
            start += size;
            unit
        })
        .collect()
}

/// Names and requires: up to two earlier namespaces of the same unit, and
/// for service namespaces one library namespace.
///
/// Example: `synth.u00.n0002` might require `synth.u00.n0000` and `synth.u01.n0001`.
fn plan_namespaces(units: &[Unit], rng: &mut Rng) -> Vec<Ns> {
    let libs: Vec<&Unit> = units.iter().filter(|u| !u.service).collect();
    let mut out = Vec::new();
    for (u, unit) in units.iter().enumerate() {
        for k in 0..unit.namespaces.len() {
            let mut requires = BTreeSet::new();
            for _ in 0..2.min(k) {
                requires.insert(unit.namespaces.start + rng.below(k));
            }
            if unit.service && !libs.is_empty() {
                let lib = libs[rng.below(libs.len())];
                requires.insert(lib.namespaces.start + rng.below(lib.namespaces.len()));
            }
            out.push(Ns {
                unit: u,
                name: format!("synth.u{u:02}.n{k:04}"),
                requires: requires.into_iter().collect(),
            });
        }
    }
    out
}

/// A unit's `deps.edn`: `src` as its path; services depend on every
/// library by `:local/root`.
///
/// Example: `{:paths ["src"] :deps {synth/lib-00 {:local/root "../../libs/lib-00"}}}`.
fn deps_edn(unit: &Unit, units: &[Unit]) -> String {
    let deps: Vec<String> = if unit.service {
        units
            .iter()
            .filter(|u| !u.service)
            .map(|u| {
                let name = u.dir.rsplit('/').next().unwrap_or(&u.dir);
                format!("synth/{name} {{:local/root \"../../{}\"}}", u.dir)
            })
            .collect()
    } else {
        Vec::new()
    };
    format!(
        "{{:paths [\"src\"]\n :deps {{{}}}}}\n",
        deps.join("\n        ")
    )
}

/// The source of namespace `i` and the repository calls it makes. Function
/// `j` calls up to `calls_per_fn` of: earlier functions in the same
/// namespace, any function of a required namespace; always through `str`.
///
/// Example: `(defn f0002 [x] (str (f0000 x) (r0/f0003 x)))`.
fn namespace_source(
    i: usize,
    namespaces: &[Ns],
    units: &[Unit],
    spec: &SrcSpec,
    main: bool,
    rng: &mut Rng,
) -> (String, Vec<(String, String)>) {
    let ns = &namespaces[i];
    let id = |n: usize, f: &str| {
        format!(
            "clj:{}//{}/{f}",
            units[namespaces[n].unit].dir, namespaces[n].name
        )
    };
    let mut text = format!("(ns {}", ns.name);
    if !ns.requires.is_empty() {
        text.push_str("\n  (:require");
        for (a, r) in ns.requires.iter().enumerate() {
            text.push_str(&format!("\n   [{} :as r{a}]", namespaces[*r].name));
        }
        text.push(')');
    }
    text.push_str(")\n");
    let mut calls = Vec::new();
    for j in 0..spec.fns_per_ns {
        let mut args = BTreeSet::new();
        for _ in 0..spec.calls_per_fn {
            let local = j > 0 && (ns.requires.is_empty() || rng.below(2) == 0);
            if local {
                let f = rng.below(j);
                args.insert((format!("f{f:04}"), id(i, &format!("f{f:04}"))));
            } else if !ns.requires.is_empty() {
                let a = rng.below(ns.requires.len());
                let f = format!("f{:04}", rng.below(spec.fns_per_ns));
                args.insert((format!("r{a}/{f}"), id(ns.requires[a], &f)));
            }
        }
        let name = format!("f{j:04}");
        let body: Vec<String> = args.iter().map(|(call, _)| format!("({call} x)")).collect();
        text.push_str(&format!(
            "\n(defn {name}\n  \"Synthetic function {j}.\"\n  [x]\n  (str x {}))\n",
            body.join(" ")
        ));
        calls.extend(args.into_iter().map(|(_, to)| (id(i, &name), to)));
    }
    if main {
        text.push_str("\n(defn -main [& args]\n  (println (f0000 args)))\n");
        calls.push((id(i, "-main"), id(i, "f0000")));
    }
    (text, calls)
}

/// Writes `text` to `path`, creating directories.
///
/// Example: `write_file(&root.join("services/svc-00/deps.edn"), "{}")`.
fn write_file(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use aldente_model::{Certainty, EdgeKind, NodeKind, Reason};

    use super::*;

    #[test]
    fn analysis_finds_exactly_the_written_calls() {
        let dir = std::env::temp_dir().join(format!("aldente-bench-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let spec = SrcSpec {
            units: 3,
            namespaces: 9,
            fns_per_ns: 4,
            calls_per_fn: 3,
            seed: 5,
        };
        let manifest = write_corpus(&dir, &spec).unwrap();
        assert_eq!(manifest.files, 9);
        assert!(manifest.calls.len() > 20, "{}", manifest.calls.len());
        let cross_unit = manifest
            .calls
            .iter()
            .filter(|(a, b)| a.split("//").next() != b.split("//").next())
            .count();
        assert!(cross_unit > 0, "services call into libraries");

        let analyzed = aldente_testkit::golden::analyze_fixture(&dir);
        let g = &analyzed.graph;
        assert_eq!(analyzed.discovery.report.units.len(), 3);
        assert!(g.diagnostics.is_empty(), "{:?}", g.diagnostics);
        let symbols = g
            .nodes
            .values()
            .filter(|n| n.kind == NodeKind::Symbol)
            .count();
        assert_eq!(symbols, manifest.symbols);
        let found: BTreeSet<(String, String)> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls && e.reason == Some(Reason::Direct))
            .filter(|e| e.certainty == Some(Certainty::Certain))
            .filter(|e| {
                g.nodes
                    .get(&e.to)
                    .is_some_and(|n| n.kind == NodeKind::Symbol)
            })
            .map(|e| (e.from.to_string(), e.to.to_string()))
            .collect();
        assert_eq!(found, manifest.calls);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_a_directory_with_files() {
        let dir = std::env::temp_dir().join(format!("aldente-bench-full-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep.txt"), "mine").unwrap();
        assert!(write_corpus(&dir, &SrcSpec::default()).is_err());
        assert!(dir.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
