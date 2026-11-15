//! `log`: the plugin's log, where Deckly keeps it
//! (`<config>/logs/plugins/<id>.log`): the plugin's standard error and
//! Deckly's own notes, marked `[deckly]`. Deckly starts the file again at
//! each start of the plugin.

use crate::args::Common;
use crate::config::{config_dir, log_file};
use crate::project::Project;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The most read from the log at once.
const CHUNK: u64 = 256 * 1024;

pub fn command(common: &Common, follow: bool, id: Option<String>) -> Result<(), String> {
    let config = config_dir(common.config_dir.as_deref())?;
    let id = match id {
        Some(id) => id,
        None => {
            let project = Project::find(common.package.as_deref())?;
            crate::build::describe_built(&project)?
        }
    };
    let path = log_file(&config, &id);
    let mut tail = Tail::new(path.clone());
    if !path.is_file() {
        println!("{id} has no log yet ({}).", path.display());
        if !follow {
            return Ok(());
        }
    }
    tail.print();
    if follow {
        loop {
            std::thread::sleep(Duration::from_millis(250));
            tail.print();
        }
    }
    Ok(())
}

/// Reads what was added to a log since the last read.
pub struct Tail {
    path: PathBuf,
    at: u64,
}

impl Tail {
    pub fn new(path: PathBuf) -> Self {
        Self { path, at: 0 }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The new text, if any. A file that got shorter was started again.
    pub fn read(&mut self) -> Option<String> {
        let mut file = File::open(&self.path).ok()?;
        let len = file.metadata().ok()?.len();
        let mut restarted = false;
        if len < self.at {
            self.at = 0;
            restarted = true;
        }
        if len == self.at {
            return None;
        }
        file.seek(SeekFrom::Start(self.at)).ok()?;
        let mut bytes = Vec::new();
        file.take(CHUNK).read_to_end(&mut bytes).ok()?;
        self.at += bytes.len() as u64;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Some(if restarted {
            format!("--- the log started again ---\n{text}")
        } else {
            text
        })
    }

    pub fn print(&mut self) {
        while let Some(text) = self.read() {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(text.as_bytes());
            let _ = out.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_new_lines_and_notices_a_new_file() {
        let path = std::env::temp_dir().join(format!("deckly-cli-tail-{}.log", std::process::id()));
        std::fs::write(&path, "one\n").unwrap();
        let mut tail = Tail::new(path.clone());
        assert_eq!(tail.read().as_deref(), Some("one\n"));
        assert_eq!(tail.read(), None);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"two\n").unwrap();
        assert_eq!(tail.read().as_deref(), Some("two\n"));
        // Deckly starts the file again when the plugin restarts.
        std::fs::write(&path, "new\n").unwrap();
        assert!(tail.read().unwrap().ends_with("new\n"));
        let _ = std::fs::remove_file(&path);
    }
}
