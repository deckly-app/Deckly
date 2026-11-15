//! `cargo dev book`: builds the plugin SDK book (`docs/sdk`) into
//! `target/book`, the site published on GitHub Pages:
//!
//! - the book itself, by mdBook;
//! - the API reference of `deckly-sdk` and `deckly-plugin-protocol` under
//!   `api/`, by rustdoc;
//! - for AI agents: `llms.txt` (an index), `llms-full.txt` (every page in
//!   one file) and each page as Markdown next to its HTML.
//!
//! The book's checks (pages listed, links, tested examples, code block
//! languages) run first, and in `cargo test -p xtask`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{cargo, root};

/// Where the site is published.
const SITE: &str = "https://deckly-app.github.io/Deckly/";
/// The book, from the workspace root.
const BOOK: &str = "docs/sdk";
/// The file that lists the pages tested as doctests.
const DOCTESTS: &str = "crates/deckly-sdk/src/lib.rs";

pub fn run(args: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    if let Some(arg) = args.into_iter().next() {
        return Err(format!(
            "unknown book option {}; use cargo dev --help",
            arg.to_string_lossy()
        ));
    }
    let root = root()?;
    let book = Book::load(&root.join(BOOK))?;
    let doctests = read(&root.join(DOCTESTS))?;
    let problems = book.check(&doctests);
    if !problems.is_empty() {
        return Err(format!(
            "the book has problems:\n  {}",
            problems.join("\n  ")
        ));
    }

    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let out = target.join("book");

    let built = Command::new("mdbook")
        .arg("build")
        .arg(root.join(BOOK))
        .arg("--dest-dir")
        .arg(&out)
        .status()
        .map_err(|e| {
            format!("cannot run mdbook ({e}); install it with `cargo install mdbook --locked`")
        })?;
    if !built.success() {
        return Err("mdbook build failed".into());
    }
    // The pages sit next to book.toml, so mdBook copies it; the site does not
    // need it.
    let _ = fs::remove_file(out.join("book.toml"));

    let documented = Command::new(cargo())
        .args(["doc", "--no-deps", "--locked"])
        .args(["-p", "deckly-sdk", "-p", "deckly-plugin-protocol"])
        .current_dir(root)
        .status()
        .map_err(|e| format!("cannot run cargo doc: {e}"))?;
    if !documented.success() {
        return Err("cargo doc failed".into());
    }
    let api = out.join("api");
    copy_dir(&target.join("doc"), &api)?;
    write(
        &api.join("index.html"),
        "<!doctype html><meta charset=\"utf-8\">\
         <meta http-equiv=\"refresh\" content=\"0; url=deckly_sdk/\">\
         <title>Deckly SDK API</title><a href=\"deckly_sdk/\">deckly_sdk</a>\n",
    )?;

    for page in &book.pages {
        write(&out.join(&page.path), &Book::agent_text(page))?;
    }
    write(&out.join("llms.txt"), &book.llms_index())?;
    write(&out.join("llms-full.txt"), &book.llms_full())?;
    println!("The site is in {}", out.display());
    Ok(())
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let entries = fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", from.display()))?;
        let kind = entry
            .file_type()
            .map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &dest)
                .map_err(|e| format!("cannot copy {}: {e}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// One page, as `SUMMARY.md` lists it.
struct Page {
    /// Relative to the book, with `/`.
    path: String,
    title: String,
    /// The `SUMMARY.md` part it is in.
    part: String,
    text: String,
}

struct Book {
    dir: PathBuf,
    title: String,
    description: String,
    pages: Vec<Page>,
}

impl Book {
    fn load(dir: &Path) -> Result<Self, String> {
        let config = read(&dir.join("book.toml"))?;
        let setting = |name: &str| {
            config
                .lines()
                .find_map(|l| l.strip_prefix(name)?.trim_start().strip_prefix('='))
                .map(|v| v.trim().trim_matches('"').to_owned())
                .unwrap_or_default()
        };
        let mut pages = Vec::new();
        for (path, title, part) in summary(&read(&dir.join("SUMMARY.md"))?) {
            let text = read(&dir.join(&path))?;
            pages.push(Page {
                path,
                title,
                part,
                text,
            });
        }
        Ok(Self {
            dir: dir.to_owned(),
            title: setting("title"),
            description: setting("description"),
            pages,
        })
    }

    /// Everything wrong with the book, one line each.
    fn check(&self, doctests: &str) -> Vec<String> {
        let mut problems = Vec::new();
        let listed: BTreeSet<&str> = self.pages.iter().map(|p| p.path.as_str()).collect();
        for file in markdown_files(&self.dir, "") {
            if file != "SUMMARY.md" && !listed.contains(file.as_str()) {
                problems.push(format!("{file} is not in SUMMARY.md"));
            }
        }
        for page in &self.pages {
            if !doctests.contains(&format!("\"{}\"", page.path)) {
                problems.push(format!(
                    "{} is not tested: list it in the book module of {DOCTESTS}",
                    page.path
                ));
            }
            for (line, fence) in fences(&page.text) {
                if fence.is_empty() {
                    problems.push(format!(
                        "{}:{line}: a code block without a language",
                        page.path
                    ));
                }
            }
            for (line, link) in links(&page.text) {
                if let Err(why) = self.resolve(&page.path, &link) {
                    problems.push(format!("{}:{line}: {link}: {why}", page.path));
                }
            }
            for (line, spec, shown) in examples(&page.text) {
                let base = self.dir.join(&page.path);
                let base = base.parent().unwrap_or(&self.dir);
                match source(base, &spec) {
                    Ok(source) if source == shown => {}
                    Ok(_) => problems.push(format!(
                        "{}:{line}: the code differs from {spec}; copy it again",
                        page.path
                    )),
                    Err(why) => problems.push(format!("{}:{line}: {why}", page.path)),
                }
            }
        }
        problems
    }

    /// Checks a link from `page`; web links are not followed.
    fn resolve(&self, page: &str, link: &str) -> Result<(), String> {
        if link.contains("://") || link.starts_with("mailto:") {
            return Ok(());
        }
        let (file, anchor) = link.split_once('#').unwrap_or((link, ""));
        let target = if file.is_empty() {
            page.to_owned()
        } else {
            join(page, file).ok_or("leaves the book")?
        };
        let Some(page) = self.pages.iter().find(|p| p.path == target) else {
            return Err(if self.dir.join(&target).exists() {
                "not a page of the book".into()
            } else {
                "no such file".into()
            });
        };
        if !anchor.is_empty() && !headings(&page.text).any(|h| slug(&h) == anchor) {
            return Err(format!("{target} has no heading #{anchor}"));
        }
        Ok(())
    }

    /// A page as an agent should read it: the lines hidden on the site
    /// shown, so every Rust example is complete.
    fn agent_text(page: &Page) -> String {
        let mut out = String::new();
        let mut in_rust = false;
        let mut in_fence = false;
        for line in page.text.lines() {
            let trimmed = line.trim();
            if let Some(info) = trimmed.strip_prefix("```") {
                if in_fence {
                    in_fence = false;
                    in_rust = false;
                } else {
                    in_fence = true;
                    in_rust = info.split(',').next() == Some("rust");
                    if in_rust {
                        out.push_str("```rust\n");
                        continue;
                    }
                }
            } else if in_rust {
                if line == "#" {
                    out.push('\n');
                    continue;
                }
                if let Some(rest) = line.strip_prefix("# ") {
                    out.push_str(rest);
                    out.push('\n');
                    continue;
                }
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    fn llms_index(&self) -> String {
        let mut out = format!(
            "# {}\n\n> {}\n\nEvery page below is Markdown. All of them in one file: {SITE}llms-full.txt\n",
            self.title, self.description
        );
        let mut part = None;
        for page in &self.pages {
            if part != Some(&page.part) {
                let name = if page.part.is_empty() {
                    "Start"
                } else {
                    &page.part
                };
                out.push_str(&format!("\n## {name}\n\n"));
                part = Some(&page.part);
            }
            out.push_str(&format!("- [{}]({SITE}{})\n", page.title, page.path));
        }
        out
    }

    fn llms_full(&self) -> String {
        let mut out = format!("# {}\n\n> {}\n", self.title, self.description);
        for page in &self.pages {
            let html = page
                .path
                .strip_suffix(".md")
                .map(|p| if p == "README" { "index" } else { p })
                .unwrap_or(&page.path);
            out.push_str(&format!(
                "\n\n<!-- Page: {} ({SITE}{html}.html) -->\n\n",
                page.path
            ));
            out.push_str(&Self::agent_text(page));
        }
        out
    }
}

/// `(path, title, part)` for each link in `SUMMARY.md`, in order.
fn summary(text: &str) -> Vec<(String, String, String)> {
    let mut part = String::new();
    let mut pages = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(heading) = line.strip_prefix("# ") {
            if heading != "Summary" {
                heading.clone_into(&mut part);
            }
        } else if line == "---" {
            "More".clone_into(&mut part);
        } else if let Some((title, rest)) = line
            .trim_start_matches("- ")
            .strip_prefix('[')
            .and_then(|l| l.split_once("]("))
            && let Some(path) = rest.strip_suffix(')')
            && path.ends_with(".md")
        {
            pages.push((path.to_owned(), title.to_owned(), part.clone()));
        }
    }
    pages
}

/// Every `.md` file under `dir`, relative to the book.
fn markdown_files(dir: &Path, prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = format!("{prefix}{name}");
        let kind = entry.file_type();
        if kind.as_ref().is_ok_and(fs::FileType::is_dir) {
            found.extend(markdown_files(&entry.path(), &format!("{path}/")));
        } else if name.ends_with(".md") {
            found.push(path);
        }
    }
    found
}

/// The info string of each opening code fence, with its line number.
fn fences(text: &str) -> Vec<(usize, String)> {
    let mut open = false;
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(info) = line.trim_start().strip_prefix("```") {
            if !open {
                found.push((i + 1, info.trim().to_owned()));
            }
            open = !open;
        }
    }
    found
}

/// Markdown link targets outside code, with their line numbers: inline
/// links and reference definitions.
fn links(text: &str) -> Vec<(usize, String)> {
    let mut open = false;
    let mut found = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            open = !open;
            continue;
        }
        if open {
            continue;
        }
        // Inline code may show link-like text; leave it out.
        let prose: String = line.split('`').step_by(2).collect::<Vec<_>>().join(" ");
        let mut rest = prose.as_str();
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            let end = rest.find(')').unwrap_or(rest.len());
            found.push((i + 1, rest[..end].trim().to_owned()));
            rest = &rest[end..];
        }
        if let Some(target) = prose
            .trim_start()
            .strip_prefix('[')
            .and_then(|l| l.split_once("]: "))
            .map(|(_, t)| t.trim())
        {
            found.push((i + 1, target.to_owned()));
        }
    }
    found
}

