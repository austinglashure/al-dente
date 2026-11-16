//! The user's config file, `<config dir>/aldente/config.toml`
//! (`02-architecture.md`, "Configuration"); `ALDENTE_CONFIG_DIR` replaces
//! `<config dir>/aldente` (tests use it). A config file inside an analyzed
//! repository is never read here.
//!
//! Read so far: `editor`, the command "Open in editor" runs, for example
//! `editor = ["emacsclient", "-n", "+{line}:{col}", "{file}"]`.

use std::path::{Path, PathBuf};

/// The directory holding `config.toml`.
///
/// Example: on Linux, `~/.config/aldente`.
fn config_dir() -> Option<PathBuf> {
    match std::env::var_os("ALDENTE_CONFIG_DIR") {
        Some(d) => Some(d.into()),
        None => dirs::config_dir().map(|d| d.join("aldente")),
    }
}

/// The `editor` command in a config file's text: `None` when not set; an
/// error when it is not a non-empty list of strings (a broken config is
/// reported, not silently ignored).
///
/// Example: `editor_in("editor = [\"code\", \"-g\", \"{file}:{line}:{col}\"]")` is
/// `Ok(Some(["code", "-g", "{file}:{line}:{col}"]))`.
fn editor_in(text: &str) -> anyhow::Result<Option<Vec<String>>> {
    let table: toml::Table = text.parse()?;
    let Some(value) = table.get("editor") else {
        return Ok(None);
    };
    let list = value
        .as_array()
        .and_then(|a| a.iter().map(|v| v.as_str().map(str::to_owned)).collect::<Option<Vec<_>>>())
        .filter(|l| !l.is_empty())
        .ok_or_else(|| anyhow::anyhow!("`editor` must be a non-empty list of strings, e.g. [\"emacsclient\", \"-n\", \"+{{line}}:{{col}}\", \"{{file}}\"]"))?;
    Ok(Some(list))
}

/// The editor command from the user's config file, if one is set.
///
/// Example: with no config file, `Ok(None)`.
pub fn editor() -> anyhow::Result<Option<Vec<String>>> {
    let Some(dir) = config_dir() else {
        return Ok(None);
    };
    editor_from(&dir.join("config.toml"))
}

/// [`editor`] for one config file path.
///
/// Example: a missing file gives `Ok(None)`.
fn editor_from(path: &Path) -> anyhow::Result<Option<Vec<String>>> {
    match std::fs::read_to_string(path) {
        Ok(text) => editor_in(&text).map_err(|e| anyhow::anyhow!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_editor_command() {
        let e = editor_in(r#"editor = ["emacsclient", "-n", "+{line}:{col}", "{file}"]"#).unwrap();
        assert_eq!(e.unwrap(), ["emacsclient", "-n", "+{line}:{col}", "{file}"]);
        assert_eq!(editor_in("other = 1").unwrap(), None);
    }

    #[test]
    fn a_broken_editor_setting_is_an_error() {
        assert!(
            editor_in(r#"editor = "emacsclient {file}""#).is_err(),
            "a string would need a shell"
        );
        assert!(editor_in("editor = []").is_err());
        assert!(editor_in("editor = [1]").is_err());
        assert!(editor_in("editor = [").is_err());
    }

    #[test]
    fn a_missing_file_is_no_editor() {
        assert_eq!(
            editor_from(Path::new("/no/such/aldente/config.toml")).unwrap(),
            None
        );
    }
}
