//! The SQLite snapshot (`02-architecture.md`, "Snapshots and the cache").
//!
//! One file per key, `snapshots/<hex>.db`, written in one transaction to a
//! temporary file and renamed into place. A file whose schema version differs
//! from [`SCHEMA_VERSION`], or that cannot be read, is treated as absent.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aldente_adapter::LiteralFact;
use aldente_model::{BlindSpot, Diagnostic, Edge, Node, NodeId, RepoPath, Site, UnitInfo};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use thiserror::Error;

use crate::build::Graph;

/// Bump when the tables or the meaning of stored data change.
pub const SCHEMA_VERSION: &str = "1";

/// A stored analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// `b3:<hex>`.
    pub key: String,
    /// Where it came from: the URL, or the canonical local path.
    pub repo: String,
    /// The commit checked out, when the target is a git repository.
    pub rev: Option<String>,
    /// Whether a `.git` was found.
    pub git: bool,
    /// Al Dente's version.
    pub analyzer: String,
    pub units: Vec<UnitInfo>,
    pub graph: Graph,
}

/// Why a snapshot could not be written or read.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("bad stored value: {0}")]
    Bad(String),
}

const TABLES: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE unit (id INTEGER PRIMARY KEY, dir TEXT NOT NULL UNIQUE, json TEXT NOT NULL);
CREATE TABLE node (id INTEGER PRIMARY KEY, node_id TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, json TEXT NOT NULL);
CREATE TABLE edge (id INTEGER PRIMARY KEY, src INTEGER NOT NULL REFERENCES node(id),
                   dst INTEGER NOT NULL REFERENCES node(id), kind TEXT NOT NULL,
                   certainty TEXT, reason TEXT, lang TEXT);
CREATE TABLE site (id INTEGER PRIMARY KEY, edge INTEGER NOT NULL REFERENCES edge(id),
                   file TEXT NOT NULL, line INTEGER NOT NULL, col INTEGER NOT NULL);
CREATE TABLE file (path TEXT PRIMARY KEY);
CREATE TABLE literal (id INTEGER PRIMARY KEY, file TEXT NOT NULL, line INTEGER NOT NULL,
                      col INTEGER NOT NULL, kind TEXT NOT NULL, text TEXT NOT NULL);
