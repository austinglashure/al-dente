//! Milestone M3 (WP3.8): forward and reverse craters of the three
//! repository functions with the highest fan-in, timed, and a sample of
//! twenty reverse-crater entries with every step's call site and source
//! line, for a hand check.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::Path;
use std::time::Instant;

use aldente_analysis::crater::{CraterIndex, CraterOptions, crater};
use aldente_graph::Graph;
use aldente_model::crater::{CraterResult, Direction};
use aldente_model::{Certainty, EdgeKind, NodeId, NodeKind, SymbolFlag};
use serde::Serialize;

/// How many seeds and how many hand-check entries.
const SEEDS: usize = 3;
const SAMPLE: usize = 20;

/// One timed crater.
#[derive(Debug, Serialize)]
pub struct Timed {
    pub seed: String,
    pub fan_in: usize,
    pub direction: Direction,
    pub nodes: usize,
    pub hidden_below_min: u64,
    pub truncated: bool,
    pub seconds: f64,
}

/// The M3 numbers for one repository.
#[derive(Debug, Serialize)]
pub struct Report {
    pub index_seconds: f64,
    pub craters: Vec<Timed>,
    /// The hand-check sample: each entry's path to its seed.
    pub sample: Vec<SampleEntry>,
}

/// One sampled reverse-crater entry.
#[derive(Debug, Serialize)]
pub struct SampleEntry {
    pub seed: String,
    pub id: String,
    pub distance: u32,
    pub path_certainty: Certainty,
    pub steps: Vec<Step>,
}

/// One step of a path: `from` invokes `to` at `site`, whose source line is `text`.
#[derive(Debug, Serialize)]
pub struct Step {
    pub from: String,
    pub to: String,
    pub edge: String,
    pub site: String,
    pub text: String,
}

/// Invocation kinds that count for fan-in (the crater's default kinds).
const INVOKE: &[EdgeKind] = &[
    EdgeKind::Calls,
    EdgeKind::References,
    EdgeKind::Dispatches,
    EdgeKind::Registers,
];

