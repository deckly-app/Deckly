//! `pack`: one `.decklyPlugin` file to share. It is a zip archive holding
//! the assembled plugin folder as `<plugin id>/`, never the repository, so
//! `.git`, `.env` and logs cannot leak. `pack --merge` joins files packed on
//! different systems into one with every system's program.

use crate::args::Common;
use crate::build;
use crate::project::Project;
use deckly_plugin_protocol::{MANIFEST_FILE, Manifest, current_target};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

/// The file name extension of a Deckly plugin file.
pub const EXTENSION: &str = "decklyPlugin";
/// Bounds for reading a file to merge.
const MAX_FILES: usize = 50_000;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;

pub fn command(common: &Common, merge: Vec<PathBuf>, out: Option<PathBuf>) -> Result<(), String> {
    if !merge.is_empty() {
        let out = crate::step("merge the plugin files", || merge_files(&merge, out))?;
        println!("The plugin file is {}", out.display());
        return Ok(());
    }
    let project = Project::find(common.package.as_deref())?;
    // What is shared is always built with optimizations.
    let built = build::build(&project, true)?;
    let manifest = crate::validate::run(&built.dir, true)?;
    let out = out.unwrap_or_else(|| {
        project.target_dir.join("deckly").join(format!(
            "{}-{}-{}.{EXTENSION}",
            manifest.id,
            manifest.version,
            current_target()
        ))
    });
    crate::step("pack the plugin file", || {
        pack(&built.dir, &manifest.id, &out)
    })?;
    println!("The plugin file is {}", out.display());
    println!("Install it in Deckly: Plugins tab, Add File… (or drop it on the window).");
    Ok(())
}

/// One file in a plugin file: its bytes and whether it is a program.
struct Entry {
    bytes: Vec<u8>,
    executable: bool,
}

/// Zips the plugin folder `dir` as `<id>/` into `out`.
pub fn pack(dir: &Path, id: &str, out: &Path) -> Result<(), String> {
    let mut files = BTreeMap::new();
    collect(dir, "", &mut files)?;
    let manifest = std::fs::read(dir.join(MANIFEST_FILE))
        .map_err(|e| format!("cannot read {MANIFEST_FILE}: {e}"))?;
    let manifest = Manifest::parse(&manifest)?;
    for rel in manifest.programs.values() {
        if let Some(e) = files.get_mut(rel.as_str()) {
            e.executable = true;
        }
    }
    write_zip(out, id, &files)
}

fn collect(dir: &Path, prefix: &str, files: &mut BTreeMap<String, Entry>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            return Err(format!(
                "{} has a name that is not valid Unicode",
                entry.path().display()
            ));
        };
        // Left by an interrupted build.
        if name.ends_with(".tmp") {
            continue;
        }
        let rel = format!("{prefix}{name}");
        if kind.is_dir() {
            collect(&entry.path(), &format!("{rel}/"), files)?;
        } else if kind.is_file() {
            if files.len() >= MAX_FILES {
                return Err("the plugin has too many files".into());
            }
            let bytes = std::fs::read(entry.path())
                .map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
            files.insert(
                rel,
                Entry {
                    bytes,
                    executable: false,
                },
            );
        }
    }
    Ok(())
}

