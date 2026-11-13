//! Writing a synthetic corpus as an ordinary snapshot file, which
//! `aldente serve --snapshot FILE` opens.

use std::path::Path;

use aldente_graph::snapshot::{self, Snapshot};

use crate::synth::{Corpus, Spec};

/// The snapshot for a corpus. The key is a hash of the spec, so the same
/// spec gives the same key; `repo` says it is synthetic and how it was made.
///
/// Example: `to_snapshot(&Spec::default(), corpus).repo` is
/// `"synthetic: 20 units, 8000 namespaces, 300000 symbols, 2000000 edges, seed 1"`.
pub fn to_snapshot(spec: &Spec, corpus: Corpus) -> Snapshot {
    let hash = blake3::hash(format!("{spec:?}").as_bytes());
    Snapshot {
        key: format!("b3:{}", hash.to_hex()),
        repo: format!(
            "synthetic: {} units, {} namespaces, {} symbols, {} edges, seed {}",
            spec.units, spec.namespaces, spec.symbols, spec.edges, spec.seed
        ),
        rev: None,
        git: false,
        analyzer: env!("CARGO_PKG_VERSION").to_owned(),
        units: corpus.units,
        // No source files exist, so `/source` refuses every path.
        files: Vec::new(),
        graph: corpus.graph,
    }
}

/// Writes `snap` to `path`, creating its directory.
///
/// Example: `write(Path::new("target/bench/synthetic.db"), &snap)`.
pub fn write(path: &Path, snap: &Snapshot) -> anyhow::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    snapshot::write(path, snap)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::synthesize;

    #[test]
    fn written_snapshot_loads_back_unchanged() {
        let spec = Spec {
            units: 3,
            namespaces: 20,
            symbols: 400,
            edges: 3_000,
            hubs: 1,
            big_namespace: 50,
            seed: 4,
        };
        let snap = to_snapshot(&spec, synthesize(&spec).unwrap());
        let dir = std::env::temp_dir().join(format!("aldente-bench-out-{}", std::process::id()));
        let path = dir.join("nested/synthetic.db");
        write(&path, &snap).unwrap();
        let loaded = snapshot::load(&path).unwrap().unwrap();
        assert_eq!(loaded, snap);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
