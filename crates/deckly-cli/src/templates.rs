//! The files of a new project. The templates are text files in
//! `templates/`, written for Deckly and shipped under its licence. `%%key%%`
//! marks a value: values ending in `_text` and `_comment` go in as they are
//! (Markdown, comments), the others as quoted, escaped strings for TOML and
//! Rust.

use crate::args::{Example, License, Sdk};
use std::path::{Path, PathBuf};

/// The answers a project is made from.
#[derive(Clone, Debug)]
pub struct Answers {
    pub name: String,
    pub crate_name: String,
    pub id: String,
    pub author: String,
    pub description: String,
    pub example: Example,
    pub license: License,
    pub sdk: Sdk,
    pub ci: bool,
    /// The project is made inside another Cargo workspace and must not
    /// become part of it.
    pub own_workspace: bool,
}

/// One file of the project: its path inside the project and its contents.
pub struct File {
    pub path: &'static str,
    pub text: String,
}

/// The project's files.
pub fn files(a: &Answers, year: i32) -> Vec<File> {
    let values = values(a, year);
    let render = |t: &str| render(t, &values);
    let mut files = vec![
        File {
            path: "Cargo.toml",
            text: render(include_str!("../templates/common/Cargo.toml.tmpl")),
        },
        File {
            path: ".gitignore",
            text: include_str!("../templates/common/gitignore").to_owned(),
        },
        File {
            path: "README.md",
            text: render(include_str!("../templates/common/README.md")),
        },
        File {
            path: "assets/plugin.svg",
            text: include_str!("../templates/common/plugin.svg").to_owned(),
        },
    ];
    match a.example {
        Example::Counter => files.extend([
            File {
                path: "src/main.rs",
                text: render(include_str!("../templates/counter/main.rs")),
            },
            File {
                path: "assets/counter.svg",
                text: include_str!("../templates/counter/counter.svg").to_owned(),
            },
        ]),
        Example::Minimal => files.extend([
            File {
                path: "src/main.rs",
                text: render(include_str!("../templates/minimal/main.rs")),
            },
            File {
                path: "assets/hello.svg",
                text: include_str!("../templates/minimal/hello.svg").to_owned(),
            },
        ]),
    }
    let mit = || render(include_str!("../templates/common/LICENSE-MIT"));
    let apache = || include_str!("../templates/common/LICENSE-APACHE").to_owned();
    match a.license {
        License::Mit => files.push(File {
            path: "LICENSE",
            text: mit(),
        }),
        License::Apache => files.push(File {
            path: "LICENSE",
            text: apache(),
        }),
        License::MitOrApache => files.extend([
            File {
                path: "LICENSE-MIT",
                text: mit(),
            },
            File {
                path: "LICENSE-APACHE",
                text: apache(),
            },
        ]),
        License::None => {}
    }
    if a.ci {
        files.push(File {
            path: ".github/workflows/release.yml",
            text: render(include_str!("../templates/common/release.yml")),
        });
    }
    files
}

/// A quoted string, valid in both TOML and Rust.
fn quoted(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// Text for one line of a comment or Markdown: no line breaks.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn values(a: &Answers, year: i32) -> Vec<(&'static str, String)> {
    let sdk = match &a.sdk {
        Sdk::CratesIo => quoted(crate::SDK_VERSION),
        Sdk::Path(path) => format!("{{ path = {} }}", quoted(&path.to_string_lossy())),
        Sdk::Git { url, branch: None } => format!("{{ git = {} }}", quoted(url)),
        Sdk::Git {
            url,
            branch: Some(b),
        } => format!("{{ git = {}, branch = {} }}", quoted(url), quoted(b)),
    };
    let install_cli = match &a.sdk {
        Sdk::CratesIo => format!(
            "cargo install deckly-cli --version {} --locked",
            crate::VERSION
        ),
        Sdk::Git { url, branch } => {
            let branch = branch
                .as_ref()
                .map(|b| format!(" --branch {b}"))
                .unwrap_or_default();
            format!("cargo install --git {url}{branch} deckly-cli --locked")
        }
        // A local SDK cannot be reached from CI; the tool comes from the
        // Deckly repository.
        Sdk::Path(_) => {
            "cargo install --git https://github.com/deckly-app/Deckly deckly-cli --locked".into()
        }
    };
    let license_line = match a.license.spdx() {
        "" => String::new(),
        spdx => format!("license = {}\n", quoted(spdx)),
    };
    vec![
        ("crate", quoted(&a.crate_name)),
        ("crate_text", a.crate_name.clone()),
        ("name", quoted(&a.name)),
        ("name_text", one_line(&a.name)),
        ("name_comment", one_line(&a.name)),
        ("id", quoted(&a.id)),
        ("id_text", a.id.clone()),
        ("id_comment", a.id.clone()),
        ("author", quoted(&a.author)),
        ("author_text", one_line(&a.author)),
        ("description", quoted(&a.description)),
        ("description_text", one_line(&a.description)),
        ("license_line", license_line),
        ("sdk", sdk),
        ("install_cli", install_cli),
        ("year", year.to_string()),
        (
            "workspace",
            if a.own_workspace {
                "# Its own workspace, apart from the one in a folder above.\n[workspace]\n\n".into()
            } else {
                String::new()
            },
        ),
    ]
}