CREATE TABLE diagnostic (id INTEGER PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE blindspot (id INTEGER PRIMARY KEY, category TEXT NOT NULL, json TEXT NOT NULL);
CREATE TABLE analysis_result (name TEXT NOT NULL, params TEXT NOT NULL, json TEXT NOT NULL,
                              PRIMARY KEY (name, params));
CREATE INDEX edge_src ON edge(src);
CREATE INDEX edge_dst ON edge(dst);
CREATE INDEX site_edge ON site(edge);
";

/// The file for a key: `<dir>/<hex>.db`.
///
/// Example: `path_for(dir, "b3:abcd")` is `dir/abcd.db`.
pub fn path_for(dir: &Path, key: &str) -> PathBuf {
    dir.join(format!("{}.db", key.strip_prefix("b3:").unwrap_or(key)))
}

/// Writes `snap` to `path` atomically (temporary file, one transaction, rename).
///
/// Example: `write(&path_for(dir, &snap.key), &snap)` creates `dir/<hex>.db`.
pub fn write(path: &Path, snap: &Snapshot) -> Result<(), StoreError> {
    let tmp = path.with_extension(format!("db.tmp-{}", std::process::id()));
    if tmp.exists() {
        std::fs::remove_file(&tmp)?;
    }
    let result = write_file(&tmp, snap);
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return result;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Writes every table into a new database at `path`.
///
/// Example: see [`write`].
fn write_file(path: &Path, snap: &Snapshot) -> Result<(), StoreError> {
    let mut conn = Connection::open(path)?;
    let tx = conn.transaction()?;
    tx.execute_batch(TABLES)?;
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let meta = [
        ("schema_version", SCHEMA_VERSION.to_owned()),
        ("key", snap.key.clone()),
        ("repo", snap.repo.clone()),
        ("rev", snap.rev.clone().unwrap_or_default()),
        ("git", snap.git.to_string()),
        ("analyzer", snap.analyzer.clone()),
        ("created", created.to_string()),
    ];
    for (k, v) in meta {
        tx.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)",
            params![k, v],
        )?;
    }
    for unit in &snap.units {
        tx.execute(
            "INSERT INTO unit (dir, json) VALUES (?1, ?2)",
            params![unit.dir.as_str(), serde_json::to_string(unit)?],
        )?;
    }
    let mut rowid: BTreeMap<&NodeId, i64> = BTreeMap::new();
    for (i, node) in snap.graph.nodes.values().enumerate() {
        let id = i as i64 + 1;
        tx.execute(
            "INSERT INTO node (id, node_id, kind, json) VALUES (?1, ?2, ?3, ?4)",
            params![
                id,
                node.id.to_string(),
                token(&node.kind)?,
                serde_json::to_string(node)?
            ],
        )?;
        rowid.insert(&node.id, id);
    }
    for (i, edge) in snap.graph.edges.iter().enumerate() {
        let id = i as i64 + 1;
        let (Some(src), Some(dst)) = (rowid.get(&edge.from), rowid.get(&edge.to)) else {
            return Err(StoreError::Bad(format!(
                "edge to a missing node: {} -> {}",
                edge.from, edge.to
            )));
        };
        tx.execute(
            "INSERT INTO edge (id, src, dst, kind, certainty, reason, lang) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                src,
                dst,
                token(&edge.kind)?,
                edge.certainty.as_ref().map(token).transpose()?,
                edge.reason.as_ref().map(token).transpose()?,
                edge.lang.as_ref().map(token).transpose()?,
            ],
        )?;
        for site in &edge.sites {
            tx.execute(
                "INSERT INTO site (edge, file, line, col) VALUES (?1, ?2, ?3, ?4)",
                params![id, site.file.as_str(), site.line, site.col],
            )?;
        }
    }
    for l in &snap.graph.literals {
        tx.execute(
            "INSERT INTO literal (file, line, col, kind, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![l.file.as_str(), l.line, l.col, l.kind, l.text],
        )?;
    }
    for d in &snap.graph.diagnostics {
        tx.execute(
            "INSERT INTO diagnostic (json) VALUES (?1)",
            params![serde_json::to_string(d)?],
        )?;
    }
    for b in &snap.graph.blindspots {
        tx.execute(
            "INSERT INTO blindspot (category, json) VALUES (?1, ?2)",
            params![token(&b.category)?, serde_json::to_string(b)?],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Reads a snapshot. `Ok(None)` when the file is missing, has another schema
/// version, or is not a snapshot at all.
///
/// Example: `load(&path_for(dir, key))` after [`write`] gives the same snapshot.
pub fn load(path: &Path) -> Result<Option<Snapshot>, StoreError> {
    if !path.is_file() {
        return Ok(None);
    }
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Ok(None);
    };
    let Ok(Some(version)) = meta(&conn, "schema_version") else {
        return Ok(None);
    };
    if version != SCHEMA_VERSION {
        return Ok(None);
    }
    let get = |k: &str| -> Result<String, StoreError> {
        meta(&conn, k)?.ok_or_else(|| StoreError::Bad(format!("meta {k} missing")))
    };
    let rev = get("rev")?;
    let mut snap = Snapshot {
        key: get("key")?,
        repo: get("repo")?,
        rev: (!rev.is_empty()).then_some(rev),
        git: get("git")? == "true",
        analyzer: get("analyzer")?,
        units: Vec::new(),
        graph: Graph::default(),
    };
    for json in column(&conn, "SELECT json FROM unit ORDER BY id")? {
        snap.units.push(serde_json::from_str(&json)?);
    }
    let mut by_rowid: BTreeMap<i64, NodeId> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT id, json FROM node ORDER BY id")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))? {
        let (id, json) = row?;
        let node: Node = serde_json::from_str(&json)?;
        by_rowid.insert(id, node.id.clone());
        snap.graph.nodes.insert(node.id.clone(), node);
    }
    snap.graph.edges = load_edges(&conn, &by_rowid)?;
    let mut stmt = conn.prepare("SELECT file, line, col, kind, text FROM literal ORDER BY id")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?,
            r.get::<_, u32>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (file, line, col, kind, text) = row?;
        snap.graph.literals.push(LiteralFact {
            file: RepoPath::parse(&file).map_err(|e| StoreError::Bad(e.to_string()))?,
            line,
            col,
            kind: literal_kind(&kind)?,
            text,
        });
    }
    for json in column(&conn, "SELECT json FROM diagnostic ORDER BY id")? {
        snap.graph
            .diagnostics
            .push(serde_json::from_str::<Diagnostic>(&json)?);
    }
    for json in column(&conn, "SELECT json FROM blindspot ORDER BY id")? {
        snap.graph
            .blindspots
            .push(serde_json::from_str::<BlindSpot>(&json)?);
    }
    Ok(Some(snap))
}

