//! Where Deckly keeps its files. The same rule as the app's `config_dir`
//! (`crates/deckly/src/store.rs`), copied because the tool must not depend
//! on the app; the table is in `docs/sdk/reference/folders.md`. Every path
//! comes from this rule, never from a fixed folder.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Deckly's settings folder: `--config-dir`, else `DECKLY_CONFIG_DIR`, else
/// the system's rule. A folder that does not exist yet is not an error.
pub fn config_dir(over: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(dir) = over {
        return Ok(dir.to_owned());
    }
    rule(|name| std::env::var_os(name), Os::current())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Other,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Other
        }
    }
}

fn rule(var: impl Fn(&str) -> Option<OsString>, os: Os) -> Result<PathBuf, String> {
    let env = |name: &str| var(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(dir) = env("DECKLY_CONFIG_DIR") {
        return Ok(dir);
    }
    let missing = |name: &str| {
        format!("cannot find Deckly's settings folder: {name} is not set; pass --config-dir")
    };
    Ok(match os {
        Os::Windows => env("APPDATA")
            .ok_or_else(|| missing("APPDATA"))?
            .join("Deckly"),
        Os::Mac => env("HOME")
            .ok_or_else(|| missing("HOME"))?
            .join("Library/Application Support/Deckly"),
        Os::Other => env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .ok_or_else(|| missing("HOME"))?
            .join("deckly"),
    })
}

/// The log of a plugin, as Deckly writes it.
pub fn log_file(config: &Path, id: &str) -> PathBuf {
    config
        .join("logs")
        .join("plugins")
        .join(format!("{id}.log"))
}

/// The folder of installed plugins.
pub fn plugins_dir(config: &Path) -> PathBuf {
    config.join("plugins")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            vars.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    /// The table in docs/sdk/reference/folders.md.
    #[test]
    fn follows_the_folders_table() {
        let home = [("HOME", "/home/ada")];
        assert_eq!(
            rule(with(&home), Os::Other),
            Ok(PathBuf::from("/home/ada/.config/deckly"))
        );
        let xdg = [("HOME", "/home/ada"), ("XDG_CONFIG_HOME", "/xdg")];
        assert_eq!(
            rule(with(&xdg), Os::Other),
            Ok(PathBuf::from("/xdg/deckly"))
        );
        assert_eq!(
            rule(with(&[("HOME", "/Users/ada")]), Os::Mac),
            Ok(PathBuf::from(
                "/Users/ada/Library/Application Support/Deckly"
            ))
        );
        assert_eq!(
            rule(
                with(&[("APPDATA", "C:/Users/ada/AppData/Roaming")]),
                Os::Windows
            ),
            Ok(PathBuf::from("C:/Users/ada/AppData/Roaming/Deckly"))
        );
        let custom = [("DECKLY_CONFIG_DIR", "/tmp/d"), ("HOME", "/home/ada")];
        assert_eq!(rule(with(&custom), Os::Other), Ok(PathBuf::from("/tmp/d")));
        // An empty variable counts as unset.
        let empty = [("DECKLY_CONFIG_DIR", ""), ("HOME", "/h")];
        assert_eq!(
            rule(with(&empty), Os::Other),
            Ok(PathBuf::from("/h/.config/deckly"))
        );
    }

    #[test]
    fn a_missing_variable_is_named_and_never_guessed() {
        let e = rule(with(&[]), Os::Windows).unwrap_err();
        assert!(e.contains("APPDATA") && e.contains("--config-dir"), "{e}");
        assert!(rule(with(&[]), Os::Other).unwrap_err().contains("HOME"));
    }
}
