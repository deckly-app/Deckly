//! `validate`: checks the assembled plugin folder as Deckly will, then
//! starts the plugin once as Deckly does (the smoke start). This catches
//! the common mistakes before Deckly shows a plugin that keeps restarting:
//! a `println!`, slow work before `run()`, a plugin that ignores its closed
//! input.

use crate::args::Common;
use crate::build;
use crate::project::Project;
use deckly_plugin_protocol::limits::MAX_MANIFEST_BYTES;
use deckly_plugin_protocol::{
    Hello, HostMessage, MANIFEST_FILE, Manifest, PROTOCOL, PluginMessage, Reader, Writer,
    current_target,
};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// Deckly restarts a plugin that has not answered `hello` in 10 seconds;
/// the check asks for less, to leave room on a slower computer.
const READY_WITHIN: Duration = Duration::from_secs(5);
/// Deckly kills a plugin that has not exited 3 seconds after its input
/// closed.
const EXIT_WITHIN: Duration = Duration::from_secs(3);
/// The most of the plugin's standard error kept for a message.
const MAX_STDERR: usize = 4096;

pub fn command(common: &Common, start: bool) -> Result<(), String> {
    let project = Project::find(common.package.as_deref())?;
    let built = build::build(&project, common.release)?;
    run(&built.dir, start)?;
    println!("{} is ready for Deckly.", built.manifest.id);
    Ok(())
}

/// Checks the folder, and with `start` starts the plugin once.
pub fn run(dir: &Path, start: bool) -> Result<Manifest, String> {
    let manifest = crate::step("check the plugin folder", || {
        check_folder(dir).map_err(|problems| {
            format!(
                "the plugin folder has problems:\n  {}",
                problems.join("\n  ")
            )
        })
    })?;
    if start {
        crate::step("start the plugin as Deckly does", || smoke(dir, &manifest))?;
    }
    Ok(manifest)
}

