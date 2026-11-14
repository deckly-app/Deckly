//! The plugin project in the current folder, as `cargo metadata` sees it.

use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The `cargo` that runs this tool when it runs as `cargo deckly`, else the
/// one on the PATH.
pub fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

#[derive(Clone, Debug)]
pub struct Project {
    /// The package name.
    pub name: String,
    /// The folder holding the package's `Cargo.toml`.
    pub dir: PathBuf,
    pub manifest_path: PathBuf,
    /// The program's binary target.
    pub bin: String,
    /// Cargo's target folder (it may be shared by a workspace).
    pub target_dir: PathBuf,
}

impl Project {
    /// The package in the current folder (or the folder above it); in a
    /// workspace root, the one package that uses `deckly-sdk`, or the one
    /// named by `--package`.
    pub fn find(package: Option<&str>) -> Result<Self, String> {
        let here =
            std::env::current_dir().map_err(|e| format!("cannot read the current folder: {e}"))?;
        Self::find_in(&here, package)
    }

    pub fn find_in(here: &Path, package: Option<&str>) -> Result<Self, String> {
        let output = Command::new(cargo())
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(here)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot run cargo ({e}); install Rust from https://rustup.rs"))?;
        if !output.status.success() {
            let text = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "this is not a Rust project (cargo metadata: {})",
                text.trim()
            ));
        }
        let meta: Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("cannot read cargo metadata: {e}"))?;
        pick(&meta, here, package)
    }
}

fn pick(meta: &Value, here: &Path, package: Option<&str>) -> Result<Project, String> {
    let target_dir = meta["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or("cargo metadata has no target folder")?;
    let packages = meta["packages"].as_array().cloned().unwrap_or_default();
    let dir_of = |p: &Value| {
        p["manifest_path"]
            .as_str()
            .and_then(|m| Path::new(m).parent().map(Path::to_path_buf))
    };
    let chosen = match package {
        Some(name) => packages
            .iter()
            .find(|p| p["name"] == name)
            .ok_or_else(|| format!("there is no package {name} here"))?,
        None => {
            // The deepest package folder that holds the current folder.
            let around = packages
                .iter()
                .filter_map(|p| Some((p, dir_of(p)?)))
                .filter(|(_, dir)| here.starts_with(dir) || canon(here).starts_with(canon(dir)))
                .max_by_key(|(_, dir)| dir.components().count())
                .map(|(p, _)| p);
            match around {
                Some(p) => p,
                None => {
                    let plugins: Vec<&Value> = packages.iter().filter(|p| uses_sdk(p)).collect();
                    match plugins.as_slice() {
                        [one] => *one,
                        [] => return Err("no package here uses deckly-sdk".into()),
                        _ => {
                            return Err(
                                "several packages here use deckly-sdk; pass --package <name>"
                                    .into(),
                            );
                        }
                    }
                }
            }
        }
    };
    let name = chosen["name"].as_str().unwrap_or_default().to_owned();
    let bins: Vec<&str> = chosen["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| {
            t["kind"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k == "bin"))
        })
        .filter_map(|t| t["name"].as_str())
        .collect();
    let bin = match bins.as_slice() {
        [one] => (*one).to_owned(),
        [] => return Err(format!("{name} has no program (src/main.rs)")),
        many => many
            .iter()
            .find(|b| **b == name)
            .map(|b| (*b).to_owned())
            .ok_or_else(|| format!("{name} has several programs and none named {name}"))?,
    };
    let manifest_path = chosen["manifest_path"]
        .as_str()
        .map(PathBuf::from)
        .ok_or("cargo metadata has no manifest path")?;
    let dir = dir_of(chosen).ok_or("cargo metadata has no manifest path")?;
    Ok(Project {
        name,
        dir,
        manifest_path,
        bin,
        target_dir,
    })
}

/// The path with links resolved (`/tmp` and `/private/tmp` on macOS), or
/// as it is when that fails.
fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_owned())
}

fn uses_sdk(p: &Value) -> bool {
    p["dependencies"]
        .as_array()
        .is_some_and(|d| d.iter().any(|d| d["name"] == "deckly-sdk"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn meta() -> Value {
        json!({
            "target_directory": "/w/target",
            "packages": [
                {
                    "name": "hello",
                    "manifest_path": "/w/hello/Cargo.toml",
                    "dependencies": [{"name": "deckly-sdk"}],
                    "targets": [{"name": "hello", "kind": ["bin"]}]
                },
                {
                    "name": "util",
                    "manifest_path": "/w/util/Cargo.toml",
                    "dependencies": [],
                    "targets": [{"name": "util", "kind": ["lib"]}]
                }
            ]
        })
    }

    #[test]
    fn picks_the_package_around_the_current_folder() {
        let p = pick(&meta(), Path::new("/w/hello/src"), None).unwrap();
        assert_eq!((p.name.as_str(), p.bin.as_str()), ("hello", "hello"));
        assert_eq!(p.dir, PathBuf::from("/w/hello"));
        // At the workspace root, the one that uses the SDK.
        let p = pick(&meta(), Path::new("/w"), None).unwrap();
        assert_eq!(p.name, "hello");
        assert!(pick(&meta(), Path::new("/w/util"), None).is_err());
        assert!(pick(&meta(), Path::new("/w"), Some("nope")).is_err());
    }
}
