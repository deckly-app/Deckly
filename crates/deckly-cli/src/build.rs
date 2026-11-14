//! `build`: compile the program and assemble the plugin folder,
//! `target/deckly/<plugin id>/`:
//!
//! ```text
//! deckly-plugin.json        written last, from `<program> --describe`
//! bin/<target>/<program>    one program per system, so packs can be merged
//! assets/                   a copy of the project's assets folder
//! ```
//!
//! The program is written before the manifest, so Deckly never sees a new
//! manifest with an old program.

use crate::project::{Project, cargo};
use deckly_plugin_protocol::limits::MAX_MANIFEST_BYTES;
use deckly_plugin_protocol::{MANIFEST_FILE, Manifest};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The most files and bytes copied from `assets/`.
const MAX_ASSET_FILES: usize = 10_000;
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;

pub struct Built {
    pub manifest: Manifest,
    /// The assembled plugin folder.
    pub dir: PathBuf,
}

/// Builds the program and assembles the plugin folder. Cargo's output and
/// errors go straight to the terminal.
pub fn build(project: &Project, release: bool) -> Result<Built, String> {
    let profile = if release { "release" } else { "debug" };
    let program = crate::step(&format!("cargo build ({profile})"), || {
        compile(project, release)
    })?;
    let built = crate::step("assemble the plugin folder", || assemble(project, &program))?;
    Ok(built)
}

/// Runs `cargo build` and returns the program it made.
fn compile(project: &Project, release: bool) -> Result<PathBuf, String> {
    let mut command = Command::new(cargo());
    command
        .arg("build")
        .arg("--manifest-path")
        .arg(&project.manifest_path)
        .args(["--bin", &project.bin])
        .arg("--message-format=json-render-diagnostics")
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    if release {
        command.arg("--release");
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot run cargo ({e}); install Rust from https://rustup.rs"))?;
    let mut program = None;
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(path) = executable(&message, &project.bin) {
                program = Some(path);
            }
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("cargo did not finish: {e}"))?;
    if !status.success() {
        return Err("the build failed; cargo's messages are above".into());
    }
    program.ok_or_else(|| format!("cargo built no program named {}", project.bin))
}

/// The program in one of cargo's JSON messages.
fn executable(message: &Value, bin: &str) -> Option<PathBuf> {
    if message["reason"] != "compiler-artifact" || message["target"]["name"] != bin {
        return None;
    }
    let is_bin = message["target"]["kind"]
        .as_array()
        .is_some_and(|k| k.iter().any(|k| k == "bin"));
    is_bin
        .then(|| message["executable"].as_str().map(PathBuf::from))
        .flatten()
}

/// Asks the program for its manifest, as Deckly would read it.
pub fn describe(program: &Path) -> Result<Manifest, String> {
    let output = Command::new(program)
        .arg("--describe")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "`{} --describe` failed: {}",
            program.display(),
            text.trim()
        ));
    }
    if output.stdout.len() > MAX_MANIFEST_BYTES {
        return Err("the program's manifest is too large".into());
    }
    Manifest::parse(&output.stdout).map_err(|e| format!("the program's manifest: {e}"))
}

/// The plugin id of the program built last (debug, else release), without
/// building.
pub fn describe_built(project: &Project) -> Result<String, String> {
    let name = format!("{}{}", project.bin, std::env::consts::EXE_SUFFIX);
    ["debug", "release"]
        .iter()
        .map(|profile| project.target_dir.join(profile).join(&name))
        .find(|p| p.is_file())
        .ok_or_else(|| "the plugin is not built yet; run deckly-cli build, or name its id".into())
        .and_then(|program| describe(&program))
        .map(|m| m.id)
}

