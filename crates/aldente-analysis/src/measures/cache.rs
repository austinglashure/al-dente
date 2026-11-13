//! Lazy measures: computed on first request and kept in the snapshot's
//! `analysis_result` table, keyed by analysis name and projection.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use aldente_graph::Graph;
use aldente_model::measures::{MeasureReport, ProjectionSpec};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{ProjectionError, first_set};

/// Where computed results are kept. Failures to read or write are not
/// errors: the result is recomputed or simply not kept.
pub trait ResultStore {
    /// The stored JSON for `(name, params)`, if any.
    fn get(&self, name: &str, params: &str) -> Option<String>;
    /// Keeps `json`; returns a message when it could not be kept.
    fn put(&self, name: &str, params: &str, json: &str) -> Result<(), String>;
}

/// A snapshot file's `analysis_result` table.
#[derive(Debug, Clone)]
pub struct SnapshotStore(pub PathBuf);

impl ResultStore for SnapshotStore {
    fn get(&self, name: &str, params: &str) -> Option<String> {
        aldente_graph::snapshot::read_result(&self.0, name, params)
            .ok()
            .flatten()
    }

    fn put(&self, name: &str, params: &str, json: &str) -> Result<(), String> {
        aldente_graph::snapshot::write_result(&self.0, name, params, json)
            .map_err(|e| e.to_string())
    }
}

/// An in-memory store (tests, and snapshots that were not saved).
#[derive(Debug, Default)]
pub struct MemoryStore(RefCell<HashMap<(String, String), String>>);

impl ResultStore for MemoryStore {
    fn get(&self, name: &str, params: &str) -> Option<String> {
        self.0
            .borrow()
            .get(&(name.to_owned(), params.to_owned()))
            .cloned()
    }

    fn put(&self, name: &str, params: &str, json: &str) -> Result<(), String> {
        self.0
            .borrow_mut()
            .insert((name.to_owned(), params.to_owned()), json.to_owned());
        Ok(())
    }
}

/// The stored result of `name` for `params`, or `compute()`'s, which is then
/// stored. A stored value that no longer parses is recomputed. The second
/// value is a warning when the result could not be kept.
///
/// Example: `cached(&store, "measures", &spec, || Ok(42))` computes once;
/// a second call reads `42` back.
pub fn cached<P: Serialize, T: Serialize + DeserializeOwned, E>(
    store: &dyn ResultStore,
    name: &str,
    params: &P,
    compute: impl FnOnce() -> Result<T, E>,
) -> Result<(T, Option<String>), E> {
    let key = serde_json::to_string(params).unwrap_or_default();
    if let Some(v) = store
        .get(name, &key)
        .and_then(|j| serde_json::from_str(&j).ok())
    {
        return Ok((v, None));
    }
    let value = compute()?;
    let warning = match serde_json::to_string(&value) {
        Ok(json) => store.put(name, &key, &json).err(),
        Err(e) => Some(e.to_string()),
    };
    Ok((value, warning))
}

/// The first set of measures for `spec`, from the store when present.
///
/// Example: `measures(&graph, &spec, &SnapshotStore(path))`.
pub fn measures(
    graph: &Graph,
    spec: &ProjectionSpec,
    store: &dyn ResultStore,
) -> Result<(MeasureReport, Option<String>), ProjectionError> {
    cached(store, "measures/first-set", spec, || first_set(graph, spec))
}
