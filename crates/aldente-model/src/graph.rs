//! Nodes, edges and graph payloads as they appear in JSON.
//! See "JSON shapes" in `docs/plan/03-graph-model.md`.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::certainty::{Certainty, Reason};
use crate::id::NodeId;
use crate::kinds::{EdgeKind, Lang, NodeKind, SymbolFlag, SymbolSubkind};
use crate::path::RepoPath;

/// A source range. Lines and columns are 1-based.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct Location {
    pub file: RepoPath,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// One place in the source where an edge occurs, e.g. a call site.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct Site {
    pub file: RepoPath,
    pub line: u32,
    pub col: u32,
}

/// A graph node. Optional fields are omitted from JSON when absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subkind: Option<SymbolSubkind>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<NodeId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub langs: BTreeSet<Lang>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub flags: BTreeSet<SymbolFlag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loc: Option<Location>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// For an aggregate (unit, namespace) in a graph payload: how many
    /// children it contains, which is what `/expand` pages through. Never
    /// stored in a snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<u64>,
    /// For a unit in a graph payload: how many symbols and impls it has
    /// (the service overview sizes units by it). Never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbols: Option<u64>,
}

/// A graph edge. Distinct edges differ in `(from, to, kind, reason)`; call
/// sites hang off the edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certainty: Option<Certainty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
    /// The language the edge was resolved in, when it differs from the source
    /// node's (e.g. the `cljs` branch of a `.cljc` reader conditional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<Lang>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<Site>,
    /// Rolled-up edges only: how many stored edges this one stands for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// Rolled-up edges only: member edges per certainty. `certainty` is the
    /// strongest present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_certainty: Option<BTreeMap<Certainty, u64>>,
}

/// Why an edge breaks a model invariant.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EdgeError {
    #[error("{kind:?} edge {from} -> {to} needs both a certainty and a reason")]
    MissingCertainty {
        kind: EdgeKind,
        from: String,
        to: String,
    },
}

impl Edge {
    /// Checks that certainty-bearing kinds carry both a certainty and a reason.
    ///
    /// Example: a `calls` edge with `certainty: None` is an error; a `contains`
    /// edge without one is fine.
    pub fn validate(&self) -> Result<(), EdgeError> {
        if self.kind.needs_certainty() && (self.certainty.is_none() || self.reason.is_none()) {
            return Err(EdgeError::MissingCertainty {
                kind: self.kind,
                from: self.from.to_string(),
                to: self.to.to_string(),
            });
        }
        Ok(())
    }
}

/// Which level a graph was aggregated to.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AggregationLevel {
    Unit,
    Namespace,
    Symbol,
}

/// How nodes were grouped when a result was cut to fit the payload cap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Aggregation {
    pub level: AggregationLevel,
}

/// A graph result: `{ nodes, edges, truncated, aggregated }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(transform = crate::schema::all_keys_required)]
pub struct GraphPayload {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub truncated: bool,
    pub aggregated: Option<Aggregation>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edge(kind: EdgeKind, certainty: Option<Certainty>, reason: Option<Reason>) -> Edge {
        Edge {
            from: "clj:.//app.orders/submit!".parse().unwrap(),
            to: "clj:.//app.pay/charge".parse().unwrap(),
            kind,
            certainty,
            reason,
            lang: None,
            sites: vec![],
            count: None,
            by_certainty: None,
        }
    }

    #[test]
    fn certainty_bearing_edges_need_certainty_and_reason() {
        assert!(edge(EdgeKind::Calls, None, None).validate().is_err());
        assert!(
            edge(EdgeKind::Calls, Some(Certainty::Certain), None)
                .validate()
                .is_err()
        );
        assert!(
            edge(EdgeKind::Calls, None, Some(Reason::Direct))
                .validate()
                .is_err()
        );
        assert!(
            edge(
                EdgeKind::Calls,
                Some(Certainty::Certain),
                Some(Reason::Direct)
            )
            .validate()
            .is_ok()
        );
        assert!(edge(EdgeKind::Contains, None, None).validate().is_ok());
    }

    #[test]
    fn plan_node_example_round_trips() {
        let value = json!({
            "id": "clj:.//app.orders/submit!", "kind": "symbol", "subkind": "fn",
            "name": "submit!", "namespace": "app.orders", "unit": "unit:.",
            "langs": ["clj"], "flags": ["entry-point"],
            "loc": { "file": "src/app/orders.clj", "line": 41, "col": 1, "end_line": 58, "end_col": 30 },
            "doc": "Submits an order.", "signature": "[order] [order opts]"
        });
        let node: Node = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(node.flags, BTreeSet::from([SymbolFlag::EntryPoint]));
        assert_eq!(serde_json::to_value(&node).unwrap(), value);
    }

    #[test]
    fn plan_edge_example_round_trips() {
        let value = json!({
            "from": "clj:.//app.orders/submit!", "to": "clj:.//app.pay/charge",
            "kind": "dispatches", "certainty": "possible", "reason": "multimethod-dispatch",
            "sites": [ { "file": "src/app/orders.clj", "line": 50, "col": 5 } ]
        });
        let edge: Edge = serde_json::from_value(value.clone()).unwrap();
        assert!(edge.validate().is_ok());
        assert_eq!(serde_json::to_value(&edge).unwrap(), value);
    }

    #[test]
    fn empty_payload_has_the_plan_shape() {
        assert_eq!(
            serde_json::to_value(GraphPayload::default()).unwrap(),
            json!({ "nodes": [], "edges": [], "truncated": false, "aggregated": null })
        );
    }

    #[test]
    fn bad_paths_and_ids_are_rejected_on_input() {
        let bad_path = json!({ "file": "../etc/passwd", "line": 1, "col": 1 });
        assert!(serde_json::from_value::<Site>(bad_path).is_err());
        let bad_id = json!({ "from": "nope", "to": "unit:.", "kind": "contains" });
        assert!(serde_json::from_value::<Edge>(bad_id).is_err());
    }
}
