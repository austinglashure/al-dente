//! Unit discovery: which directories are build units, their source roots, and
//! which unit owns each source file (`02-architecture.md`, "Units").

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use aldente_model::{
    Diagnostic, DiagnosticCode, NodeId, RepoPath, RootKind, SourceRoot, UnitInfo, UnitsReport,
};

use crate::manifest::{self, ManifestInfo};
use crate::walk::{FileKind, Manifest, SourceExt, Walk, WalkedFile, native_path, read_inside};

/// A source file that will be analyzed: under a root of the unit that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzedFile {
    pub path: RepoPath,
    /// Directory of the owning unit.
    pub unit: RepoPath,
    pub ext: SourceExt,
    /// blake3 hex of the content.
    pub hash: String,
}

/// The units report plus the files each unit will analyze, sorted by path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub report: UnitsReport,
    pub analyzed: Vec<AnalyzedFile>,
    /// Every manifest that was read, with its content hash, sorted by path.
    pub manifests: Vec<(RepoPath, String)>,
}

impl Discovery {
    /// The `(path, content hash)` list that goes into the snapshot key:
    /// analyzed sources and manifests, sorted by path.
    ///
    /// Example: on clj-mini, 14 entries starting with `("deps.edn", "…")`.
    pub fn key_files(&self) -> Vec<(RepoPath, String)> {
        let mut files: Vec<_> = self
            .analyzed
            .iter()
            .map(|f| (f.path.clone(), f.hash.clone()))
            .chain(self.manifests.iter().cloned())
            .collect();
        files.sort();
        files
    }
}

/// A unit being assembled.
#[derive(Debug, Default)]
struct Draft {
    manifests: Vec<(Manifest, RepoPath)>,
    dockerfile: bool,
}

/// Builds units from a finished walk of `root` (canonical). Every problem is
/// a diagnostic; discovery itself does not fail.
///
/// Example: on clj-mini gives one unit `unit:.` with roots `src` and `test`
/// (test, alias `test`) owning 13 analyzed files.
pub fn discover(root: &Path, git: bool, walk: Walk) -> Discovery {
    let Walk {
        files,
        mut diagnostics,
    } = walk;
    let mut drafts = drafts(&files);
    let sources: Vec<&WalkedFile> = files
        .iter()
        .filter(|f| matches!(f.kind, FileKind::Source(_)))
        .collect();
    let has_orphans = sources.iter().any(|f| owner(&drafts, &f.path).is_none());
    if has_orphans {
        drafts.entry(RepoPath::root()).or_default();
    }

    let mut units = Vec::new();
    let mut analyzed = Vec::new();
    for (dir, draft) in &drafts {
        let ManifestFacts {
            roots,
            service,
            entry_points,
            deps,
        } = unit_roots(root, dir, draft, &mut diagnostics);
        let mut langs = BTreeSet::new();
        let mut count = 0;
        for file in sources
            .iter()
            .filter(|f| owner(&drafts, &f.path) == Some(dir))
        {
            let FileKind::Source(ext) = file.kind else {
                continue;
            };
            if !roots.iter().any(|r| file.path.starts_with(&r.path)) {
                diagnostics.push(Diagnostic::new(
                    Some(file.path.clone()),
                    DiagnosticCode::OutsideRoots,
                    format!("not under any source root of unit:{dir}; not analyzed"),
                ));
                continue;
            }
            langs.extend(ext.langs().iter().copied());
            count += 1;
            analyzed.push(AnalyzedFile {
                path: file.path.clone(),
                unit: dir.clone(),
                ext,
                hash: file.hash.clone(),
            });
        }
        units.push(UnitInfo {
            id: NodeId::Unit(dir.clone()),
            dir: dir.clone(),
            manifests: draft
                .manifests
                .iter()
                .map(|(m, _)| m.file_name().to_owned())
                .collect(),
            langs,
            roots,
            service: service || draft.dockerfile,
            entry_points,
            deps,
            files: count,
        });
    }
    analyzed.sort_by(|a, b| a.path.cmp(&b.path));
    diagnostics.sort();
    diagnostics.dedup();
    let manifests = files
        .iter()
        .filter(|f| matches!(f.kind, FileKind::Manifest(_)))
        .map(|f| (f.path.clone(), f.hash.clone()))
        .collect();
    Discovery {
        report: UnitsReport {
            git,
            units,
            diagnostics,
        },
        analyzed,
        manifests,
    }
}

/// One draft per directory holding a manifest, with its manifests sorted by
/// name and whether it has a `Dockerfile`.
///
/// Example: files `a/deps.edn` and `a/Dockerfile` give `{a: Draft { manifests:
/// [DepsEdn], dockerfile: true }}`.
fn drafts(files: &[WalkedFile]) -> BTreeMap<RepoPath, Draft> {
    let mut drafts: BTreeMap<RepoPath, Draft> = BTreeMap::new();
    for file in files {
        if let (FileKind::Manifest(m), Some(dir)) = (file.kind, file.path.parent()) {
            drafts
                .entry(dir)
                .or_default()
                .manifests
                .push((m, file.path.clone()));
        }
    }
    for file in files {
        if let (FileKind::Dockerfile, Some(dir)) = (file.kind, file.path.parent())
            && let Some(draft) = drafts.get_mut(&dir)
        {
            draft.dockerfile = true;
        }
    }
    for draft in drafts.values_mut() {
        draft.manifests.sort();
    }
    drafts
}