/// Reads the manifest with the host's own checks, then checks what the
/// manifest points at.
pub fn check_folder(dir: &Path) -> Result<Manifest, Vec<String>> {
    let path = dir.join(MANIFEST_FILE);
    let too_large = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_MANIFEST_BYTES as u64);
    if too_large {
        return Err(vec![format!("{MANIFEST_FILE} is too large")]);
    }
    let bytes =
        std::fs::read(&path).map_err(|e| vec![format!("cannot read {MANIFEST_FILE}: {e}")])?;
    let m = Manifest::parse(&bytes).map_err(|e| vec![e])?;
    let mut problems = Vec::new();
    if dir.file_name().and_then(|n| n.to_str()) != Some(m.id.as_str()) {
        problems.push(format!("the folder should be named {}", m.id));
    }
    let prefix = format!("{}.", m.id);
    for a in &m.actions {
        if !a.id.starts_with(&prefix) {
            problems.push(format!(
                "action {} does not start with the plugin id {}",
                a.id, m.id
            ));
        }
    }
    let mut files: Vec<(&str, &str)> = Vec::new();
    if !m.icon.is_empty() {
        files.push(("the plugin icon", &m.icon));
    }
    for a in &m.actions {
        if !a.icon.is_empty() {
            files.push(("an action icon", &a.icon));
        }
        for s in &a.states {
            if !s.image.is_empty() {
                files.push(("a state image", &s.image));
            }
        }
    }
    for (what, rel) in files {
        if !dir.join(rel).is_file() {
            problems.push(format!("{what} {rel} is missing"));
        }
    }
    let here = current_target();
    if m.program_for(&here).is_none() {
        problems.push(format!("there is no program for this system ({here})"));
    }
    for (target, rel) in &m.programs {
        let program = dir.join(rel);
        if !program.is_file() {
            problems.push(format!("the program for {target}, {rel}, is missing"));
        } else if !is_executable(&program) {
            problems.push(format!("the program {rel} is not executable"));
        }
    }
    if problems.is_empty() {
        Ok(m)
    } else {
        Err(problems)
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

/// Starts the plugin as Deckly does: `hello` in, `ready` out, then its input
/// closes and it must exit.
pub fn smoke(dir: &Path, m: &Manifest) -> Result<(), String> {
    let rel = m
        .program_for(&current_target())
        .ok_or("there is no program for this system")?;
    let program: PathBuf = dir.join(rel);
    let mut child = Command::new(&program)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", program.display()))?;
    let (Some(stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        stop(child);
        return Err("no pipes to the plugin".into());
    };
    let errors = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stderr.take(MAX_STDERR as u64).read_to_end(&mut text);
        String::from_utf8_lossy(&text).into_owned()
    });
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut reader = Reader::new(stdout);
        loop {
            let read = reader.read::<PluginMessage>();
            let end = !matches!(read, Ok(Some(_)));
            if tx.send(read.map(|r| r.map(|r| r.message))).is_err() || end {
                break;
            }
        }
    });
    let mut input = Writer::new(stdin);
    let _ = input.write_message(&HostMessage::Hello(hello()));
    let began = Instant::now();
    let answer = rx.recv_timeout(READY_WITHIN);
    let log = |child: Child| {
        stop(child);
        let text = errors.join().unwrap_or_default();
        if text.trim().is_empty() {
            String::new()
        } else {
            format!("\nIts standard error:\n{}", text.trim_end())
        }
    };
    match answer {
        Ok(Ok(Some(PluginMessage::Ready { .. }))) => {}
        Ok(Ok(Some(other))) => {
            let kind = serde_json::to_value(&other)
                .ok()
                .and_then(|v| v["type"].as_str().map(str::to_owned))
                .unwrap_or_default();
            return Err(format!(
                "the plugin answered hello with {kind}, not ready{}",
                log(child)
            ));
        }
        Ok(Ok(None)) => {
            return Err(format!(
                "the plugin exited without answering hello{}",
                log(child)
            ));
        }
        Ok(Err(e)) => {
            return Err(format!(
                "the plugin wrote something that is not the protocol to standard output \
                 ({e}). Standard output belongs to Deckly: log with eprintln!, never \
                 println!{}",
                log(child)
            ));
        }
        Err(RecvTimeoutError::Timeout) => {
            return Err(format!(
                "the plugin did not answer hello within {} s. Do slow work after run() \
                 starts, on a thread of its own{}",
                READY_WITHIN.as_secs(),
                log(child)
            ));
        }
        Err(RecvTimeoutError::Disconnected) => {
            return Err(format!("the plugin stopped{}", log(child)));
        }
    }
    let ready_after = began.elapsed();
    // Deckly closes the input when it quits; the plugin must exit then.
    drop(input);
    let deadline = Instant::now() + EXIT_WITHIN;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                return Err(format!(
                    "the plugin did not exit within {} s after its input closed, as it \
                     must when Deckly quits{}",
                    EXIT_WITHIN.as_secs(),
                    log(child)
                ));
            }
        }
    }
    println!(
        "          answered hello in {} ms and exited when its input closed",
        ready_after.as_millis()
    );
    Ok(())
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// The `hello` a smoke start sends: one test device, no services.
fn hello() -> Hello {
    let platform = match std::env::consts::OS {
        "macos" => "mac",
        "windows" => "windows",
        _ => "linux",
    };
    Hello {
        protocol: PROTOCOL,
        app_version: format!("deckly-cli {}", crate::VERSION),
        platform: platform.into(),
        session: String::new(),
        language: "en".into(),
        devices: Vec::new(),
        services: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_without_a_manifest_is_a_problem() {
        let dir = std::env::temp_dir().join(format!("deckly-cli-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let problems = check_folder(&dir).unwrap_err();
        assert!(problems[0].contains(MANIFEST_FILE), "{problems:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_files_and_foreign_actions_are_listed() {
        let dir = std::env::temp_dir()
            .join(format!("deckly-cli-check-{}", std::process::id()))
            .join("com.example.a");
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = serde_json::json!({
            "id": "com.example.a", "name": "A", "version": "1", "protocol": 1,
            "icon": "assets/plugin.svg",
            "programs": {"x86_64-unknown-linux-gnu": "bin/a"},
            "actions": [{"id": "com.other.b", "name": "B"}]
        });
        std::fs::write(dir.join(MANIFEST_FILE), manifest.to_string()).unwrap();
        let problems = check_folder(&dir).unwrap_err().join("\n");
        assert!(problems.contains("plugin icon"), "{problems}");
        assert!(problems.contains("com.other.b"), "{problems}");
        assert!(problems.contains("bin/a"), "{problems}");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
