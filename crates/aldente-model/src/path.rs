//! Repository-relative paths, stored with forward slashes on every platform.

use std::borrow::Cow;
use std::fmt;
use std::path::Path;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// A path relative to the repository root, `/`-separated, case preserved.
/// The root itself is `.`. Never absolute, never contains `.` or `..`
/// components or empty components.
///
/// A backslash is an ordinary character here: on Linux it is legal in file
/// names. Windows separators are converted by [`RepoPath::from_native_str`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoPath(String);

/// The separator convention of a path string coming from the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    /// Only `/` separates components.
    Unix,
    /// `\` and `/` both separate components; drive letters are absolute.
    Windows,
}

impl PathStyle {
    /// The style of the platform this binary was built for.
    #[cfg(windows)]
    pub const NATIVE: PathStyle = PathStyle::Windows;
    /// The style of the platform this binary was built for.
    #[cfg(not(windows))]
    pub const NATIVE: PathStyle = PathStyle::Unix;
}

/// Why a string is not a valid [`RepoPath`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PathError {
    #[error("path is empty")]
    Empty,
    #[error("path is absolute: {0:?}")]
    Absolute(String),
    #[error("path has an empty, `.` or `..` component: {0:?}")]
    BadComponent(String),
    #[error("path contains a NUL character")]
    Nul,
    #[error("path is not valid UTF-8")]
    NotUtf8,
}

impl RepoPath {
    /// The repository root, written `.`.
    ///
    /// Example: `RepoPath::root().as_str()` is `"."`.
    pub fn root() -> RepoPath {
        RepoPath(".".to_owned())
    }

    /// Whether this is the repository root.
    ///
    /// Example: `RepoPath::parse(".")?.is_root()` is `true`.
    pub fn is_root(&self) -> bool {
        self.0 == "."
    }

    /// The canonical string form.
    ///
    /// Example: `RepoPath::parse("src/app")?.as_str()` is `"src/app"`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses the canonical form exactly as `as_str` writes it, rejecting
    /// anything that would not round-trip.
    ///
    /// Example: `parse("src/app.clj")` is ok; `parse("./src")`, `parse("a//b")`
    /// and `parse("/etc")` are errors.
    pub fn parse(s: &str) -> Result<RepoPath, PathError> {
        if s == "." {
            return Ok(RepoPath::root());
        }
        if s.is_empty() {
            return Err(PathError::Empty);
        }
        if s.contains('\0') {
            return Err(PathError::Nul);
        }
        if s.starts_with('/') {
            return Err(PathError::Absolute(s.to_owned()));
        }
        if s.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
            return Err(PathError::BadComponent(s.to_owned()));
        }
        Ok(RepoPath(s.to_owned()))
    }

    /// Normalizes a relative path string from the OS: converts Windows
    /// separators, drops empty and `.` components, rejects `..` and absolute
    /// paths. An empty result is the root.
    ///
    /// Example: `from_native_str("services\\billing\\", PathStyle::Windows)`
    /// gives `services/billing`; `from_native_str("C:\\x", PathStyle::Windows)`
    /// is an error.
    pub fn from_native_str(s: &str, style: PathStyle) -> Result<RepoPath, PathError> {
        if s.contains('\0') {
            return Err(PathError::Nul);
        }
        let unified: Cow<'_, str> = match style {
            PathStyle::Unix => Cow::Borrowed(s),
            PathStyle::Windows => {
                if has_drive_prefix(s) {
                    return Err(PathError::Absolute(s.to_owned()));
                }
                Cow::Owned(s.replace('\\', "/"))
            }
        };
        if unified.starts_with('/') {
            return Err(PathError::Absolute(s.to_owned()));
        }
        let mut parts = Vec::new();
        for component in unified.split('/') {
            match component {
                "" | "." => {}
                ".." => return Err(PathError::BadComponent(s.to_owned())),
                c => parts.push(c),
            }
        }
        if parts.is_empty() {
            Ok(RepoPath::root())
        } else {
            Ok(RepoPath(parts.join("/")))
        }
    }

    /// Converts a relative [`Path`] from this platform.
    ///
    /// Example: on Windows, `Path::new("src\\a.clj")` gives `src/a.clj`.
    pub fn from_native(path: &Path) -> Result<RepoPath, PathError> {
        let s = path.to_str().ok_or(PathError::NotUtf8)?;
        RepoPath::from_native_str(s, PathStyle::NATIVE)
    }
}

/// Whether `s` starts with a Windows drive such as `C:`.
///
/// Example: `has_drive_prefix("C:\\x")` is `true`; `has_drive_prefix("src")` is `false`.
fn has_drive_prefix(s: &str) -> bool {
    let mut chars = s.chars();
    matches!((chars.next(), chars.next()), (Some(d), Some(':')) if d.is_ascii_alphabetic())
}

impl fmt::Display for RepoPath {
    /// Writes the canonical form, e.g. `src/app.clj`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for RepoPath {
    /// Serializes as the canonical string, e.g. `"src/app.clj"`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RepoPath {
    /// Deserializes from the canonical string, rejecting invalid paths.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        RepoPath::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for RepoPath {
    /// Names the schema `RepoPath`.
    fn schema_name() -> Cow<'static, str> {
        "RepoPath".into()
    }

    /// A string: repository-relative, forward slashes, `.` for the root.
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "Repository-relative path with forward slashes; `.` is the root."
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_paths_parse() {
        for s in [
            ".",
            "src",
            "src/app/core.clj",
            "a\\b",
            "dir with space/é.cljc",
        ] {
            assert_eq!(RepoPath::parse(s).unwrap().as_str(), s);
        }
    }

    #[test]
    fn non_canonical_paths_are_rejected() {
        for s in [
            "",
            "/etc/passwd",
            "./src",
            "a//b",
            "a/",
            "a/./b",
            "../x",
            "a/..",
            "a\0b",
        ] {
            assert!(RepoPath::parse(s).is_err(), "{s:?} should be rejected");
        }
    }

    #[test]
    fn windows_paths_normalize_to_forward_slashes() {
        let p = |s| RepoPath::from_native_str(s, PathStyle::Windows);
        assert_eq!(p("services\\billing").unwrap().as_str(), "services/billing");
        assert_eq!(p(".\\src\\app\\").unwrap().as_str(), "src/app");
        assert_eq!(p("mixed/sep\\here").unwrap().as_str(), "mixed/sep/here");
        assert!(p("").unwrap().is_root());
        assert!(p("C:\\Users\\x").is_err());
        assert!(p("\\\\server\\share").is_err());
        assert!(p("..\\escape").is_err());
    }

    #[test]
    fn unix_style_keeps_backslashes_as_characters() {
        let p = RepoPath::from_native_str("weird\\name/file", PathStyle::Unix).unwrap();
        assert_eq!(p.as_str(), "weird\\name/file");
        assert!(RepoPath::from_native_str("/abs", PathStyle::Unix).is_err());
    }
}
