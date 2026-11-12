//! Running clj-kondo safely and reading its analysis output
//! (`04-clojure-analysis.md`, "Running clj-kondo safely").
//!
//! clj-kondo gets exactly the files the walker kept (never whole roots, which
//! could reach through symlinks or into ignored files), an empty
//! `--config-dir` so the target's `.clj-kondo` config and hooks never load,
//! the cache off, a timeout, and a temp working directory.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use aldente_acquire::process::{RunError, run_command};
use aldente_model::{Diagnostic, DiagnosticCode, RepoPath};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// The inline `--config` from the plan, with JSON output.
pub const CONFIG: &str = "{:output {:format :json} \
 :analysis {:arglists true :keywords true :locals true :protocol-impls true \
 :java-class-usages true :java-class-definitions true :java-member-definitions true \
 :instance-invocations true :context [:re-frame.core] \
 :var-definitions {:meta true} :namespace-definitions {:meta true}}}";

/// Buckets the resolver relies on; the probe test asserts each is present.
pub const BUCKETS: &[&str] = &[
    "instance-invocations",
    "java-class-definitions",
    "java-class-usages",
    "java-member-definitions",
    "keywords",
    "local-usages",
    "locals",
    "namespace-definitions",
    "namespace-usages",
    "protocol-impls",
    "var-definitions",
    "var-usages",
];

/// Per-unit time limit.
pub const TIMEOUT: Duration = Duration::from_secs(300);

/// Roughly how many characters of file arguments go into one invocation, to
/// stay far below Windows' 32k command-line limit.
const CHUNK_CHARS: usize = 8000;

/// A `def`-like definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VarDefinition {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub end_row: Option<u32>,
    pub end_col: Option<u32>,
    pub name_row: Option<u32>,
    pub name_col: Option<u32>,
    pub ns: String,
    pub name: String,
    pub defined_by: Option<String>,
    #[serde(rename = "defined-by->lint-as")]
    pub defined_by_lint_as: Option<String>,
    pub fixed_arities: Option<Vec<u32>>,
    pub varargs_min_arity: Option<u32>,
    pub arglist_strs: Option<Vec<String>>,
    pub doc: Option<String>,
    pub meta: Option<Value>,
    pub private: Option<bool>,
    pub deprecated: Option<Value>,
    #[serde(rename = "macro")]
    pub is_macro: Option<bool>,
    pub test: Option<bool>,
    pub protocol_name: Option<String>,
    pub protocol_ns: Option<String>,
    pub lang: Option<String>,
}

/// A use of a var.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VarUsage {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub end_row: Option<u32>,
    pub end_col: Option<u32>,
    pub name_row: Option<u32>,
    pub name_col: Option<u32>,
    pub name: String,
    /// The namespace the usage is in.
    pub from: String,
    /// The namespace of the used var (`clj-kondo/unknown-namespace` if
    /// unresolved); absent for JS interop such as `js/console.log`.
    pub to: Option<String>,
    pub from_var: Option<String>,
    #[serde(rename = "macro")]
    pub is_macro: Option<bool>,
    pub arity: Option<u32>,
    pub alias: Option<String>,
    pub defmethod: Option<bool>,
    pub dispatch_val_str: Option<String>,
    pub lang: Option<String>,
}

/// An `ns` form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NamespaceDefinition {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub end_row: Option<u32>,
    pub end_col: Option<u32>,
    pub name: String,
    pub doc: Option<String>,
    pub meta: Option<Value>,
    pub lang: Option<String>,
}

/// A namespace required by another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct NamespaceUsage {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub from: String,
    pub to: String,
    pub alias: Option<String>,
    pub lang: Option<String>,
}

/// A protocol method implemented in `defrecord`, `extend-type`, `reify`, ….
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProtocolImpl {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub end_row: Option<u32>,
    pub end_col: Option<u32>,
    pub impl_ns: Option<String>,
    pub method_name: String,
    pub protocol_name: String,
    pub protocol_ns: String,
    pub defined_by: Option<String>,
    pub lang: Option<String>,
}

/// A local binding or a use of one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Local {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub id: u64,
    pub name: String,
    pub lang: Option<String>,
}