fn render(template: &str, values: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("%%") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let value = after.find("%%").and_then(|end| {
            let key = &after[..end];
            values
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (v, end))
        });
        match value {
            Some((v, end)) => {
                out.push_str(v);
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("%%");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Writes the files into `dir`, which must not exist or be empty.
pub fn write(dir: &Path, files: &[File]) -> Result<(), String> {
    for f in files {
        let path: PathBuf = dir.join(f.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, &f.text)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers() -> Answers {
        Answers {
            name: "Ada's \"Clock\"".into(),
            crate_name: "adas-clock".into(),
            id: "com.ada.clock".into(),
            author: "Ada \\ Lovelace".into(),
            description: "Shows the time.\nTwo lines.".into(),
            example: Example::Counter,
            license: License::MitOrApache,
            sdk: Sdk::CratesIo,
            ci: true,
            own_workspace: false,
        }
    }

    #[test]
    fn every_value_is_filled_in_and_escaped() {
        let files = files(&answers(), 2026);
        for f in &files {
            // Only GitHub's own ${{ }} stays.
            assert!(!f.text.contains("%%"), "{}: {}", f.path, f.text);
        }
        let get = |p: &str| &files.iter().find(|f| f.path == p).unwrap().text;
        assert!(get("Cargo.toml").contains(r#"authors = ["Ada \\ Lovelace"]"#));
        assert!(get("Cargo.toml").contains(r#"license = "MIT OR Apache-2.0""#));
        assert!(get("Cargo.toml").contains(r#"deckly-sdk = "0.0.1""#));
        assert!(
            get("src/main.rs")
                .contains(r#"plugin!("com.ada.clock", "Ada's \"Clock\"", [Counter])"#)
        );
        assert!(get("LICENSE-MIT").contains("Copyright (c) 2026 Ada \\ Lovelace"));
        assert!(get("LICENSE-APACHE").contains("Apache License"));
        assert!(get("README.md").contains("Shows the time. Two lines."));
        assert!(get(".github/workflows/release.yml").contains("${{ matrix.os }}"));
    }

    #[test]
    fn choices_change_the_files() {
        let mut a = answers();
        a.example = Example::Minimal;
        a.license = License::None;
        a.ci = false;
        a.own_workspace = true;
        a.sdk = Sdk::Git {
            url: "https://example.com/deckly".into(),
            branch: Some("dev".into()),
        };
        let files = files(&a, 2026);
        let paths: Vec<&str> = files.iter().map(|f| f.path).collect();
        assert!(paths.contains(&"assets/hello.svg"));
        assert!(!paths.iter().any(|p| p.starts_with("LICENSE")));
        assert!(!paths.iter().any(|p| p.starts_with(".github")));
        let cargo = &files[0].text;
        assert!(cargo.contains(r#"{ git = "https://example.com/deckly", branch = "dev" }"#));
        assert!(!cargo.contains("license ="));
        assert!(cargo.contains("\n[workspace]\n"));
    }

    /// Cargo treats a folder holding a file named `Cargo.toml` as another
    /// package: `cargo publish` leaves it out of this crate (so an installed
    /// `deckly-cli` would not build), and a git dependency on the repository
    /// reports the template as a broken manifest.
    #[test]
    fn no_template_is_named_like_a_cargo_manifest() {
        fn walk(dir: &std::path::Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if entry.file_name() == "Cargo.toml" {
                    found.push(path.display().to_string());
                }
            }
        }
        let mut found = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates"),
            &mut found,
        );
        assert_eq!(found, Vec::<String>::new());
    }
}