/// `file` relative to the folder of `page`, both relative to the book.
fn join(page: &str, file: &str) -> Option<String> {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for part in file.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            name => parts.push(name),
        }
    }
    Some(parts.join("/"))
}

/// The text of each heading outside code.
fn headings(text: &str) -> impl Iterator<Item = String> + '_ {
    let mut open = false;
    text.lines().filter_map(move |line| {
        if line.trim_start().starts_with("```") {
            open = !open;
            return None;
        }
        if open {
            return None;
        }
        let heading = line.trim_start_matches('#');
        (heading.len() < line.len() && heading.starts_with(' ')).then(|| heading.trim().to_owned())
    })
}

/// The anchor mdBook gives a heading.
fn slug(heading: &str) -> String {
    heading
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                Some(c.to_lowercase().collect::<String>())
            } else if c.is_whitespace() {
                Some("-".into())
            } else {
                None
            }
        })
        .collect()
}

/// Code blocks copied from a file, each after a comment
/// `<!-- example: path:anchor -->`: the comment's line, the spec and the
/// block's text.
fn examples(text: &str) -> Vec<(usize, String, String)> {
    let mut found = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((i, line)) = lines.next() {
        let Some(spec) = line
            .trim()
            .strip_prefix("<!-- example:")
            .and_then(|s| s.strip_suffix("-->"))
        else {
            continue;
        };
        let mut code = String::new();
        if lines
            .next()
            .is_some_and(|(_, l)| l.trim_start().starts_with("```"))
        {
            for (_, l) in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                code.push_str(l);
                code.push('\n');
            }
        }
        found.push((i + 1, spec.trim().to_owned(), code));
    }
    found
}