/// Edges with their sites, in stored order.
///
/// Example: see [`load`].
fn load_edges(conn: &Connection, nodes: &BTreeMap<i64, NodeId>) -> Result<Vec<Edge>, StoreError> {
    let mut sites: BTreeMap<i64, Vec<Site>> = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT edge, file, line, col FROM site ORDER BY id")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, u32>(2)?,
            r.get::<_, u32>(3)?,
        ))
    })? {
        let (edge, file, line, col) = row?;
        let file = RepoPath::parse(&file).map_err(|e| StoreError::Bad(e.to_string()))?;
        sites
            .entry(edge)
            .or_default()
            .push(Site { file, line, col });
    }
    let mut edges = Vec::new();
    let mut stmt =
        conn.prepare("SELECT id, src, dst, kind, certainty, reason, lang FROM edge ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
        ))
    })?;
    for row in rows {
        let (id, src, dst, kind, certainty, reason, lang) = row?;
        let node = |n: i64| {
            nodes
                .get(&n)
                .cloned()
                .ok_or_else(|| StoreError::Bad(format!("edge {id} names node row {n}")))
        };
        edges.push(Edge {
            from: node(src)?,
            to: node(dst)?,
            kind: untoken(&kind)?,
            certainty: certainty.as_deref().map(untoken).transpose()?,
            reason: reason.as_deref().map(untoken).transpose()?,
            lang: lang.as_deref().map(untoken).transpose()?,
            sites: sites.remove(&id).unwrap_or_default(),
            count: None,
            by_certainty: None,
        });
    }
    Ok(edges)
}

/// One `meta` value.
///
/// Example: `meta(&conn, "schema_version")` is `Ok(Some("1"))`.
fn meta(conn: &Connection, key: &str) -> Result<Option<String>, StoreError> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
            r.get(0)
        })
        .optional()?)
}

/// The first column of every row of `sql`.
///
/// Example: `column(&conn, "SELECT json FROM unit ORDER BY id")`.
fn column(conn: &Connection, sql: &str) -> Result<Vec<String>, StoreError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The JSON string form of a model enum value.
///
/// Example: `token(&EdgeKind::Calls)` is `Ok("calls")`.
fn token<T: serde::Serialize>(value: &T) -> Result<String, StoreError> {
    match serde_json::to_value(value)? {
        serde_json::Value::String(s) => Ok(s),
        other => Err(StoreError::Bad(format!("not a string token: {other}"))),
    }
}

/// Parses a model enum value from its JSON string form.
///
/// Example: `untoken::<EdgeKind>("calls")` is `Ok(EdgeKind::Calls)`.
fn untoken<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, StoreError> {
    Ok(serde_json::from_value(serde_json::Value::String(
        s.to_owned(),
    ))?)
}

/// The static literal kind for a stored string.
///
/// Example: `literal_kind("keyword")` is `Ok("keyword")`.
fn literal_kind(s: &str) -> Result<&'static str, StoreError> {
    ["string", "keyword", "number", "regex"]
        .into_iter()
        .find(|k| *k == s)
        .ok_or_else(|| StoreError::Bad(format!("literal kind {s:?}")))
}

