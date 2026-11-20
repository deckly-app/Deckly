//! Keeps the `AGENTS.md` that `create` writes in step with the SDK: the
//! book pages it names exist, and the SDK calls it tells agents to use are
//! still there. Needs the Deckly repository around this crate; a crate
//! unpacked on its own skips the check.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

const AGENTS: &str = include_str!("../templates/common/AGENTS.md");

fn repository() -> Option<PathBuf> {
    let dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_MANIFEST_DIR").into());
    let root = dir.ancestors().nth(2)?.to_owned();
    root.join("docs/sdk/SUMMARY.md").is_file().then_some(root)
}

#[test]
fn the_named_book_pages_exist() {
    let Some(root) = repository() else { return };
    let summary = std::fs::read_to_string(root.join("docs/sdk/SUMMARY.md")).unwrap();
    let line = AGENTS
        .split("- Most used pages:")
        .nth(1)
        .and_then(|rest| rest.split("\n\n").next())
        .unwrap();
    let titles: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
    assert!(titles.len() >= 8, "{titles:?}");
    for title in titles {
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(
            summary.contains(&format!("[{title}](")),
            "AGENTS.md names a book page that SUMMARY.md lacks: {title}"
        );
    }
}

#[test]
fn the_named_sdk_calls_exist() {
    let Some(root) = repository() else { return };
    let sdk = root.join("crates/deckly-sdk/src");
    let mut source = String::new();
    for entry in std::fs::read_dir(&sdk).unwrap().flatten() {
        if entry.path().extension().is_some_and(|e| e == "rs") {
            source.push_str(&std::fs::read_to_string(entry.path()).unwrap());
        }
    }
    for (named, defined) in [
        ("key.show_alert()", "pub fn show_alert("),
        ("key.settings()", "pub fn settings("),
        ("key.update_settings(", "pub fn update_settings("),
        ("deckly_sdk::testing::Harness", "pub struct Harness"),
    ] {
        assert!(AGENTS.contains(named), "AGENTS.md no longer names {named}");
        assert!(
            source.contains(defined),
            "the SDK has no {defined}, which AGENTS.md names"
        );
    }
}
