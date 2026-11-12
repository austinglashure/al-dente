//! Reading jar listings from the user's local Maven cache (R10 rule 2).
//! Read-only: entry names come from the zip central directory; nothing is
//! decompressed, put on a classpath or loaded (`07-safety.md`).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use aldente_model::DeclaredDep;

/// Why a jar could not be listed.
#[derive(Debug, thiserror::Error)]
pub enum JarError {
    #[error("reading {0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: not a zip file this reader understands ({1})")]
    Format(PathBuf, &'static str),
}

/// The end-of-central-directory record: 22 bytes plus a comment of up to 64 KiB.
const EOCD_SIG: u32 = 0x0605_4b50;
const EOCD_LEN: usize = 22;
const MAX_COMMENT: usize = 0xFFFF;
/// A central-directory file header: 46 bytes plus name, extra and comment.
const CDH_SIG: u32 = 0x0201_4b50;
const CDH_LEN: usize = 46;
/// More entries than this, or a bigger central directory, is refused.
const MAX_ENTRIES: usize = 500_000;
const MAX_CENTRAL: usize = 256 * 1024 * 1024;

/// The local Maven cache: `~/.m2/repository`, if there is a home directory.
///
/// Example: on Linux `/home/me/.m2/repository`.
pub fn default_repository() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".m2").join("repository"))
}

/// Where a dependency's jar sits in a Maven repository, if the coordinate
/// and version are plain names (no `..`, separators or empty parts: they
/// come from an analyzed repository's manifest).
///
/// Example: `jar_path(repo, "org.clojure/core.async", "1.6.681")` is
/// `repo/org/clojure/core.async/1.6.681/core.async-1.6.681.jar`.
pub fn jar_path(repo: &Path, coord: &str, version: &str) -> Option<PathBuf> {
    let (group, artifact) = coord.split_once('/')?;
    let plain = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
    };
    let parts: Vec<&str> = group.split('.').collect();
    if !parts.iter().all(|p| plain(p)) || !plain(artifact) || !plain(version) {
        return None;
    }
    let mut path = repo.to_path_buf();
    path.extend(&parts);
    path.push(artifact);
    path.push(version);
    path.push(format!("{artifact}-{version}.jar"));
    Some(path)
}

/// Entry names of a zip file, from its central directory.
///
/// Example: a jar holding `reagent/core.cljs` gives `["reagent/core.cljs"]`.
pub fn list_jar(path: &Path) -> Result<Vec<String>, JarError> {
    let io = |e| JarError::Io(path.to_path_buf(), e);
    let bad = |why| JarError::Format(path.to_path_buf(), why);
    let mut file = File::open(path).map_err(io)?;
    let len = file.seek(SeekFrom::End(0)).map_err(io)?;
    let len = usize::try_from(len).map_err(|_| bad("too large"))?;
    if len < EOCD_LEN {
        return Err(bad("too short"));
    }
    // The record is at the end, before a comment of unknown length: read
    // the last 64 KiB + 22 bytes and search backwards for its signature.
    let tail_len = len.min(EOCD_LEN + MAX_COMMENT);
    let tail = read_at(&mut file, len - tail_len, tail_len).map_err(io)?;
    let at = (0..=tail.len() - EOCD_LEN)
        .rev()
        .find(|&i| u32_at(&tail, i) == Some(EOCD_SIG))
        .ok_or_else(|| bad("no end-of-central-directory record"))?;
    let entries = usize::from(u16_at(&tail, at + 10).ok_or_else(|| bad("truncated record"))?);
    let size = u32_at(&tail, at + 12).ok_or_else(|| bad("truncated record"))?;
    let offset = u32_at(&tail, at + 16).ok_or_else(|| bad("truncated record"))?;
    if size == u32::MAX || offset == u32::MAX || entries == usize::from(u16::MAX) {
        return Err(bad("zip64 is not supported"));
    }
    let (size, offset) = (size as usize, offset as usize);
    if size > MAX_CENTRAL || entries > MAX_ENTRIES || offset.saturating_add(size) > len {
        return Err(bad("central directory out of bounds"));
    }
    let central = read_at(&mut file, offset, size).map_err(io)?;
    let mut names = Vec::with_capacity(entries);
    let mut pos = 0;
    for _ in 0..entries {
        if u32_at(&central, pos) != Some(CDH_SIG) {
            return Err(bad("bad central directory header"));
        }
        let field = |off| u16_at(&central, pos + off).map(usize::from);
        let (Some(name_len), Some(extra), Some(comment)) = (field(28), field(30), field(32)) else {
            return Err(bad("truncated header"));
        };
        let name = central
            .get(pos + CDH_LEN..pos + CDH_LEN + name_len)
            .ok_or_else(|| bad("truncated name"))?;
        names.push(String::from_utf8_lossy(name).into_owned());
        pos += CDH_LEN + name_len + extra + comment;
    }
    Ok(names)
}