/// A keyword occurrence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Keyword {
    pub filename: String,
    pub row: u32,
    pub col: u32,
    pub name: String,
    pub ns: Option<String>,
    pub from: Option<String>,
    pub from_var: Option<String>,
    pub reg: Option<String>,
    pub lang: Option<String>,
}

/// A Java class used in code or imports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct JavaClassUsage {
    pub filename: String,
    pub row: Option<u32>,
    pub col: Option<u32>,
    pub class: String,
    pub method_name: Option<String>,
    pub call: Option<bool>,
    pub import: Option<bool>,
    pub lang: Option<String>,
}

/// A method call on an instance, `(.method x)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InstanceInvocation {
    pub filename: String,
    pub name_row: u32,
    pub name_col: u32,
    pub method_name: String,
    pub lang: Option<String>,
}

/// clj-kondo's `analysis` map. Unknown fields are ignored; unused buckets
/// are kept as raw JSON.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct Analysis {
    pub var_definitions: Vec<VarDefinition>,
    pub var_usages: Vec<VarUsage>,
    pub namespace_definitions: Vec<NamespaceDefinition>,
    pub namespace_usages: Vec<NamespaceUsage>,
    pub protocol_impls: Vec<ProtocolImpl>,
    pub locals: Vec<Local>,
    pub local_usages: Vec<Local>,
    pub keywords: Vec<Keyword>,
    pub java_class_usages: Vec<JavaClassUsage>,
    pub instance_invocations: Vec<InstanceInvocation>,
    pub java_class_definitions: Vec<Value>,
    pub java_member_definitions: Vec<Value>,
}

/// A lint finding. Only syntax and file errors are used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub filename: String,
    pub row: Option<u32>,
    pub col: Option<u32>,
    pub level: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct RawOutput {
    #[serde(default)]
    findings: Vec<Finding>,
    analysis: Option<Analysis>,
}

/// The analysis of one unit, file names repository-relative with `/`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KondoRun {
    pub analysis: Analysis,
    /// Syntax errors and unreadable files.
    pub diagnostics: Vec<Diagnostic>,
}

/// Why clj-kondo could not analyze a unit.
#[derive(Debug, Error)]
pub enum KondoError {
    #[error("clj-kondo not found at {0}")]
    Missing(String),
    #[error("clj-kondo failed (exit {code:?}): {stderr}")]
    Failed { code: Option<i32>, stderr: String },
    #[error("clj-kondo timed out after {0:?}")]
    Timeout(Duration),
    #[error("clj-kondo output could not be read: {0}")]
    BadOutput(String),
}

/// A configured clj-kondo.
#[derive(Debug, Clone)]
pub struct Kondo {
    program: PathBuf,
    empty_dir: PathBuf,
    timeout: Duration,
}

impl Kondo {
    /// clj-kondo at `program`, using `empty_dir` (which must stay empty) as
    /// config dir, home config dir and working directory.
    ///
    /// Example: `Kondo::new(which("clj-kondo"), cache.tmp.join("kondo-empty"))`.
    pub fn new(program: PathBuf, empty_dir: PathBuf) -> Kondo {
        Kondo {
            program,
            empty_dir,
            timeout: TIMEOUT,
        }
    }

    /// Analyzes `files` (repository-relative) under `root` (canonical).
    /// Lint findings are ignored except syntax and file errors, which become
    /// diagnostics; exit codes 2 and 3 ("findings exist") are success.
    ///
    /// Example: on clj-mini's 13 sources gives `var_definitions` for
    /// `shop.core/checkout` and friends, with `filename: "src/shop/core.clj"`.
    pub fn analyze(&self, root: &Path, files: &[RepoPath]) -> Result<KondoRun, KondoError> {
        std::fs::create_dir_all(&self.empty_dir).map_err(|e| KondoError::Failed {
            code: None,
            stderr: format!("cannot create {}: {e}", self.empty_dir.display()),
        })?;
        let root = simplify(root);
        let mut run = KondoRun::default();
        let mut findings = Vec::new();
        for chunk in chunks(&root, files) {
            let raw = self.run_chunk(&chunk)?;
            let analysis = raw
                .analysis
                .ok_or_else(|| KondoError::BadOutput("no analysis in output".into()))?;
            merge(&mut run.analysis, analysis);
            findings.extend(raw.findings);
        }
        normalize(&mut run.analysis, &root);
        run.diagnostics = file_diagnostics(&findings, &root);
        Ok(run)
    }

