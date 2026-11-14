//! Plugin development: what the plugin tool (`deckly-cli`) and a running
//! Deckly share, besides the plugin protocol itself.
//!
//! - **The link file**, `dev-plugins.json` in Deckly's settings folder,
//!   lists plugin folders that Deckly runs as development plugins: from a
//!   fresh copy, restarted on request, with "development" shown in the
//!   Plugins tab.
//! - **The control line**: one line of text the tool sends over the local
//!   port a running Deckly listens on (`instance.port`), with the token
//!   Deckly keeps in `instance.token`, to restart a plugin after a build.
//!   A connection that sends nothing still only shows Deckly's window.
//!
//! Pure: the callers read and write the files and the socket.

use crate::limits::MAX_ID_BYTES;
use crate::message::valid_id;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The link file's name inside Deckly's settings folder.
pub const LINKS_FILE: &str = "dev-plugins.json";
/// The file holding the port a running Deckly listens on.
pub const PORT_FILE: &str = "instance.port";
/// The file holding the token a control line must carry. Readable only by
/// the user who runs Deckly.
pub const TOKEN_FILE: &str = "instance.token";
/// The first word of a control line, with the format version.
pub const CONTROL_PREFIX: &str = "deckly-control 1";
/// Largest control line, and largest answer, in bytes.
pub const MAX_CONTROL_BYTES: usize = 1024;
/// Largest link file.
pub const MAX_LINKS_BYTES: usize = 256 * 1024;
/// Most plugins linked at once.
pub const MAX_LINKS: usize = 256;
/// Longest folder path in the link file.
pub const MAX_LINK_PATH_BYTES: usize = 4096;

/// The link file: plugin id to the absolute folder that holds the plugin
/// (`deckly-plugin.json`, the program and its files).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Links {
    #[serde(default)]
    pub plugins: BTreeMap<String, String>,
}

impl Links {
    /// Reads a link file. Entries with a bad id or path are dropped, so one
    /// damaged entry does not hide the others.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_LINKS_BYTES {
            return Err(format!("{LINKS_FILE} is too large"));
        }
        let mut links: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("{LINKS_FILE} is not valid: {e}"))?;
        links
            .plugins
            .retain(|id, dir| valid_id(id) && !dir.is_empty() && dir.len() <= MAX_LINK_PATH_BYTES);
        while links.plugins.len() > MAX_LINKS {
            links.plugins.pop_last();
        }
        Ok(links)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into()) + "\n"
    }
}

/// What a control line asks a running Deckly to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// Read the plugins folder and the link file again, then restart the
    /// plugin (from a fresh copy, for a development plugin). Answers once
    /// the plugin was started, not once it is running.
    Reload(String),
    /// Read the plugins folder and the link file again.
    Rescan,
}

impl Control {
    /// The line to send, ending in a newline.
    pub fn line(&self, token: &str) -> String {
        match self {
            Control::Reload(id) => format!("{CONTROL_PREFIX} {token} reload {id}\n"),
            Control::Rescan => format!("{CONTROL_PREFIX} {token} rescan\n"),
        }
    }

    /// Reads a line: the token it carries and what it asks.
    pub fn parse(line: &str) -> Result<(String, Self), String> {
        if line.len() > MAX_CONTROL_BYTES {
            return Err("the line is too long".into());
        }
        let rest = line
            .trim_end_matches(['\r', '\n'])
            .strip_prefix(CONTROL_PREFIX)
            .and_then(|r| r.strip_prefix(' '))
            .ok_or("not a control line of this version")?;
        let mut words = rest.split(' ');
        let token = words.next().filter(|t| !t.is_empty()).ok_or("no token")?;
        let control = match (words.next(), words.next(), words.next()) {
            (Some("reload"), Some(id), None) if valid_id(id) && id.len() <= MAX_ID_BYTES => {
                Control::Reload(id.to_owned())
            }
            (Some("rescan"), None, None) => Control::Rescan,
            _ => return Err("unknown request".into()),
        };
        Ok((token.to_owned(), control))
    }
}

/// The answer line: `ok <text>` or `error <text>`, one line.
pub fn answer(result: &Result<String, String>) -> String {
    let (word, text) = match result {
        Ok(t) => ("ok", t),
        Err(t) => ("error", t),
    };
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let line = format!("{word} {}", text.trim());
    let cut = crate::truncate(&line, MAX_CONTROL_BYTES / 4);
    format!("{cut}\n")
}

/// Reads an answer line.
pub fn parse_answer(line: &str) -> Result<String, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(text) = line.strip_prefix("ok") {
        Ok(text.trim().to_owned())
    } else if let Some(text) = line.strip_prefix("error") {
        Err(text.trim().to_owned())
    } else {
        Err(format!(
            "Deckly gave an answer this tool does not know: {}",
            crate::truncate(line, 80)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_lines_round_trip() {
        for c in [Control::Reload("com.example.hello".into()), Control::Rescan] {
            let line = c.line("abc123");
            assert!(line.ends_with('\n'));
            assert_eq!(Control::parse(&line), Ok(("abc123".to_owned(), c)));
        }
    }

    #[test]
    fn odd_control_lines_are_refused() {
        for line in [
            "",
            "hello",
            "deckly-control 2 t rescan",
            "deckly-control 1  rescan",
            "deckly-control 1 t reload",
            "deckly-control 1 t reload ../etc",
            "deckly-control 1 t reload a b",
            "deckly-control 1 t delete everything",
        ] {
            assert!(Control::parse(line).is_err(), "{line:?}");
        }
        let long = format!("deckly-control 1 t reload {}", "a".repeat(2000));
        assert!(Control::parse(&long).is_err());
    }

    #[test]
    fn answers_round_trip_on_one_line() {
        let ok = answer(&Ok("restarted".into()));
        assert_eq!(parse_answer(&ok), Ok("restarted".into()));
        let err = answer(&Err("no such\nplugin".into()));
        assert_eq!(err.matches('\n').count(), 1);
        assert_eq!(parse_answer(&err), Err("no such plugin".into()));
        assert!(parse_answer("what").is_err());
    }

    #[test]
    fn links_drop_bad_entries() {
        let text =
            br#"{"plugins": {"com.example.a": "/src/a", "../x": "/y", "com.example.b": ""}}"#;
        let links = Links::parse(text).unwrap();
        assert_eq!(links.plugins.len(), 1);
        assert_eq!(Links::parse(links.to_json().as_bytes()), Ok(links));
        assert!(Links::parse(b"nope").is_err());
        assert_eq!(Links::parse(b"{}"), Ok(Links::default()));
    }
}
