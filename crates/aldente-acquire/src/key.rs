//! The snapshot key (`02-architecture.md`, "Snapshots and the cache").

use aldente_model::RepoPath;

/// Everything besides file contents that changes analysis results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInputs {
    /// Al Dente's version, e.g. `0.1.0`.
    pub analyzer: String,
    /// Snapshot schema version.
    pub schema: String,
    /// `(adapter name, adapter version)`; order does not matter.
    pub adapters: Vec<(String, String)>,
    /// clj-kondo's version, when it takes part.
    pub clj_kondo: Option<String>,
    /// The effective configuration in a canonical text form.
    pub config: String,
}

/// Domain separator, so these hashes never collide with other blake3 uses.
const DOMAIN: &str = "aldente snapshot key v1";

/// `b3:` plus the blake3 hex of the inputs and the `(path, content hash)`
/// list. Every field is length-prefixed so no two different inputs encode the
/// same bytes; adapters and files are sorted first, so order does not matter.
/// Paths are forward-slash `RepoPath`s and contents are hashed as stored, so
/// the key is the same on every OS for the same checkout.
///
/// Example: `snapshot_key(&inputs, &[("src/a.clj", "af13…")])` gives
/// `"b3:5c0e…"` (64 hex digits after the prefix).
pub fn snapshot_key(inputs: &KeyInputs, files: &[(RepoPath, String)]) -> String {
    let mut h = Fields(blake3::Hasher::new());
    h.text(DOMAIN);
    h.text(&inputs.analyzer);
    h.text(&inputs.schema);
    let mut adapters: Vec<_> = inputs.adapters.iter().collect();
    adapters.sort();
    h.count(adapters.len());
    for (name, version) in adapters {
        h.text(name);
        h.text(version);
    }
    match &inputs.clj_kondo {
        Some(v) => {
            h.count(1);
            h.text(v);
        }
        None => h.count(0),
    }
    h.text(&inputs.config);
    let mut files: Vec<_> = files.iter().collect();
    files.sort();
    h.count(files.len());
    for (path, hash) in files {
        h.text(path.as_str());
        h.text(hash);
    }
    format!("b3:{}", h.0.finalize().to_hex())
}

/// A hasher that writes length-prefixed fields.
struct Fields(blake3::Hasher);

impl Fields {
    /// Writes a number as 8 little-endian bytes.
    ///
    /// Example: `count(2)` writes `02 00 00 00 00 00 00 00`.
    fn count(&mut self, n: usize) {
        self.0.update(&(n as u64).to_le_bytes());
    }

    /// Writes the byte length, then the UTF-8 bytes.
    ///
    /// Example: `text("ab")` writes `02 00 00 00 00 00 00 00 61 62`.
    fn text(&mut self, s: &str) {
        self.count(s.len());
        self.0.update(s.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> KeyInputs {
        KeyInputs {
            analyzer: "0.1.0".into(),
            schema: "1".into(),
            adapters: vec![("clj".into(), "1".into())],
            clj_kondo: Some("2026.10.26".into()),
            config: String::new(),
        }
    }

    fn files(list: &[(&str, &str)]) -> Vec<(RepoPath, String)> {
        list.iter()
            .map(|(p, h)| (RepoPath::parse(p).unwrap(), (*h).to_owned()))
            .collect()
    }

    #[test]
    fn key_is_pinned() {
        // If this changes, every existing snapshot is invalidated: only change
        // it together with DOMAIN's version.
        let key = snapshot_key(
            &inputs(),
            &files(&[("src/a.clj", "00"), ("deps.edn", "11")]),
        );
        assert_eq!(
            key,
            "b3:a9f4f7665f6ddb7a1f370fd67e0f3e2ebf952ea52afe4be7f034e7637ceefed1"
        );
    }

    #[test]
    fn order_does_not_matter() {
        let a = snapshot_key(&inputs(), &files(&[("a", "1"), ("b", "2")]));
        let b = snapshot_key(&inputs(), &files(&[("b", "2"), ("a", "1")]));
        assert_eq!(a, b);
        let mut swapped = inputs();
        swapped.adapters = vec![("y".into(), "1".into()), ("x".into(), "1".into())];
        let mut sorted = inputs();
        sorted.adapters = vec![("x".into(), "1".into()), ("y".into(), "1".into())];
        assert_eq!(snapshot_key(&swapped, &[]), snapshot_key(&sorted, &[]));
    }

    #[test]
    fn every_input_changes_the_key() {
        let base = snapshot_key(&inputs(), &files(&[("a", "1")]));
        let changed = [
            KeyInputs {
                analyzer: "0.2.0".into(),
                ..inputs()
            },
            KeyInputs {
                schema: "2".into(),
                ..inputs()
            },
            KeyInputs {
                adapters: vec![],
                ..inputs()
            },
            KeyInputs {
                clj_kondo: None,
                ..inputs()
            },
            KeyInputs {
                config: "x".into(),
                ..inputs()
            },
        ];
        for c in &changed {
            assert_ne!(snapshot_key(c, &files(&[("a", "1")])), base, "{c:?}");
        }
        assert_ne!(snapshot_key(&inputs(), &files(&[("a", "2")])), base);
        assert_ne!(snapshot_key(&inputs(), &files(&[("b", "1")])), base);
        assert_ne!(snapshot_key(&inputs(), &[]), base);
    }

    #[test]
    fn field_boundaries_are_unambiguous() {
        let a = snapshot_key(&inputs(), &files(&[("ab", "c")]));
        let b = snapshot_key(&inputs(), &files(&[("a", "bc")]));
        assert_ne!(a, b);
        let none = KeyInputs {
            clj_kondo: None,
            ..inputs()
        };
        let empty = KeyInputs {
            clj_kondo: Some(String::new()),
            ..inputs()
        };
        assert_ne!(snapshot_key(&none, &[]), snapshot_key(&empty, &[]));
    }

    #[test]
    fn key_has_the_documented_shape() {
        let key = snapshot_key(&inputs(), &[]);
        let hex = key.strip_prefix("b3:").unwrap();
        assert_eq!(hex.len(), 64);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }
}
