//! A whole plugin author's start, without a terminal: `create --yes`
//! against the SDK in this repository, then the new project's own tests,
//! `build`, `validate` (with the smoke start), `pack` and `link`. Deckly's
//! settings folder is a throwaway one; the user's is never touched.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CLI: &str = env!("CARGO_BIN_EXE_deckly-cli");

/// The repository root, which holds crates/deckly-sdk.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs a command with the throwaway settings folder and a build folder
/// shared by these tests, and fails with its output when it fails.
fn run(command: &mut Command, config: &Path) -> Output {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("create-target");
    let output = command
        .env("DECKLY_CONFIG_DIR", config)
        .env("CARGO_TARGET_DIR", &target)
        .env_remove("DECKLY_SDK_PATH")
        .env_remove("DECKLY_SDK_GIT")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{command:?} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn cli(dir: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(CLI);
    c.args(args).current_dir(dir);
    c
}

#[test]
fn create_yes_makes_a_project_that_tests_builds_validates_and_packs() {
    let root = folder("create-counter");
    let config = root.join("config");
    let sdk = repository();
    run(
        &mut cli(
            &root,
            &[
                "create",
                "Test Counter",
                "--yes",
                "--author",
                "Deckly Tests",
                "--no-git",
                "--sdk-path",
                sdk.to_str().unwrap(),
            ],
        ),
        &config,
    );
    let project = root.join("test-counter");
    for file in [
        "Cargo.toml",
        "src/main.rs",
        "assets/plugin.svg",
        "assets/counter.svg",
        "README.md",
        "LICENSE",
        ".gitignore",
        ".github/workflows/release.yml",
    ] {
        assert!(project.join(file).is_file(), "{file} is missing");
    }
    let cargo_toml = std::fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(
        cargo_toml.contains("deckly-sdk = { path = "),
        "{cargo_toml}"
    );

    // create built and linked it already.
    let id = "com.deckly-tests.test-counter";
    let links = std::fs::read_to_string(config.join("dev-plugins.json")).unwrap();
    assert!(links.contains(id), "{links}");

    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let tests = run(
        Command::new(&cargo).arg("test").current_dir(&project),
        &config,
    );
    let text = String::from_utf8_lossy(&tests.stdout);
    assert!(
        text.contains("counts_presses_and_saves_the_count ... ok"),
        "{text}"
    );

    run(&mut cli(&project.join("src"), &["build"]), &config);
    let validated = run(&mut cli(&project, &["validate"]), &config);
    let text = String::from_utf8_lossy(&validated.stdout);
    assert!(text.contains("answered hello"), "{text}");

    let packed = root.join("out").join("test-counter.decklyPlugin");
    run(
        &mut cli(&project, &["pack", "-o", packed.to_str().unwrap()]),
        &config,
    );
    let bytes = std::fs::read(&packed).unwrap();
    // A zip file whose entries sit in the plugin id's folder.
    assert!(bytes.starts_with(b"PK"));
    let needle = format!("{id}/deckly-plugin.json");
    assert!(bytes.windows(needle.len()).any(|w| w == needle.as_bytes()));

    run(&mut cli(&project, &["unlink"]), &config);
    let links = std::fs::read_to_string(config.join("dev-plugins.json")).unwrap();
    assert!(!links.contains(id), "{links}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_minimal_example_passes_its_own_test() {
    let root = folder("create-minimal");
    let config = root.join("config");
    let sdk = repository();
    run(
        &mut cli(
            &root,
            &[
                "new",
                "Tiny",
                "--yes",
                "--author",
                "Deckly Tests",
                "--example",
                "minimal",
                "--license",
                "mit-or-apache-2.0",
                "--no-git",
                "--no-ci",
                "--no-build",
                "--no-link",
                "--sdk-path",
                sdk.to_str().unwrap(),
            ],
        ),
        &config,
    );
    let project = root.join("tiny");
    assert!(project.join("LICENSE-MIT").is_file());
    assert!(project.join("LICENSE-APACHE").is_file());
    assert!(!project.join(".github").exists());
    // --no-link: nothing was written into Deckly's folder.
    assert!(!config.exists());
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let tests = run(
        Command::new(&cargo).arg("test").current_dir(&project),
        &config,
    );
    assert!(String::from_utf8_lossy(&tests.stdout).contains("a_press_shows_ok ... ok"));
    run(&mut cli(&project, &["validate"]), &config);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_a_terminal_or_yes_it_lists_the_missing_flags() {
    let root = folder("create-missing");
    let output = Command::new(CLI)
        .args(["create", "Clock"])
        .current_dir(&root)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(
        text.contains("--author") && text.contains("--yes"),
        "{text}"
    );
    // Nothing was written.
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
}
