//! `link`, `unlink` and `list`: the development link file,
//! `<config>/dev-plugins.json`. Deckly runs each linked folder as a
//! development plugin. No symbolic links: they need a privilege on Windows.

use crate::args::Common;
use crate::build;
use crate::config::config_dir;
use crate::control::{self, Answer};
use crate::project::Project;
use deckly_plugin_protocol::dev::{Control, LINKS_FILE, Links, MAX_LINKS};
use std::path::{Path, PathBuf};

pub fn read(config: &Path) -> Result<Links, String> {
    let path = config.join(LINKS_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => Links::parse(&bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Links::default()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Writes the link file through a temporary name, so Deckly never reads
/// half of it.
fn write(config: &Path, links: &Links) -> Result<(), String> {
    std::fs::create_dir_all(config)
        .map_err(|e| format!("cannot create {}: {e}", config.display()))?;
    let path = config.join(LINKS_FILE);
    let temp = config.join(format!(".{LINKS_FILE}.{}.tmp", std::process::id()));
    std::fs::write(&temp, links.to_json())
        .and_then(|()| std::fs::rename(&temp, &path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("cannot write {}: {e}", path.display())
        })
}

/// Links plugin `id` to `folder` (made absolute).
pub fn add(config: &Path, id: &str, folder: &Path) -> Result<(), String> {
    let folder = std::path::absolute(folder)
        .map_err(|e| format!("cannot resolve {}: {e}", folder.display()))?;
    let mut links = read(config)?;
    if !links.plugins.contains_key(id) && links.plugins.len() >= MAX_LINKS {
        return Err(format!(
            "{MAX_LINKS} plugins are linked already; unlink some"
        ));
    }
    let text = folder
        .to_str()
        .ok_or("the plugin folder's path is not valid Unicode")?
        .to_owned();
    if links.plugins.get(id) == Some(&text) {
        return Ok(());
    }
    links.plugins.insert(id.to_owned(), text);
    write(config, &links)
}

/// Removes plugin `id`'s link; false when it had none.
pub fn remove(config: &Path, id: &str) -> Result<bool, String> {
    let mut links = read(config)?;
    if links.plugins.remove(id).is_none() {
        return Ok(false);
    }
    write(config, &links).map(|()| true)
}

/// Asks a running Deckly to restart the plugin and says what happened.
pub fn reload(config: &Path, id: &str) -> Result<(), String> {
    match control::send(config, &Control::Reload(id.to_owned()))? {
        Answer::Done(what) => {
            println!("  done    Deckly {what} {id}");
            Ok(())
        }
        Answer::NotRunning => {
            println!("  note    Deckly is not running; it starts {id} the next time it starts");
            Ok(())
        }
    }
}

/// Builds the plugin and links its folder.
pub fn build_and_link(common: &Common, project: &Project) -> Result<(String, PathBuf), String> {
    let config = config_dir(common.config_dir.as_deref())?;
    let built = build::build(project, common.release)?;
    let id = built.manifest.id.clone();
    crate::step(&format!("link {id} into Deckly"), || {
        add(&config, &id, &built.dir)
    })?;
    Ok((id, config))
}

pub fn link_command(common: &Common) -> Result<(), String> {
    let project = Project::find(common.package.as_deref())?;
    let (id, config) = build_and_link(common, &project)?;
    reload(&config, &id)?;
    println!("Deckly runs {id} from this project now. Remove the link with `deckly-cli unlink`.");
    Ok(())
}

pub fn unlink_command(common: &Common, id: Option<String>) -> Result<(), String> {
    let config = config_dir(common.config_dir.as_deref())?;
    let id = match id {
        Some(id) => id,
        None => {
            // The link that points into this project's build folder.
            let project = Project::find(common.package.as_deref())?;
            let ours = project.target_dir.join("deckly");
            read(&config)?
                .plugins
                .into_iter()
                .find(|(_, dir)| Path::new(dir).starts_with(&ours))
                .map(|(id, _)| id)
                .ok_or("this project's plugin is not linked")?
        }
    };
    if !remove(&config, &id)? {
        return Err(format!("{id} is not linked"));
    }
    println!("Unlinked {id}.");
    if let Answer::Done(_) = control::send(&config, &Control::Rescan)? {
        println!("Deckly stopped running it from the project folder.");
    }
    Ok(())
}

pub fn list_command(common: &Common) -> Result<(), String> {
    let config = config_dir(common.config_dir.as_deref())?;
    let links = read(&config)?;
    if links.plugins.is_empty() {
        println!(
            "No plugins are linked ({}).",
            config.join(LINKS_FILE).display()
        );
    }
    for (id, dir) in &links.plugins {
        let note = if Path::new(dir).is_dir() {
            ""
        } else {
            "  (missing)"
        };
        println!("{id}  {dir}{note}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_added_replaced_and_removed() {
        let dir = std::env::temp_dir().join(format!("deckly-cli-links-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // The settings folder need not exist yet.
        add(&dir, "com.example.a", Path::new("/src/a")).unwrap();
        add(&dir, "com.example.a", Path::new("/src/a2")).unwrap();
        add(&dir, "com.example.b", Path::new("/src/b")).unwrap();
        let links = read(&dir).unwrap();
        assert_eq!(links.plugins.len(), 2);
        assert!(links.plugins["com.example.a"].ends_with("a2"));
        assert!(remove(&dir, "com.example.a").unwrap());
        assert!(!remove(&dir, "com.example.a").unwrap());
        assert_eq!(read(&dir).unwrap().plugins.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