/// The lines of `path:anchor` between its `ANCHOR:` and `ANCHOR_END:` lines,
/// or all of `path`, without any anchor lines: what mdBook's include shows.
fn source(base: &Path, spec: &str) -> Result<String, String> {
    let (file, anchor) = match spec.rsplit_once(':') {
        Some((file, anchor)) if !anchor.contains('/') => (file, Some(anchor)),
        _ => (spec, None),
    };
    let text = read(&base.join(file))?;
    let is_anchor = |l: &str| l.contains("ANCHOR:") || l.contains("ANCHOR_END:");
    let mut out = String::new();
    let mut inside = anchor.is_none();
    let mut found = anchor.is_none();
    for line in text.lines() {
        if let Some(name) = anchor {
            let t = line.trim();
            if t.ends_with(&format!("ANCHOR: {name}")) {
                inside = true;
                found = true;
                continue;
            }
            if t.ends_with(&format!("ANCHOR_END: {name}")) {
                inside = false;
                continue;
            }
        }
        if inside && !is_anchor(line) {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !found {
        return Err(format!("{file} has no anchor {spec}"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sdk_book_passes_its_checks() {
        let root = root().unwrap();
        let book = Book::load(&root.join(BOOK)).unwrap();
        let doctests = read(&root.join(DOCTESTS)).unwrap();
        assert_eq!(book.check(&doctests), Vec::<String>::new());
        assert!(book.pages.len() > 10);
        assert!(
            book.llms_full()
                .contains("fn main() -> deckly_sdk::Result<()>")
        );
    }

    #[test]
    fn summary_lists_pages_by_part() {
        let pages = summary(
            "# Summary\n\n[Intro](README.md)\n\n- [Start](start.md)\n\n# Guides\n\n\
             - [Keys](guide/keys.md)\n\n---\n\n[About](about.md)\n",
        );
        let parts: Vec<_> = pages
            .iter()
            .map(|(p, t, part)| (p.as_str(), t.as_str(), part.as_str()))
            .collect();
        assert_eq!(
            parts,
            [
                ("README.md", "Intro", ""),
                ("start.md", "Start", ""),
                ("guide/keys.md", "Keys", "Guides"),
                ("about.md", "About", "More"),
            ]
        );
    }

    #[test]
    fn links_skip_code_and_find_references() {
        let text = "See [a](a.md) and `[b](b.md)`.\n```rust\n[c](c.md)\n```\n[d]: https://x/\n";
        assert_eq!(
            links(text),
            [(1, "a.md".to_owned()), (5, "https://x/".to_owned())]
        );
    }

    #[test]
    fn code_blocks_must_name_a_language() {
        let text = "```rust\nx\n```\n\n```\ny\n```\n";
        assert_eq!(fences(text), [(1, "rust".to_owned()), (5, String::new())]);
    }

    #[test]
    fn relative_links_resolve_inside_the_book() {
        assert_eq!(join("guide/a.md", "b.md").as_deref(), Some("guide/b.md"));
        assert_eq!(
            join("guide/a.md", "../reference/c.md").as_deref(),
            Some("reference/c.md")
        );
        assert_eq!(join("a.md", "../../x.md"), None);
    }

    #[test]
    fn slugs_match_mdbook() {
        assert_eq!(
            slug("Slow work on another thread"),
            "slow-work-on-another-thread"
        );
        assert_eq!(slug("deckly-plugin.json"), "deckly-pluginjson");
        assert_eq!(slug("1. What you need"), "1-what-you-need");
    }

    #[test]
    fn agent_text_shows_hidden_lines_and_plain_rust_fences() {
        let page = Page {
            path: "p.md".into(),
            title: String::new(),
            part: String::new(),
            text: "# Title\n```rust,no_run\n# use x;\n#\n#[derive(Default)]\nfn f() {}\n```\n# Not code\n"
                .into(),
        };
        assert_eq!(
            Book::agent_text(&page),
            "# Title\n```rust\nuse x;\n\n#[derive(Default)]\nfn f() {}\n```\n# Not code\n"
        );
    }

    #[test]
    fn examples_are_found_and_sources_take_the_anchored_lines() {
        let page = "<!-- example: main.rs:b -->\n```rust\nfn b() {}\n```\n";
        assert_eq!(
            examples(page),
            [(1, "main.rs:b".to_owned(), "fn b() {}\n".to_owned())]
        );

        let dir = std::env::temp_dir().join(format!("xtask-book-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("main.rs"),
            "// head\n// ANCHOR: a\nfn a() {}\n// ANCHOR: b\nfn b() {}\n// ANCHOR_END: b\n// ANCHOR_END: a\n",
        )
        .unwrap();
        assert_eq!(source(&dir, "main.rs:b").unwrap(), "fn b() {}\n");
        assert_eq!(source(&dir, "main.rs:a").unwrap(), "fn a() {}\nfn b() {}\n");
        assert_eq!(
            source(&dir, "main.rs").unwrap(),
            "// head\nfn a() {}\nfn b() {}\n"
        );
        assert!(source(&dir, "main.rs:missing").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