    /// Runs one invocation over `paths` and parses its JSON.
    ///
    /// Example: `run_chunk(&["/r/src/a.clj"])` gives the raw output for one file.
    fn run_chunk(&self, paths: &[PathBuf]) -> Result<RawOutput, KondoError> {
        let mut cmd = Command::new(&self.program);
        cmd.arg("--lint")
            .args(paths)
            .arg("--config-dir")
            .arg(&self.empty_dir)
            .args(["--cache", "false", "--parallel", "--config", CONFIG])
            .env("XDG_CONFIG_HOME", &self.empty_dir)
            .current_dir(&self.empty_dir);
        let out = run_command(cmd, self.timeout).map_err(|e| match e {
            RunError::Spawn(_) => KondoError::Missing(self.program.display().to_string()),
            RunError::Timeout(t) => KondoError::Timeout(t),
            RunError::Wait(e) => KondoError::Failed {
                code: None,
                stderr: e.to_string(),
            },
        })?;
        match out.status.code() {
            Some(0 | 2 | 3) => {}
            code => {
                return Err(KondoError::Failed {
                    code,
                    stderr: out.stderr.chars().take(2000).collect(),
                });
            }
        }
        serde_json::from_str(&out.stdout).map_err(|e| KondoError::BadOutput(e.to_string()))
    }
}

/// Splits files into argument lists of at most about [`CHUNK_CHARS`]
/// characters, as absolute paths.
///
/// Example: 3 files with short paths give one chunk of 3 paths.
fn chunks(root: &Path, files: &[RepoPath]) -> Vec<Vec<PathBuf>> {
    let mut out: Vec<Vec<PathBuf>> = Vec::new();
    let mut size = 0;
    for file in files {
        let path = aldente_acquire::walk::native_path(root, file);
        let len = path.as_os_str().len() + 1;
        if out.is_empty() || size + len > CHUNK_CHARS {
            out.push(Vec::new());
            size = 0;
        }
        size += len;
        if let Some(last) = out.last_mut() {
            last.push(path);
        }
    }
    out
}

/// Appends every bucket of `more` to `into`.
///
/// Example: two runs with one var definition each give two.
fn merge(into: &mut Analysis, more: Analysis) {
    into.var_definitions.extend(more.var_definitions);
    into.var_usages.extend(more.var_usages);
    into.namespace_definitions
        .extend(more.namespace_definitions);
    into.namespace_usages.extend(more.namespace_usages);
    into.protocol_impls.extend(more.protocol_impls);
    into.locals.extend(more.locals);
    into.local_usages.extend(more.local_usages);
    into.keywords.extend(more.keywords);
    into.java_class_usages.extend(more.java_class_usages);
    into.instance_invocations.extend(more.instance_invocations);
    into.java_class_definitions
        .extend(more.java_class_definitions);
    into.java_member_definitions
        .extend(more.java_member_definitions);
}

/// Rewrites every `filename` to a repository-relative `/` path, and sorts
/// every bucket so output does not depend on `--parallel` scheduling.
///
/// Example: `/r/src/a.clj` under root `/r` becomes `src/a.clj`.
fn normalize(a: &mut Analysis, root: &Path) {
    macro_rules! fix {
        ($($bucket:ident),*) => {$(
            for item in &mut a.$bucket {
                item.filename = relative(root, &item.filename);
            }
            a.$bucket.sort_by_cached_key(|x| serde_json::to_string(x).unwrap_or_default());
        )*};
    }
    fix!(
        var_definitions,
        var_usages,
        namespace_definitions,
        namespace_usages,
        protocol_impls,
        locals,
        local_usages,
        keywords,
        java_class_usages,
        instance_invocations
    );
    for bucket in [
        &mut a.java_class_definitions,
        &mut a.java_member_definitions,
    ] {
        for item in bucket.iter_mut() {
            if let Some(Value::String(f)) = item.get_mut("filename") {
                *f = relative(root, f);
            }
        }
        bucket.sort_by_cached_key(|x| x.to_string());
    }
}