fn write_zip(out: &Path, id: &str, files: &BTreeMap<String, Entry>) -> Result<(), String> {
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let temp = out.with_extension(format!("{EXTENSION}.tmp"));
    let result = (|| -> Result<(), String> {
        let file =
            File::create(&temp).map_err(|e| format!("cannot create {}: {e}", temp.display()))?;
        let mut zip = zip::ZipWriter::new(file);
        for (rel, entry) in files {
            let mode = if entry.executable { 0o755 } else { 0o644 };
            // Unix as the system on every host: on Windows the crate would
            // write DOS, and readers then ignore the mode, so a program
            // packed on Windows would not run on Linux or macOS.
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .system(zip::System::Unix)
                .unix_permissions(mode);
            zip.start_file(format!("{id}/{rel}"), options)
                .and_then(|()| zip.write_all(&entry.bytes).map_err(Into::into))
                .map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
        }
        zip.finish()
            .map_err(|e| format!("cannot write {}: {e}", temp.display()))?;
        std::fs::rename(&temp, out).map_err(|e| format!("cannot write {}: {e}", out.display()))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Reads a plugin file: its plugin id, manifest and files.
fn read_file(path: &Path) -> Result<(String, Manifest, BTreeMap<String, Entry>), String> {
    let name = path.display();
    let file = File::open(path).map_err(|e| format!("cannot open {name}: {e}"))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("{name} is not a zip file: {e}"))?;
    if zip.len() > MAX_FILES {
        return Err(format!("{name} has too many files"));
    }
    let mut id: Option<String> = None;
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("cannot read {name}: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let path = entry
            .enclosed_name()
            .ok_or_else(|| format!("{name} has an unsafe entry {:?}", entry.name()))?;
        let rel = path
            .to_str()
            .ok_or_else(|| format!("{name} has an odd entry name"))?;
        let rel = rel.replace('\\', "/");
        let (top, rest) = rel
            .split_once('/')
            .ok_or_else(|| format!("{name} has a file outside the plugin folder: {rel}"))?;
        match &id {
            None => id = Some(top.to_owned()),
            Some(id) if id != top => {
                return Err(format!("{name} holds more than one plugin folder"));
            }
            Some(_) => {}
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(MAX_BYTES - total + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("cannot read {name}: {e}"))?;
        total += bytes.len() as u64;
        if total > MAX_BYTES {
            return Err(format!("{name} is too large"));
        }
        let executable = entry.unix_mode().is_some_and(|m| m & 0o111 != 0);
        files.insert(rest.to_owned(), Entry { bytes, executable });
    }
    let id = id.ok_or_else(|| format!("{name} is empty"))?;
    let manifest = files
        .get(MANIFEST_FILE)
        .ok_or_else(|| format!("{name} has no {id}/{MANIFEST_FILE}"))
        .and_then(|e| Manifest::parse(&e.bytes).map_err(|e| format!("{name}: {e}")))?;
    if manifest.id != id {
        return Err(format!(
            "{name}: the folder {id} holds plugin {}",
            manifest.id
        ));
    }
    Ok((id, manifest, files))
}

/// Joins plugin files of one plugin and version, packed on different
/// systems, into one.
pub fn merge_files(inputs: &[PathBuf], out: Option<PathBuf>) -> Result<PathBuf, String> {
    let mut merged: Option<(String, Manifest, BTreeMap<String, Entry>)> = None;
    for input in inputs {
        let (id, manifest, files) = read_file(input)?;
        let Some((_, all, all_files)) = &mut merged else {
            merged = Some((id, manifest, files));
            continue;
        };
        let same = |m: &Manifest| Manifest {
            programs: BTreeMap::new(),
            ..m.clone()
        };
        if same(all) != same(&manifest) {
            return Err(format!(
                "{} is not the same plugin and version as {} (or was built from other code)",
                input.display(),
                inputs[0].display()
            ));
        }
        for (target, program) in manifest.programs {
            if let Some(old) = all.programs.get(&target)
                && *old != program
            {
                return Err(format!("two files name different programs for {target}"));
            }
            all.programs.insert(target, program);
        }
        for (rel, entry) in files {
            if rel == MANIFEST_FILE {
                continue;
            }
            match all_files.get(&rel) {
                Some(old) if old.bytes != entry.bytes => {
                    return Err(format!(
                        "{rel} differs between the files; programs of different systems \
                         must have their own paths (deckly-cli build puts them in bin/<target>/)"
                    ));
                }
                Some(_) => {}
                None => {
                    all_files.insert(rel, entry);
                }
            }
        }
    }
    let (id, manifest, mut files) = merged.ok_or("nothing to merge")?;
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n";
    files.insert(
        MANIFEST_FILE.to_owned(),
        Entry {
            bytes: text.into_bytes(),
            executable: false,
        },
    );
    let out = out.unwrap_or_else(|| {
        let dir = inputs[0].parent().unwrap_or(Path::new("."));
        dir.join(format!("{id}-{}.{EXTENSION}", manifest.version))
    });
    write_zip(&out, &id, &files)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deckly-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A plugin folder as `build` makes it on one system.
    fn plugin(root: &Path, target: &str) -> PathBuf {
        let dir = root.join(target).join("com.example.a");
        let program = format!("bin/{target}/a");
        std::fs::create_dir_all(dir.join(format!("bin/{target}"))).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join(&program), target).unwrap();
        std::fs::write(dir.join("assets/plugin.svg"), "<svg/>").unwrap();
        let manifest = serde_json::json!({
            "id": "com.example.a", "name": "A", "version": "1.0.0", "protocol": 1,
            "icon": "assets/plugin.svg",
            "programs": {target: program}
        });
        std::fs::write(dir.join(MANIFEST_FILE), manifest.to_string()).unwrap();
        dir
    }

    #[test]
    fn packs_and_merges_programs_of_several_systems() {
        let root = folder("pack");
        let linux = plugin(&root, "x86_64-unknown-linux-gnu");
        let mac = plugin(&root, "aarch64-apple-darwin");
        let a = root.join("a.decklyPlugin");
        let b = root.join("b.decklyPlugin");
        pack(&linux, "com.example.a", &a).unwrap();
        pack(&mac, "com.example.a", &b).unwrap();
        let (id, m, files) = read_file(&a).unwrap();
        assert_eq!(id, "com.example.a");
        assert_eq!(m.programs.len(), 1);
        assert!(files["bin/x86_64-unknown-linux-gnu/a"].executable);
        assert!(!files["assets/plugin.svg"].executable);

        let out = merge_files(&[a.clone(), b], None).unwrap();
        assert_eq!(out, root.join("com.example.a-1.0.0.decklyPlugin"));
        let (_, m, files) = read_file(&out).unwrap();
        assert_eq!(m.programs.len(), 2);
        assert!(files.contains_key("bin/aarch64-apple-darwin/a"));
        assert!(files.contains_key("bin/x86_64-unknown-linux-gnu/a"));

        // Another version does not merge.
        let other = plugin(&root.join("v2"), "aarch64-apple-darwin");
        let text = std::fs::read_to_string(other.join(MANIFEST_FILE))
            .unwrap()
            .replace("1.0.0", "2.0.0");
        std::fs::write(other.join(MANIFEST_FILE), text).unwrap();
        let c = root.join("c.decklyPlugin");
        pack(&other, "com.example.a", &c).unwrap();
        assert!(merge_files(&[a, c], None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