/// The nearest unit directory enclosing `file`.
///
/// Example: with units `.` and `svc`, `svc/src/a.clj` gives `svc`.
fn owner<'a>(drafts: &'a BTreeMap<RepoPath, Draft>, file: &RepoPath) -> Option<&'a RepoPath> {
    let dir = file.parent()?;
    drafts
        .keys()
        .filter(|unit| dir.starts_with(unit))
        .max_by_key(|unit| unit.components().count())
}

/// What a unit's manifests say together.
struct ManifestFacts {
    roots: Vec<SourceRoot>,
    service: bool,
    entry_points: Vec<String>,
    deps: Vec<aldente_model::DeclaredDep>,
}

/// Reads a unit's manifests and returns its sorted roots, service flag,
/// entry points and declared dependencies.
/// Falls back to `src`/`test`, or the unit directory, when no manifest
/// declares roots (including when every manifest failed to parse).
///
/// Example: a `deps.edn` of `{}` in `svc` gives roots `[svc/src]` and nothing else.
fn unit_roots(
    root: &Path,
    dir: &RepoPath,
    draft: &Draft,
    diagnostics: &mut Vec<Diagnostic>,
) -> ManifestFacts {
    let mut roots = Vec::new();
    let mut declared = false;
    let mut service = false;
    let mut entry_points = Vec::new();
    let mut deps = Vec::new();
    for (kind, path) in &draft.manifests {
        let Some(info) = read_manifest(root, *kind, path, diagnostics) else {
            continue;
        };
        declared |= info.declares_roots;
        service |= info.service;
        entry_points.extend(info.entry_points);
        deps.extend(info.deps);
        for r in info.roots {
            match dir.join_relative(&r.rel) {
                Ok(p) if r.default && !is_dir(root, &p) => {}
                Ok(p) => roots.push(SourceRoot {
                    path: p,
                    kind: r.kind,
                    manifest: Some(kind.file_name().to_owned()),
                    alias: r.alias,
                }),
                Err(_) => diagnostics.push(Diagnostic::new(
                    Some(path.clone()),
                    DiagnosticCode::RootOutsideRepo,
                    format!("source root {:?} is outside the repository; ignored", r.rel),
                )),
            }
        }
    }
    for r in &roots {
        if !is_dir(root, &r.path) {
            diagnostics.push(Diagnostic::new(
                Some(r.path.clone()),
                DiagnosticCode::MissingRoot,
                format!(
                    "declared in {} but does not exist",
                    r.manifest.as_deref().unwrap_or("?")
                ),
            ));
        }
    }
    if !declared {
        roots = fallback_roots(root, dir);
    }
    roots.sort();
    roots.dedup();
    entry_points.sort();
    entry_points.dedup();
    deps.sort();
    deps.dedup();
    ManifestFacts {
        roots,
        service,
        entry_points,
        deps,
    }
}

/// Reads and parses one manifest, recording a diagnostic on failure.
///
/// Example: a `deps.edn` containing `{:paths [` gives `None` and a
/// `manifest-error` diagnostic.
fn read_manifest(
    root: &Path,
    kind: Manifest,
    path: &RepoPath,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<ManifestInfo> {
    let parsed = read_inside(root, path, crate::walk::Limits::default().max_file_bytes)
        .map_err(|e| format!("cannot read: {e:?}"))
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "not UTF-8".to_owned()))
        .and_then(|text| manifest::parse(kind, &text));
    match parsed {
        Ok(info) => Some(info),
        Err(message) => {
            diagnostics.push(Diagnostic::new(
                Some(path.clone()),
                DiagnosticCode::ManifestError,
                format!("{message}; using fallback roots"),
            ));
            None
        }
    }
}

/// `src` and `test` under `dir` if they exist, otherwise `dir` itself.
///
/// Example: with only `app/src` present, gives `[app/src (source)]`.
fn fallback_roots(root: &Path, dir: &RepoPath) -> Vec<SourceRoot> {
    let mut roots: Vec<SourceRoot> = [("src", RootKind::Source), ("test", RootKind::Test)]
        .into_iter()
        .filter_map(|(name, kind)| {
            let path = dir.join_relative(name).ok()?;
            is_dir(root, &path).then_some(SourceRoot {
                path,
                kind,
                manifest: None,
                alias: None,
            })
        })
        .collect();
    if roots.is_empty() {
        roots.push(SourceRoot {
            path: dir.clone(),
            kind: RootKind::Source,
            manifest: None,
            alias: None,
        });
    }
    roots
}

/// Whether `path` is a real directory (not a symlink) under `root`.
///
/// Example: `is_dir(root, "src")` is `true` when `root/src` is a directory.
fn is_dir(root: &Path, path: &RepoPath) -> bool {
    native_path(root, path)
        .symlink_metadata()
        .is_ok_and(|m| m.is_dir())
}