/// `file` relative to `root` with `/`, or unchanged if it is not under `root`.
///
/// Example: `relative("/r", "/r/src/a.clj")` is `"src/a.clj"`.
fn relative(root: &Path, file: &str) -> String {
    Path::new(file)
        .strip_prefix(root)
        .ok()
        .and_then(|rel| RepoPath::from_native(rel).ok())
        .map_or_else(|| file.to_owned(), |p| p.as_str().to_owned())
}

/// One diagnostic per file with syntax errors (the first message) or that
/// could not be read.
///
/// Example: a file with an unclosed `(` gives `parse-error` "Found an opening ( with no matching )".
fn file_diagnostics(findings: &[Finding], root: &Path) -> Vec<Diagnostic> {
    let mut sorted: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.level == "error" && matches!(f.kind.as_str(), "syntax" | "file"))
        .collect();
    sorted.sort_by_key(|f| (f.filename.clone(), f.row, f.col));
    let mut out: Vec<Diagnostic> = Vec::new();
    for f in sorted {
        let path = RepoPath::parse(&relative(root, &f.filename)).ok();
        if out.iter().any(|d| d.path == path) {
            continue;
        }
        let code = if f.kind == "syntax" {
            DiagnosticCode::ParseError
        } else {
            DiagnosticCode::Unreadable
        };
        let at = f
            .row
            .map(|r| format!("{r}:{}: ", f.col.unwrap_or(0)))
            .unwrap_or_default();
        out.push(Diagnostic::new(path, code, format!("{at}{}", f.message)));
    }
    out
}

/// Drops Windows' `\\?\` prefix from a canonical drive path so tools that
/// do not understand it (clj-kondo) get a normal path. Other paths are
/// returned unchanged.
///
/// Example: `\\?\C:\r` gives `C:\r`; `/r` gives `/r`.
fn simplify(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_the_size_limit() {
        let files: Vec<RepoPath> = (0..1000)
            .map(|i| RepoPath::parse(&format!("src/some/longish/path/file_{i}.clj")).unwrap())
            .collect();
        let chunks = chunks(Path::new("/repo"), &files);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.iter().map(Vec::len).sum::<usize>(), 1000);
        for c in &chunks {
            let chars: usize = c.iter().map(|p| p.as_os_str().len() + 1).sum();
            assert!(chars <= CHUNK_CHARS, "{chars}");
        }
        assert!(super::chunks(Path::new("/r"), &[]).is_empty());
    }

    #[test]
    fn relative_paths_and_windows_prefixes() {
        assert_eq!(relative(Path::new("/r"), "/r/src/a.clj"), "src/a.clj");
        assert_eq!(
            relative(Path::new("/r"), "/elsewhere/a.clj"),
            "/elsewhere/a.clj"
        );
        assert_eq!(simplify(Path::new(r"\\?\C:\r")), PathBuf::from(r"C:\r"));
        assert_eq!(simplify(Path::new("/r")), PathBuf::from("/r"));
    }

    #[test]
    fn syntax_findings_become_one_diagnostic_per_file() {
        let f = |file: &str, kind: &str, row: u32, msg: &str| Finding {
            filename: format!("/r/{file}"),
            row: Some(row),
            col: Some(1),
            level: "error".into(),
            kind: kind.into(),
            message: msg.into(),
        };
        let findings = [
            f("b.clj", "syntax", 3, "second"),
            f("b.clj", "syntax", 2, "first"),
            f("a.clj", "unresolved-symbol", 1, "ignored"),
            f("c.clj", "file", 0, "file does not exist"),
        ];
        let d = file_diagnostics(&findings, Path::new("/r"));
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].code, DiagnosticCode::ParseError);
        assert_eq!(d[0].message, "2:1: first");
        assert_eq!(d[1].code, DiagnosticCode::Unreadable);
    }
}
