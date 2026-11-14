//! `dev`: the everyday loop. Build, link and restart the plugin in a running
//! Deckly; then on every save of `src/`, `assets/` or `Cargo.toml` build
//! again and restart it; and print the plugin's log as it grows. A failed
//! build leaves the running plugin alone. Ctrl+C stops the tool and leaves
//! the link, so the plugin stays usable in Deckly.

use crate::args::Common;
use crate::config::log_file;
use crate::link;
use crate::log::Tail;
use crate::project::Project;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// Saves come in bursts (an editor writes, renames, touches); wait for the
/// burst to end before building.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// How often the log is read for new lines.
const LOG_EVERY: Duration = Duration::from_millis(250);

pub fn run(common: &Common, show_log: bool) -> Result<(), String> {
    let project = Project::find(common.package.as_deref())?;
    let (mut id, config) = link::build_and_link(common, &project)?;
    link::reload(&config, &id)?;

    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })
    .map_err(|e| format!("cannot watch the project's files: {e}"))?;
    for (dir, mode) in [
        (project.dir.join("src"), RecursiveMode::Recursive),
        (project.dir.join("assets"), RecursiveMode::Recursive),
        (project.dir.clone(), RecursiveMode::NonRecursive),
    ] {
        if dir.is_dir() {
            watcher
                .watch(&dir, mode)
                .map_err(|e| format!("cannot watch {}: {e}", dir.display()))?;
        }
    }
    println!(
        "\nWatching src/, assets/ and Cargo.toml in {}. Each save builds and restarts \
         the plugin.\nCtrl+C stops; the plugin stays linked (deckly-cli unlink removes it).",
        project.dir.display()
    );
    let mut tail = Tail::new(log_file(&config, &id));
    if show_log {
        println!("The plugin's log, {}:", tail.path().display());
    }
    let mut pending: Option<Instant> = None;
    loop {
        let wait = match pending {
            Some(at) => at.saturating_duration_since(Instant::now()),
            None if show_log => LOG_EVERY,
            None => Duration::from_secs(3600),
        };
        match rx.recv_timeout(wait) {
            Ok(Ok(event)) if relevant(&event, &project.dir, &project.target_dir) => {
                pending = Some(Instant::now() + DEBOUNCE);
            }
            Ok(Ok(_)) | Err(RecvTimeoutError::Timeout) => {}
            Ok(Err(e)) => eprintln!("deckly-cli: watching files: {e}"),
            Err(RecvTimeoutError::Disconnected) => {
                return Err("the file watcher stopped".into());
            }
        }
        if pending.is_some_and(|at| Instant::now() >= at) {
            pending = None;
            if show_log {
                tail.print();
            }
            println!("\nA file changed; building again.");
            match rebuild(common, &project, &config, &id) {
                Ok(new_id) => {
                    id = new_id;
                    // Deckly started the log again with the plugin.
                    tail = Tail::new(log_file(&config, &id));
                }
                Err(e) => {
                    eprintln!(
                        "deckly-cli: {e}\nThe plugin in Deckly is unchanged; fix it and save again."
                    );
                }
            }
        }
        if show_log {
            tail.print();
        }
    }
}

fn rebuild(common: &Common, project: &Project, config: &Path, old: &str) -> Result<String, String> {
    let (id, _) = link::build_and_link(common, project)?;
    if id != old {
        // The code changed the plugin id: the old link would run the old one.
        link::remove(config, old)?;
        println!("  note    the plugin id changed from {old} to {id}; the old link is removed");
    }
    link::reload(config, &id)?;
    Ok(id)
}

/// A change that needs a new build: a file in `src/` or `assets/`, or
/// `Cargo.toml` or `build.rs`; not editors' temporary files or the build's
/// own output.
fn relevant(event: &Event, project: &Path, target: &Path) -> bool {
    if matches!(event.kind, EventKind::Access(_) | EventKind::Other) {
        return false;
    }
    event.paths.iter().any(|path| {
        if path.starts_with(target) {
            return false;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let temporary = name.starts_with('.')
            || name.ends_with('~')
            || name.ends_with(".swp")
            || name.ends_with(".swx")
            || name.ends_with(".tmp")
            || name == "4913";
        if temporary {
            return false;
        }
        path.starts_with(project.join("src"))
            || path.starts_with(project.join("assets"))
            || (path.parent() == Some(project) && matches!(name, "Cargo.toml" | "build.rs"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, ModifyKind};
    use std::path::PathBuf;

    fn event(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(PathBuf::from(path))
    }

    #[test]
    fn only_sources_and_assets_count() {
        let (p, t) = (Path::new("/p"), Path::new("/p/target"));
        let modify = EventKind::Modify(ModifyKind::Any);
        assert!(relevant(&event(modify, "/p/src/main.rs"), p, t));
        assert!(relevant(&event(modify, "/p/assets/icon.svg"), p, t));
        assert!(relevant(&event(modify, "/p/Cargo.toml"), p, t));
        assert!(!relevant(&event(modify, "/p/Cargo.lock"), p, t));
        assert!(!relevant(&event(modify, "/p/README.md"), p, t));
        assert!(!relevant(&event(modify, "/p/src/.main.rs.swp"), p, t));
        assert!(!relevant(&event(modify, "/p/target/debug/x"), p, t));
        let access = EventKind::Access(AccessKind::Any);
        assert!(!relevant(&event(access, "/p/src/main.rs"), p, t));
    }
}