/// `n` bytes of `file` starting at `offset`.
///
/// Example: `read_at(&mut f, 0, 4)` is the zip signature of a jar.
fn read_at(file: &mut File, offset: usize, n: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset as u64))?;
    let mut buf = vec![0; n];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// A little-endian `u16` at `i`, if in bounds.
///
/// Example: `u16_at(&[1, 2], 0)` is `Some(0x0201)`.
fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

/// A little-endian `u32` at `i`, if in bounds.
///
/// Example: `u32_at(b"PK\x05\x06", 0)` is `Some(0x0605_4b50)`.
fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// Clojure namespaces in a jar listing: source files (`.clj`, `.cljs`,
/// `.cljc`) and AOT `__init.class` files, path to name with `_` as `-`.
/// `META-INF/` and top-level files (`data_readers.clj`) are not namespaces.
///
/// Example: `["my_lib/some_ns.clj"]` gives `["my-lib.some-ns"]`.
pub fn namespaces(entries: &[String]) -> Vec<String> {
    let mut out: Vec<String> = entries
        .iter()
        .filter(|e| !e.starts_with("META-INF/") && e.contains('/'))
        .filter_map(|e| {
            [".clj", ".cljs", ".cljc", "__init.class"]
                .iter()
                .find_map(|ext| e.strip_suffix(ext))
        })
        .map(|stem| stem.replace('/', ".").replace('_', "-"))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Namespace → coordinate for every declared dependency whose jar is in
/// the repository. Missing or unreadable jars are skipped: rule 3 (prefix
/// matching) still applies to them. The first coordinate (sorted) wins
/// when two jars hold the same namespace.
///
/// Example: with `core.async-1.6.681.jar` cached, `clojure.core.async` maps
/// to `org.clojure/core.async`.
pub fn jar_namespaces(repo: &Path, deps: &[DeclaredDep]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for d in deps {
        let Some(version) = &d.version else { continue };
        let Some(path) = jar_path(repo, &d.coord, version) else {
            continue;
        };
        let Ok(entries) = list_jar(&path) else {
            continue;
        };
        for ns in namespaces(&entries) {
            out.entry(ns).or_insert_with(|| d.coord.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use aldente_model::DeclaredDep;

    /// A stored (uncompressed) zip with empty entries named `names`.
    fn zip(names: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for name in names {
            let offset = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([0u8; 22]);
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(name.as_bytes());
            central.extend(0x0201_4b50u32.to_le_bytes());
            central.extend([0u8; 24]);
            central.extend((name.len() as u16).to_le_bytes());
            central.extend([0u8; 12]);
            central.extend(offset.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let cd_offset = out.len() as u32;
        out.extend(&central);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend((names.len() as u16).to_le_bytes());
        out.extend((names.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("aldente-maven-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lists_entries_and_maps_namespaces() {
        let d = tmp("list");
        let jar = d.join("x.jar");
        std::fs::write(
            &jar,
            zip(&[
                "META-INF/MANIFEST.MF",
                "reagent/core.cljs",
                "reagent/impl/util.cljc",
                "my_lib/some_ns.clj",
                "aot/thing__init.class",
                "aot/Thing.class",
                "data_readers.clj",
            ]),
        )
        .unwrap();
        let entries = list_jar(&jar).unwrap();
        assert_eq!(entries.len(), 7);
        assert_eq!(
            namespaces(&entries),
            [
                "aot.thing",
                "my-lib.some-ns",
                "reagent.core",
                "reagent.impl.util"
            ]
        );
    }

    #[test]
    fn a_fake_cache_maps_declared_deps_and_refuses_odd_coordinates() {
        let repo = tmp("repo");
        let dir = repo.join("org/clojure/core.async/1.6.681");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("core.async-1.6.681.jar"),
            zip(&[
                "clojure/core/async.clj",
                "clojure/core/async/impl/channels.clj",
            ]),
        )
        .unwrap();
        let dep = |c: &str, v: Option<&str>| DeclaredDep {
            coord: c.into(),
            version: v.map(Into::into),
        };
        let map = jar_namespaces(
            &repo,
            &[
                dep("org.clojure/core.async", Some("1.6.681")),
                dep("absent/absent", Some("1.0")),
                dep("git/dep", None),
                dep("../../etc/passwd", Some("1")),
            ],
        );
        assert_eq!(
            map.get("clojure.core.async").map(String::as_str),
            Some("org.clojure/core.async")
        );
        assert_eq!(map.len(), 2);
        assert_eq!(jar_path(&repo, "../../etc/passwd", "1"), None);
        assert_eq!(jar_path(&repo, "a/b", "../1"), None);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let d = tmp("garbage");
        let jar = d.join("bad.jar");
        std::fs::write(&jar, b"PK\x05\x06 not really").unwrap();
        assert!(list_jar(&jar).is_err());
        std::fs::write(&jar, b"").unwrap();
        assert!(list_jar(&jar).is_err());
    }
}
