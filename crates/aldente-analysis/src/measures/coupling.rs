//! Instability, abstractness and layers per namespace or unit, and the
//! dependencies that break the layering or the stable-dependencies rule.

use aldente_graph::Graph;
use aldente_model::measures::{Coupling, CouplingReport, ProjectionSpec, Violation};
use aldente_model::{AggregationLevel, NodeId, NodeKind, SymbolSubkind};

use super::projection::{Ix, Projection, ProjectionError, project};
use super::scc;

/// Coupling at namespace or unit level (`spec.level`); symbol level is
/// treated as namespace level.
///
/// Example: for clj-mini, `shop.util` has afferent coupling from
/// `shop.core` and instability below `shop.core`'s.
pub fn coupling(graph: &Graph, spec: &ProjectionSpec) -> Result<CouplingReport, ProjectionError> {
    let mut spec = spec.clone();
    if spec.level == AggregationLevel::Symbol {
        spec.level = AggregationLevel::Namespace;
    }
    let p = project(graph, &spec)?;
    let n = p.len();
    let ca: Vec<u32> = (0..n as Ix).map(|i| p.incoming(i).len() as u32).collect();
    let ce: Vec<u32> = (0..n as Ix).map(|i| p.outgoing(i).len() as u32).collect();
    let instability: Vec<f64> = (0..n)
        .map(|i| match ca[i] + ce[i] {
            0 => 0.0,
            t => f64::from(ce[i]) / f64::from(t),
        })
        .collect();
    let abstractness = abstractness(graph, &p, spec.level);
    let comps = scc::components(&p);
    let layer = layers(&p, &comps);
    let mut violations = Vec::new();
    for comp in &comps {
        let (cut, _) = scc::cut(&p, comp);
        for (f, t, _) in cut {
            violations.push(Violation {
                from: p.id(f).clone(),
                to: p.id(t).clone(),
                kind: "upward".into(),
            });
        }
    }
    for f in 0..n as Ix {
        for a in p.outgoing(f) {
            // A stabler module depending on a clearly less stable one.
            if instability[f as usize] + 1e-9 < instability[a.other as usize] {
                violations.push(Violation {
                    from: p.id(f).clone(),
                    to: p.id(a.other).clone(),
                    kind: "unstable-dependency".into(),
                });
            }
        }
    }
    violations.sort_by(|a, b| (&a.kind, &a.from, &a.to).cmp(&(&b.kind, &b.from, &b.to)));
    let modules = (0..n)
        .map(|i| Coupling {
            id: p.id(i as Ix).clone(),
            afferent: ca[i],
            efferent: ce[i],
            instability: instability[i],
            abstractness: abstractness[i],
            distance: (abstractness[i] + instability[i] - 1.0).abs(),
            layer: layer[i],
        })
        .collect();
    Ok(CouplingReport {
        projection: spec,
        modules,
        violations,
    })
}

/// Share of abstract definitions (protocols, multimethods, interfaces)
/// among the symbols each module contains.
///
/// Example: a namespace with one protocol and three functions has 0.25.
fn abstractness(graph: &Graph, p: &Projection, level: AggregationLevel) -> Vec<f64> {
    let mut total = vec![0u32; p.len()];
    let mut abs = vec![0u32; p.len()];
    let index: std::collections::HashMap<&NodeId, usize> =
        p.ids().iter().enumerate().map(|(i, id)| (id, i)).collect();
    for node in graph.nodes.values().filter(|n| n.kind == NodeKind::Symbol) {
        let owner = match level {
            AggregationLevel::Unit => graph.unit_of(&node.id),
            _ => graph.namespace_of(&node.id),
        };
        let Some(&i) = owner.as_ref().and_then(|o| index.get(o)) else {
            continue;
        };
        total[i] += 1;
        if matches!(
            node.subkind,
            Some(SymbolSubkind::Protocol | SymbolSubkind::Multimethod | SymbolSubkind::Interface)
        ) {
            abs[i] += 1;
        }
    }
    (0..p.len())
        .map(|i| {
            if total[i] == 0 {
                0.0
            } else {
                f64::from(abs[i]) / f64::from(total[i])
            }
        })
        .collect()
}

/// Layer of each node: the longest path from it to a sink in the
/// condensation (members of a cycle share their component's layer).
///
/// Example: `a → b → c` gives layers `[2, 1, 0]`.
pub fn layers(p: &Projection, comps: &[Vec<Ix>]) -> Vec<u32> {
    let n = p.len();
    let mut comp_of: Vec<usize> = (0..n).collect();
    for (ci, comp) in comps.iter().enumerate() {
        for &v in comp {
            comp_of[v as usize] = n + ci;
        }
    }
    // Memoized longest path over components, iterative DFS.
    let mut layer_of: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
    let members = |c: usize| -> Vec<Ix> {
        if c >= n {
            comps[c - n].clone()
        } else {
            vec![c as Ix]
        }
    };
    for start in 0..n {
        let c0 = comp_of[start];
        if layer_of.contains_key(&c0) {
            continue;
        }
        let mut stack: Vec<(usize, bool)> = vec![(c0, false)];
        while let Some((c, done)) = stack.pop() {
            if layer_of.contains_key(&c) {
                continue;
            }
            let succ: Vec<usize> = members(c)
                .iter()
                .flat_map(|&v| p.outgoing(v).iter().map(|a| comp_of[a.other as usize]))
                .filter(|&d| d != c)
                .collect();
            if done {
                let l = succ
                    .iter()
                    .map(|d| layer_of.get(d).copied().unwrap_or(0) + 1)
                    .max()
                    .unwrap_or(0);
                layer_of.insert(c, l);
            } else {
                stack.push((c, true));
                stack.extend(
                    succ.into_iter()
                        .filter(|d| !layer_of.contains_key(d))
                        .map(|d| (d, false)),
                );
            }
        }
    }
    (0..n)
        .map(|v| layer_of.get(&comp_of[v]).copied().unwrap_or(0))
        .collect()
}