/// Puts the program, the assets and the manifest into the plugin folder.
fn assemble(project: &Project, program: &Path) -> Result<Built, String> {
    let mut manifest = describe(program)?;
    let file_name = program
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("the program has no file name")?
        .to_owned();
    let dir = project.target_dir.join("deckly").join(&manifest.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    // The program, at bin/<target>/<name>; the manifest names it there.
    let targets: Vec<String> = manifest.programs.keys().cloned().collect();
    let bin_dir = dir.join("bin");
    remove(&bin_dir)?;
    for target in &targets {
        let rel = format!("bin/{target}/{file_name}");
        let to = dir.join(&rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::copy(program, &to)
            .map_err(|e| format!("cannot copy the program to {}: {e}", to.display()))?;
        manifest.programs.insert(target.clone(), rel);
    }

    // The assets, replaced as a whole so deleted files go too.
    let assets = dir.join("assets");
    remove(&assets)?;
    let source = project.dir.join("assets");
    if source.is_dir() {
        let mut budget = (MAX_ASSET_BYTES, MAX_ASSET_FILES);
        copy_dir(&source, &assets, &mut budget)?;
    }
    icons_by_convention(&mut manifest, &dir);

    // Anything else left from an earlier build.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !["bin", "assets", MANIFEST_FILE]
                .iter()
                .any(|keep| name == *keep)
            {
                remove(&entry.path())?;
            }
        }
    }

    // The manifest last, through a temporary name.
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n";
    let temp = dir.join(format!(".{MANIFEST_FILE}.tmp"));
    std::fs::write(&temp, text)
        .and_then(|()| std::fs::rename(&temp, dir.join(MANIFEST_FILE)))
        .map_err(|e| format!("cannot write {MANIFEST_FILE}: {e}"))?;
    Ok(Built { manifest, dir })
}

/// `assets/plugin.svg` (or `.png`) is the plugin's icon and
/// `assets/<action>.svg` an action's, when the code names none.
fn icons_by_convention(manifest: &mut Manifest, dir: &Path) {
    let find = |stem: &str| {
        ["svg", "png"]
            .iter()
            .map(|ext| format!("assets/{stem}.{ext}"))
            .find(|rel| dir.join(rel).is_file())
    };
    if manifest.icon.is_empty()
        && let Some(icon) = find("plugin")
    {
        manifest.icon = icon;
    }
    let prefix = format!("{}.", manifest.id);
    for action in &mut manifest.actions {
        let short = action.id.strip_prefix(&prefix).unwrap_or(&action.id);
        if action.icon.is_empty()
            && let Some(icon) = find(short)
        {
            action.icon = icon;
        }
    }
}

pub fn remove(path: &Path) -> Result<(), String> {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("cannot remove {}: {e}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Copies a folder, skipping links, within a budget of bytes and files.
pub fn copy_dir(from: &Path, to: &Path, budget: &mut (u64, usize)) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries =
        std::fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &dest, budget)?;
        } else if kind.is_file() {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if len > budget.0 || budget.1 == 0 {
                return Err(format!("{} is too large to copy", from.display()));
            }
            budget.0 -= len;
            budget.1 -= 1;
            std::fs::copy(entry.path(), &dest)
                .map_err(|e| format!("cannot copy {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use deckly_plugin_protocol::ManifestAction;
    use serde_json::json;

    #[test]
    fn finds_the_program_in_cargo_messages() {
        let m = json!({
            "reason": "compiler-artifact",
            "target": {"name": "hello", "kind": ["bin"]},
            "executable": "/t/debug/hello"
        });
        assert_eq!(
            executable(&m, "hello"),
            Some(PathBuf::from("/t/debug/hello"))
        );
        assert_eq!(executable(&m, "other"), None);
        let lib = json!({
            "reason": "compiler-artifact",
            "target": {"name": "hello", "kind": ["lib"]},
            "executable": null
        });
        assert_eq!(executable(&lib, "hello"), None);
    }

    #[test]
    fn icons_follow_the_file_names() {
        let dir = std::env::temp_dir().join(format!("deckly-cli-icons-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/plugin.svg"), "<svg/>").unwrap();
        std::fs::write(dir.join("assets/wave.png"), "png").unwrap();
        let mut m = Manifest {
            id: "com.example.hello".into(),
            actions: vec![
                ManifestAction {
                    id: "com.example.hello.wave".into(),
                    ..ManifestAction::default()
                },
                ManifestAction {
                    id: "com.example.hello.other".into(),
                    icon: "assets/own.svg".into(),
                    ..ManifestAction::default()
                },
            ],
            ..Manifest::default()
        };
        icons_by_convention(&mut m, &dir);
        assert_eq!(m.icon, "assets/plugin.svg");
        assert_eq!(m.actions[0].icon, "assets/wave.png");
        assert_eq!(m.actions[1].icon, "assets/own.svg");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
