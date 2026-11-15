//! `cargo dev sdk` and `cargo dev install-cli`: check the plugin crates of
//! this checkout and install its plugin tool, for work on the SDK and the CLI
//! before they are published.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// The plugin crates and the example plugins built on them.
const PACKAGES: [&str; 5] = [
    "deckly-plugin-protocol",
    "deckly-sdk",
    "deckly-cli",
    "hello-deckly",
    "counter-plugin",
];

/// `cargo dev sdk [--fix]`: format check, clippy and tests of the plugin
/// crates and examples.
pub fn check(args: impl Iterator<Item = OsString>) -> Result<bool, String> {
    let mut fix = false;
    for arg in args {
        match arg.to_str() {
            Some("--fix") => fix = true,
            Some("--help" | "-h") => {
                println!("{CHECK_HELP}");
                return Ok(true);
            }
            _ => return Err(unknown(&arg)),
        }
    }
    run_checks(fix)
}

/// `cargo dev install-cli [--no-check] [--debug]`: checks, then installs
/// `deckly-cli` and `cargo-deckly` from this checkout.
pub fn install(args: impl Iterator<Item = OsString>) -> Result<bool, String> {
    let mut check = true;
    let mut debug = false;
    for arg in args {
        match arg.to_str() {
            Some("--no-check") => check = false,
            Some("--debug") => debug = true,
            Some("--help" | "-h") => {
                println!("{INSTALL_HELP}");
                return Ok(true);
            }
            _ => return Err(unknown(&arg)),
        }
    }
    if check && !run_checks(false)? {
        eprintln!("deckly dev: checks failed; nothing was installed");
        return Ok(false);
    }
    if !install_tool(debug)? {
        return Ok(false);
    }
    print_next_steps(crate::root()?);
    Ok(true)
}

/// Installs `deckly-cli` and `cargo-deckly` from this checkout into Cargo's
/// bin folder, without checks. `cargo install --path` builds in the
/// workspace's target folder, so unchanged crates are not rebuilt.
pub fn install_tool(debug: bool) -> Result<bool, String> {
    let root = crate::root()?;
    let cli = root.join("crates").join("deckly-cli");
    let mut install = Command::new(crate::cargo());
    install
        .arg("install")
        .arg("--path")
        .arg(&cli)
        .args(["--locked", "--force"])
        .args(debug.then_some("--debug"));
    step("install deckly-cli", install.current_dir(root))
}

fn run_checks(fix: bool) -> Result<bool, String> {
    let root = crate::root()?;
    let packages: Vec<String> = PACKAGES
        .iter()
        .flat_map(|p| ["--package".to_owned(), (*p).to_owned()])
        .collect();
    let cargo = crate::cargo();

    let mut fmt = Command::new(&cargo);
    fmt.arg("fmt").args(&packages);
    if !fix {
        fmt.args(["--", "--check"]);
    }
    let mut clippy = Command::new(&cargo);
    clippy
        .args(["clippy", "--all-targets"])
        .args(&packages)
        .args(["--", "-D", "warnings"]);
    let mut test = Command::new(&cargo);
    test.arg("test").args(&packages);

    for (name, command) in [
        ("fmt", &mut fmt),
        ("clippy", &mut clippy),
        ("test", &mut test),
    ] {
        if !step(name, command.current_dir(root))? {
            if name == "fmt" && !fix {
                eprintln!("deckly dev: run `cargo dev sdk --fix` to format");
            }
            return Ok(false);
        }
    }
    Ok(true)
}

/// Runs one step and says which one it is.
fn step(name: &str, command: &mut Command) -> Result<bool, String> {
    eprintln!("deckly dev: {name}");
    command
        .status()
        .map(|s| s.success())
        .map_err(|e| format!("cannot run {name}: {e}"))
}

fn print_next_steps(root: &Path) {
    let sdk = root.join("crates").join("deckly-sdk");
    println!(
        "\nInstalled deckly-cli and cargo-deckly from this checkout.\n\
         New projects take deckly-sdk from this checkout with:\n\
         \n    deckly-cli create --sdk-path {sdk}\n\
         \n\
         or once per shell: DECKLY_SDK_PATH={sdk}",
        sdk = sdk.display()
    );
}

fn unknown(arg: &OsString) -> String {
    format!("unknown option {}; use --help", arg.to_string_lossy())
}

const CHECK_HELP: &str = "cargo dev sdk [--fix]\n\
Checks deckly-plugin-protocol, deckly-sdk, deckly-cli and the example plugins:\n\
format, clippy with warnings as errors, tests.\n\
--fix  Format the code instead of only checking it.";

const INSTALL_HELP: &str = "cargo dev install-cli [--no-check] [--debug]\n\
Runs `cargo dev sdk`, then installs deckly-cli and cargo-deckly from this\n\
checkout into Cargo's bin folder, replacing an installed copy.\n\
--no-check  Install without the checks.\n\
--debug     Install a debug build (faster to compile).";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_options_stop_before_anything_runs() {
        assert!(check([OsString::from("--relase")].into_iter()).is_err());
        assert!(install([OsString::from("--no-chek")].into_iter()).is_err());
    }

    #[test]
    fn checks_cover_every_plugin_crate_and_example() {
        for package in ["deckly-plugin-protocol", "deckly-sdk", "deckly-cli"] {
            assert!(PACKAGES.contains(&package));
        }
        let manifest = std::fs::read_to_string(crate::root().unwrap().join("Cargo.toml")).unwrap();
        for package in PACKAGES {
            let name = package.trim_start_matches("deckly-");
            assert!(manifest.contains(name), "{package} is not in the workspace");
        }
    }
}