/// Repository symbols (not tests) by how many distinct other repository
/// nodes invoke them at `likely` or stronger, highest first, then by ID.
///
/// Example: in ring, `ring.util.response/get-header`-style helpers rank high.
pub fn by_fan_in(g: &Graph) -> Vec<(NodeId, usize)> {
    let mut callers: BTreeMap<&NodeId, BTreeSet<&NodeId>> = BTreeMap::new();
    for e in &g.edges {
        if !INVOKE.contains(&e.kind) || e.from == e.to {
            continue;
        }
        if e.certainty.unwrap_or(Certainty::Certain) < Certainty::Likely {
            continue;
        }
        let (Some(to), Some(from)) = (g.nodes.get(&e.to), g.nodes.get(&e.from)) else {
            continue;
        };
        let repo = |k: NodeKind| matches!(k, NodeKind::Symbol | NodeKind::Impl);
        if to.kind != NodeKind::Symbol || !repo(from.kind) || to.flags.contains(&SymbolFlag::Test) {
            continue;
        }
        callers.entry(&e.to).or_default().insert(&e.from);
    }
    let mut ranked: Vec<(NodeId, usize)> = callers
        .into_iter()
        .map(|(id, c)| (id.clone(), c.len()))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

/// Runs the craters and builds the sample. `root` is the clone, for
/// reading source lines.
///
/// Example: `m3::run(&graph, &root)` times six craters and samples twenty entries.
pub fn run(g: &Graph, root: &Path) -> (Report, String) {
    let started = Instant::now();
    let index = CraterIndex::new(g);
    let index_seconds = started.elapsed().as_secs_f64();
    let seeds: Vec<(NodeId, usize)> = by_fan_in(g).into_iter().take(SEEDS).collect();
    let mut craters = Vec::new();
    let mut reverse: Vec<(NodeId, CraterResult)> = Vec::new();
    for (seed, fan_in) in &seeds {
        for direction in [Direction::Forward, Direction::Reverse] {
            let opts = CraterOptions {
                direction,
                ..CraterOptions::default()
            };
            let t = Instant::now();
            let r = crater(g, &index, std::slice::from_ref(seed), &opts);
            let seconds = t.elapsed().as_secs_f64();
            let Ok(r) = r else { continue };
            craters.push(Timed {
                seed: seed.to_string(),
                fan_in: *fan_in,
                direction,
                nodes: r.reach.len(),
                hidden_below_min: r.hidden_below_min,
                truncated: r.truncated,
                seconds,
            });
            if direction == Direction::Reverse {
                reverse.push((seed.clone(), r));
            }
        }
    }
    let sample = sample(g, root, &reverse);
    let text = render(&craters, index_seconds, &sample);
    (
        Report {
            index_seconds,
            craters,
            sample,
        },
        text,
    )
}

/// Twenty non-seed entries spread evenly over the reverse craters' entries
/// sorted by distance (farthest first) and ID, each with its path.
///
/// Example: with 200 candidates, every tenth.
fn sample(g: &Graph, root: &Path, reverse: &[(NodeId, CraterResult)]) -> Vec<SampleEntry> {
    let mut candidates: Vec<(&NodeId, &CraterResult, &aldente_model::crater::Reach)> = Vec::new();
    let mut seen = BTreeSet::new();
    for (seed, r) in reverse {
        for x in &r.reach {
            if x.via.is_some()
                && g.nodes
                    .get(&x.id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Symbol | NodeKind::Impl))
                && seen.insert(&x.id)
            {
                candidates.push((seed, r, x));
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.2.distance
            .cmp(&a.2.distance)
            .then_with(|| a.2.id.cmp(&b.2.id))
    });
    let n = candidates.len();
    let picks: Vec<usize> = if n <= SAMPLE {
        (0..n).collect()
    } else {
        (0..SAMPLE).map(|i| i * n / SAMPLE).collect()
    };
    picks
        .into_iter()
        .map(|i| {
            let (seed, r, x) = candidates[i];
            SampleEntry {
                seed: seed.to_string(),
                id: x.id.to_string(),
                distance: x.distance,
                path_certainty: x.path_certainty,
                steps: steps(g, root, r, &x.id),
            }
        })
        .collect()
}

/// The path from `id` to the seed along `via`; in a reverse crater each
/// node invokes its `via`.
///
/// Example: `[a -calls-> b, b -calls-> seed]`.
fn steps(g: &Graph, root: &Path, r: &CraterResult, id: &NodeId) -> Vec<Step> {
    let via: BTreeMap<&NodeId, Option<&NodeId>> =
        r.reach.iter().map(|x| (&x.id, x.via.as_ref())).collect();
    let mut out = Vec::new();
    let mut cur = id;
    while let Some(Some(next)) = via.get(cur) {
        let edge = g
            .edges
            .iter()
            .find(|e| &e.from == cur && &e.to == *next && INVOKE.contains(&e.kind));
        let (label, site, text) = match edge {
            Some(e) => {
                let token = |v: Option<serde_json::Value>| {
                    v.and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "-".into())
                };
                let label = format!(
                    "{} {} {}",
                    token(serde_json::to_value(e.kind).ok()),
                    token(e.certainty.and_then(|c| serde_json::to_value(c).ok())),
                    token(e.reason.and_then(|c| serde_json::to_value(c).ok()))
                );
                match e.sites.first() {
                    Some(s) => (
                        label,
                        format!("{}:{}:{}", s.file, s.line, s.col),
                        line_of(root, s.file.as_str(), s.line),
                    ),
                    None => (label, String::new(), String::new()),
                }
            }
            None => ("(no edge)".to_owned(), String::new(), String::new()),
        };
        out.push(Step {
            from: cur.to_string(),
            to: next.to_string(),
            edge: label,
            site,
            text,
        });
        cur = next;
        if out.len() > 64 {
            break;
        }
    }
    out
}

/// Line `line` (1-based) of `file` under `root`, trimmed.
///
/// Example: `line_of(root, "src/a.clj", 3)` gives `"(defn f [] (g))"`.
fn line_of(root: &Path, file: &str, line: u32) -> String {
    std::fs::read_to_string(root.join(file))
        .ok()
        .and_then(|t| {
            t.lines()
                .nth(line.saturating_sub(1) as usize)
                .map(|l| l.trim().to_owned())
        })
        .unwrap_or_default()
}

/// The printable summary and hand-check list.
///
/// Example: starts with `index 0.012s` and one line per crater.
fn render(craters: &[Timed], index_seconds: f64, sample: &[SampleEntry]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "crater index built in {index_seconds:.3}s");
    for c in craters {
        let _ = writeln!(
            s,
            "  {:<8} {}  fan-in {}  {} nodes  (+{} at possible){}  {:.3}s",
            format!("{:?}", c.direction).to_lowercase(),
            c.seed,
            c.fan_in,
            c.nodes,
            c.hidden_below_min,
            if c.truncated { "  truncated" } else { "" },
            c.seconds
        );
    }
    let _ = writeln!(s, "hand-check sample ({}):", sample.len());
    for (i, e) in sample.iter().enumerate() {
        let _ = writeln!(
            s,
            "{:>2}. {}  (distance {}, {:?}) -> seed {}",
            i + 1,
            e.id,
            e.distance,
            e.path_certainty,
            e.seed
        );
        for st in &e.steps {
            let _ = writeln!(s, "      {} -[{}]-> {}", st.from, st.edge, st.to);
            let _ = writeln!(s, "        {}  {}", st.site, st.text);
        }
    }
    s
}