/// Deletes all but the newest `keep` snapshots per repository in `dir`.
/// Unreadable files and leftover temporary files are removed too. Returns
/// the removed names (as `snapshots/<file>`, sorted) and their total size.
///
/// Example: six snapshots of one repository and `keep = 5` removes the oldest.
pub fn prune(dir: &Path, keep: usize) -> Result<(Vec<String>, u64), StoreError> {
    let mut by_repo: BTreeMap<String, Vec<(u64, PathBuf)>> = BTreeMap::new();
    let mut doomed: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".db.tmp-") {
            doomed.push(path);
            continue;
        }
        if !name.ends_with(".db") {
            continue;
        }
        let info = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()
            .and_then(|c| {
                Some((
                    meta(&c, "repo").ok()??,
                    meta(&c, "created").ok()??.parse::<u64>().ok()?,
                ))
            });
        match info {
            Some((repo, created)) => by_repo.entry(repo).or_default().push((created, path)),
            None => doomed.push(path),
        }
    }
    for mut snaps in by_repo.into_values() {
        snaps.sort_by(|a, b| b.cmp(a));
        doomed.extend(snaps.into_iter().skip(keep).map(|(_, p)| p));
    }
    let mut removed = Vec::new();
    let mut bytes = 0;
    for path in doomed {
        bytes += std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        std::fs::remove_file(&path)?;
        removed.push(format!(
            "snapshots/{}",
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
    }
    removed.sort();
    Ok((removed, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::{Certainty, EdgeKind, NodeKind, Reason};
    use std::collections::BTreeSet;
    use std::str::FromStr;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aldente-snap-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample(key: &str, repo: &str) -> Snapshot {
        let mut graph = Graph::default();
        for id in ["clj:.//a/f", "ext:clj//clojure.core/map"] {
            let id = NodeId::from_str(id).unwrap();
            graph.nodes.insert(
                id.clone(),
                Node {
                    id: id.clone(),
                    kind: id.kind(),
                    subkind: None,
                    name: "x".into(),
                    namespace: None,
                    unit: None,
                    langs: BTreeSet::new(),
                    flags: BTreeSet::new(),
                    loc: None,
                    doc: Some("doc with 'quotes' and \u{1b}".into()),
                    signature: None,
                },
            );
        }
        graph.edges.push(Edge {
            from: NodeId::from_str("clj:.//a/f").unwrap(),
            to: NodeId::from_str("ext:clj//clojure.core/map").unwrap(),
            kind: EdgeKind::Calls,
            certainty: Some(Certainty::Certain),
            reason: Some(Reason::Direct),
            lang: None,
            sites: vec![
                Site {
                    file: RepoPath::parse("a.clj").unwrap(),
                    line: 2,
                    col: 3,
                },
                Site {
                    file: RepoPath::parse("a.clj").unwrap(),
                    line: 9,
                    col: 1,
                },
            ],
            count: None,
            by_certainty: None,
        });
        graph.literals.push(LiteralFact {
            file: RepoPath::parse("a.clj").unwrap(),
            line: 1,
            col: 1,
            kind: "keyword",
            text: ":k".into(),
        });
        Snapshot {
            key: key.into(),
            repo: repo.into(),
            rev: Some("abc".into()),
            git: true,
            analyzer: "0.1.0".into(),
            units: Vec::new(),
            graph,
        }
    }

    #[test]
    fn write_then_load_round_trips() {
        let dir = scratch("roundtrip");
        let snap = sample("b3:00ff", "/repo");
        let path = path_for(&dir, &snap.key);
        write(&path, &snap).unwrap();
        assert_eq!(path.file_name().unwrap(), "00ff.db");
        assert_eq!(load(&path).unwrap(), Some(snap));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        assert_eq!(
            snap_kinds(&path),
            [NodeKind::External, NodeKind::Symbol].map(|k| token(&k).unwrap())
        );
    }

    fn snap_kinds(path: &Path) -> Vec<String> {
        let conn = Connection::open(path).unwrap();
        column(&conn, "SELECT kind FROM node ORDER BY kind").unwrap()
    }

    #[test]
    fn other_schema_versions_and_garbage_are_ignored() {
        let dir = scratch("schema");
        let path = path_for(&dir, "b3:01");
        write(&path, &sample("b3:01", "/repo")).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE meta SET value = '0' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        assert_eq!(load(&path).unwrap(), None);
        std::fs::write(&path, "not sqlite").unwrap();
        assert_eq!(load(&path).unwrap(), None);
        assert_eq!(load(&dir.join("missing.db")).unwrap(), None);
    }

    #[test]
    fn prune_keeps_the_newest_per_repository() {
        let dir = scratch("prune");
        for (i, repo) in ["/a", "/a", "/a", "/b"].iter().enumerate() {
            let key = format!("b3:0{i}");
            let path = path_for(&dir, &key);
            write(&path, &sample(&key, repo)).unwrap();
            Connection::open(&path)
                .unwrap()
                .execute(
                    "UPDATE meta SET value = ?1 WHERE key = 'created'",
                    params![(100 + i).to_string()],
                )
                .unwrap();
        }
        std::fs::write(dir.join("junk.db"), "x").unwrap();
        let (removed, bytes) = prune(&dir, 2).unwrap();
        assert!(bytes > 0);
        assert_eq!(removed, ["snapshots/00.db", "snapshots/junk.db"]);
        assert!(path_for(&dir, "b3:03").exists());
    }
}
