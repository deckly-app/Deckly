//! `create` (alias `new`): a few questions, then a ready project in a new
//! folder: the plugin with an example action and its tests, icons, a
//! README, a licence, a release workflow and a git repository; built and
//! linked into Deckly.
//!
//! Every question is also a flag. With `--yes` the questions without a flag
//! take their defaults; without a terminal the tool never waits on a prompt:
//! it says which flags are missing. Nothing is written before the answers
//! are complete and the folder is free.

use crate::args::{CreateArgs, Example, License, Sdk};
use crate::config::{config_dir, plugins_dir};
use crate::project::{Project, cargo};
use crate::templates::{self, Answers};
use deckly_plugin_protocol::limits::{MAX_DESCRIPTION_CHARS, MAX_ID_BYTES, MAX_NAME_CHARS};
use deckly_plugin_protocol::valid_id;
use inquire::validator::Validation;
use inquire::{Confirm, InquireError, Select, Text};
use std::fmt;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Longest plugin name the wizard accepts.
const MAX_NAME: usize = 64;

pub fn run(args: CreateArgs) -> Result<(), String> {
    let interactive =
        !args.yes && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let parent = match &args.dir {
        Some(dir) => dir.clone(),
        None => {
            std::env::current_dir().map_err(|e| format!("cannot read the current folder: {e}"))?
        }
    };
    let sdk = resolve_sdk(&args.sdk)?;
    let mut answers = if interactive {
        ask(&args, sdk)?
    } else {
        unattended(&args, sdk)?
    };
    answers.own_workspace = in_workspace(&parent);
    let folder = parent.join(&answers.crate_name);
    check_folder(&folder)?;
    warn_on_clash(&args, &answers.id);
    if interactive && !confirm(&answers, &folder)? {
        println!("Nothing was written.");
        return Ok(());
    }
    make(&args, &answers, &folder)
}

/// The answers from flags and defaults, without asking.
fn unattended(args: &CreateArgs, sdk: Sdk) -> Result<Answers, String> {
    if !args.yes {
        let mut missing = Vec::new();
        if args.name.is_none() {
            missing.push("<name>");
        }
        for (flag, given) in [
            ("--author", args.author.is_some()),
            ("--id", args.id.is_some()),
            ("--description", args.description.is_some()),
            ("--example", args.example.is_some()),
            ("--license", args.license.is_some()),
        ] {
            if !given {
                missing.push(flag);
            }
        }
        if !missing.is_empty() {
            return Err(format!(
                "there is no terminal to ask in; give {} or --yes for the defaults",
                missing.join(", ")
            ));
        }
    }
    let name = args
        .name
        .clone()
        .ok_or("give the plugin's name: deckly-cli create <name> --yes")?;
    check_name(&name)?;
    let author = match &args.author {
        Some(a) => a.clone(),
        None => default_author(),
    };
    check_author(&author)?;
    let id = match &args.id {
        Some(id) => id.clone(),
        None => default_id(&author, &name),
    };
    check_id(&id)?;
    let description = match &args.description {
        Some(d) => d.clone(),
        None => default_description(&name),
    };
    check_description(&description)?;
    Ok(Answers {
        crate_name: crate_name(&name)?,
        name,
        id,
        author,
        description,
        example: args.example.unwrap_or_default(),
        license: args.license.unwrap_or_default(),
        sdk,
        ci: args.ci,
        own_workspace: false,
    })
}

/// Asks what the flags did not give.
fn ask(args: &CreateArgs, sdk: Sdk) -> Result<Answers, String> {
    println!("A new Deckly plugin. Enter takes the suggestion; Esc or Ctrl+C stops.\n");
    let author = match &args.author {
        Some(a) => a.clone(),
        None => prompt(
            Text::new("Author:")
                .with_default(&default_author())
                .with_validator(|s: &str| valid(check_author(s))),
        )?,
    };
    let name = match &args.name {
        Some(n) => n.clone(),
        None => prompt(
            Text::new("Plugin name:")
                .with_help_message("Shown in Deckly's action list")
                .with_validator(|s: &str| {
                    valid(check_name(s).and_then(|()| crate_name(s).map(drop)))
                }),
        )?,
    };
    check_name(&name)?;
    let id = match &args.id {
        Some(id) => id.clone(),
        None => prompt(
            Text::new("Plugin id:")
                .with_default(&default_id(&author, &name))
                .with_help_message("Reverse-DNS; never change it after the first release")
                .with_validator(|s: &str| valid(check_id(s))),
        )?,
    };
    let description = match &args.description {
        Some(d) => d.clone(),
        None => prompt(
            Text::new("Description:")
                .with_default(&default_description(&name))
                .with_validator(|s: &str| valid(check_description(s))),
        )?,
    };
    let example = match args.example {
        Some(e) => e,
        None => {
            let options: Vec<Shown<Example>> = Example::ALL
                .iter()
                .map(|e| Shown(*e, e.describe()))
                .collect();
            prompt(Select::new("Example:", options))?.0
        }
    };
    let license = match args.license {
        Some(l) => l,
        None => {
            let options: Vec<Shown<License>> = License::ALL
                .iter()
                .map(|l| Shown(*l, l.describe()))
                .collect();
            prompt(Select::new("Licence:", options))?.0
        }
    };
    Ok(Answers {
        crate_name: crate_name(&name)?,
        name,
        id,
        author,
        description,
        example,
        license,
        sdk,
        ci: args.ci,
        own_workspace: false,
    })
}

/// A choice in a list, shown by its description.
#[derive(Clone)]
struct Shown<T>(T, &'static str);

impl<T> fmt::Display for Shown<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.1)
    }
}

/// A prompt's answer; cancelling stops the tool with nothing written.
trait Prompt {
    type Out;
    fn ask(self) -> Result<Self::Out, InquireError>;
}

impl Prompt for Text<'_, '_> {
    type Out = String;
    fn ask(self) -> Result<String, InquireError> {
        self.prompt()
    }
}

impl<T: Clone + fmt::Display> Prompt for Select<'_, T> {
    type Out = T;
    fn ask(self) -> Result<T, InquireError> {
        self.prompt()
    }
}

impl Prompt for Confirm<'_> {
    type Out = bool;
    fn ask(self) -> Result<bool, InquireError> {
        self.prompt()
    }
}

fn prompt<P: Prompt>(p: P) -> Result<P::Out, String> {
    p.ask().map_err(|e| match e {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            "stopped; nothing was written".to_owned()
        }
        other => format!("cannot ask: {other}"),
    })
}

fn valid(check: Result<(), String>) -> Result<Validation, inquire::CustomUserError> {
    Ok(match check {
        Ok(()) => Validation::Valid,
        Err(e) => Validation::Invalid(e.into()),
    })
}

fn confirm(a: &Answers, folder: &Path) -> Result<bool, String> {
    println!();
    println!("  Folder       {}", folder.display());
    println!("  Package      {}", a.crate_name);
    println!("  Plugin id    {}", a.id);
    println!("  Example      {}", a.example.describe());
    println!("  Licence      {}", a.license.describe());
    let sdk = match &a.sdk {
        Sdk::CratesIo => format!("deckly-sdk {} from crates.io", crate::SDK_VERSION),
        Sdk::Path(p) => format!("deckly-sdk from {}", p.display()),
        Sdk::Git { url, .. } => format!("deckly-sdk from {url}"),
    };
    println!("  SDK          {sdk}");
    println!();
    prompt(Confirm::new("Create it?").with_default(true))
}

fn check_name(name: &str) -> Result<(), String> {
    let n = name.trim().chars().count();
    if n == 0 {
        return Err("Give the plugin a name.".into());
    }
    if n > MAX_NAME || n > MAX_NAME_CHARS {
        return Err(format!("At most {MAX_NAME} characters."));
    }
    if name.chars().any(char::is_control) {
        return Err("One line, without control characters.".into());
    }
    Ok(())
}

fn check_author(author: &str) -> Result<(), String> {
    if author.trim().is_empty() {
        return Err("Give an author.".into());
    }
    if author.chars().count() > MAX_NAME_CHARS || author.chars().any(char::is_control) {
        return Err(format!("One line of at most {MAX_NAME_CHARS} characters."));
    }
    Ok(())
}

/// The host's own id rule, plus at least two parts.
fn check_id(id: &str) -> Result<(), String> {
    if !valid_id(id) || id.len() > MAX_ID_BYTES {
        return Err("Letters, digits, '.', '-' and '_', like com.example.my-plugin.".into());
    }
    if !id.contains('.') || id.ends_with('.') {
        return Err("Reverse-DNS, with dots: com.example.my-plugin.".into());
    }
    Ok(())
}

fn check_description(text: &str) -> Result<(), String> {
    if text.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(format!("At most {MAX_DESCRIPTION_CHARS} characters."));
    }
    Ok(())
}

/// Lowercase letters and digits, the rest turned into single dashes.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// The package (and folder) name for a plugin name.
fn crate_name(name: &str) -> Result<String, String> {
    let mut s = slug(name);
    if s.is_empty() {
        return Err(
            "The name needs a letter or digit (A to Z, 0 to 9) for the folder name.".into(),
        );
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s = format!("plugin-{s}");
    }
    // Names that would clash with the dependencies or Rust itself.
    if [
        "deckly-sdk",
        "serde",
        "serde-json",
        "test",
        "core",
        "std",
        "alloc",
    ]
    .contains(&s.as_str())
    {
        s.push_str("-plugin");
    }
    Ok(s)
}

fn default_author() -> String {
    let from_git = Command::new("git")
        .args(["config", "user.name"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty());
    from_git
        .or_else(|| {
            ["USER", "USERNAME", "LOGNAME"]
                .iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.trim().is_empty()))
        })
        .unwrap_or_else(|| "Me".into())
}

fn default_id(author: &str, name: &str) -> String {
    let author = slug(author);
    let author = if author.is_empty() {
        "example".into()
    } else {
        author
    };
    let name = slug(name);
    let id = format!("com.{author}.{name}");
    if check_id(&id).is_ok() {
        id
    } else {
        "com.example.plugin".into()
    }
}

fn default_description(name: &str) -> String {
    format!("{} for Deckly.", name.trim())
}

/// Where the SDK comes from; a local folder must hold it.
fn resolve_sdk(sdk: &Sdk) -> Result<Sdk, String> {
    let Sdk::Path(path) = sdk else {
        return Ok(sdk.clone());
    };
    let path =
        std::path::absolute(path).map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    // The SDK's own folder, or a Deckly checkout holding it.
    for dir in [path.clone(), path.join("crates").join("deckly-sdk")] {
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
        if manifest.contains("name = \"deckly-sdk\"") {
            return Ok(Sdk::Path(dir));
        }
    }
    Err(format!(
        "{} holds no deckly-sdk (neither it nor its crates/deckly-sdk)",
        path.display()
    ))
}

/// Whether a folder lies inside a Cargo workspace, which would otherwise
/// claim a new package made there and refuse to build it.
fn in_workspace(parent: &Path) -> bool {
    let start = std::path::absolute(parent).unwrap_or_else(|_| parent.to_owned());
    start.ancestors().any(|dir| {
        std::fs::read_to_string(dir.join("Cargo.toml"))
            .is_ok_and(|t| t.lines().any(|l| l.trim() == "[workspace]"))
    })
}

fn check_folder(folder: &Path) -> Result<(), String> {
    match std::fs::read_dir(folder) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                return Err(format!(
                    "{} exists and is not empty; pick another name or --dir",
                    folder.display()
                ));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) if folder.exists() => {
            Err(format!("{} exists and is not a folder", folder.display()))
        }
        Err(e) => Err(format!("cannot use {}: {e}", folder.display())),
    }
}

/// A plugin with the same id already in Deckly is worth a line, not a stop.
fn warn_on_clash(args: &CreateArgs, id: &str) {
    let Ok(config) = config_dir(args.config_dir.as_deref()) else {
        return;
    };
    let installed = plugins_dir(&config).join(id).is_dir();
    let linked = crate::link::read(&config).is_ok_and(|l| l.plugins.contains_key(id));
    if installed || linked {
        println!(
            "  note    Deckly already has a plugin {id}; this one will take its place while linked"
        );
    }
}

fn make(args: &CreateArgs, a: &Answers, folder: &Path) -> Result<(), String> {
    println!("\nCreating {} in {}", a.name, folder.display());
    let files = templates::files(a, this_year());
    crate::step("write the project files", || {
        templates::write(folder, &files)
    })?;
    let has_cargo = Command::new(cargo())
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let mut built = false;
    if !args.build {
        println!("  skip    build (--no-build)");
    } else if !has_cargo {
        println!(
            "  skip    build: cargo is missing. Install Rust from https://rustup.rs, then run \
             deckly-cli dev in the project"
        );
    } else {
        println!(
            "          The first build downloads and compiles the dependencies; it takes a while."
        );
        let project = Project::find_in(folder, None)?;
        match crate::build::build(&project, false) {
            Ok(_) => built = true,
            Err(e) => println!("  note    {e}"),
        }
    }
    if args.git {
        git(folder, &a.name);
    } else {
        println!("  skip    git (--no-git)");
    }
    let mut linked = false;
    if !args.link {
        println!("  skip    link into Deckly (--no-link)");
    } else if !built {
        println!("  skip    link into Deckly: the plugin is not built");
    } else {
        let config = config_dir(args.config_dir.as_deref())?;
        let dir = Project::find_in(folder, None)?
            .target_dir
            .join("deckly")
            .join(&a.id);
        crate::step(&format!("link {} into Deckly", a.id), || {
            crate::link::add(&config, &a.id, &dir)
        })?;
        crate::link::reload(&config, &a.id)?;
        linked = true;
    }
    let short = match a.example {
        Example::Counter => "Counter",
        Example::Minimal => "Hello",
    };
    println!("\nNext:");
    println!("  cd {}", relative(folder).display());
    println!("  deckly-cli dev");
    if linked {
        println!(
            "  In Deckly, open the editor and drag {} → {short} onto a key.",
            a.name
        );
    } else {
        println!("  Then in Deckly, drag {} → {short} onto a key.", a.name);
    }
    Ok(())
}

/// Makes a git repository with a first commit, unless the folder is inside
/// one already. Whatever goes wrong is a note: the files are made anyway.
fn git(folder: &Path, name: &str) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(folder)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
    };
    let inside = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(folder)
        .stdin(Stdio::null())
        .output();
    match inside {
        Err(_) => {
            println!("  skip    git: it is not installed");
            return;
        }
        Ok(o) if o.status.success() => {
            println!("  skip    git: the folder is inside a git repository already");
            return;
        }
        Ok(_) => {}
    }
    let result = crate::step("make a git repository", || {
        let init = run(&["init", "-q"]).map_err(|e| e.to_string())?;
        if !init.status.success() {
            return Err(String::from_utf8_lossy(&init.stderr).trim().to_owned());
        }
        run(&["add", "-A"]).map_err(|e| e.to_string())?;
        Ok(())
    });
    if let Err(e) = result {
        println!("  note    git: {e}");
        return;
    }
    let message = format!("Start {name}, a Deckly plugin");
    match run(&["commit", "-q", "-m", &message]) {
        Ok(o) if o.status.success() => println!("  done    first commit"),
        _ => println!(
            "  note    no first commit: set git config user.name and user.email, then commit"
        ),
    }
}

fn relative(folder: &Path) -> PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|here| folder.strip_prefix(here).ok().map(Path::to_path_buf))
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| folder.to_owned())
}

/// The year now, for the licence (UTC).
fn this_year() -> i32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    year_of_day((secs / 86_400) as i64)
}

/// The year of a day counted from 1970-01-01 (the civil calendar).
fn year_of_day(days: i64) -> i32 {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    year as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_packages_and_ids() {
        assert_eq!(crate_name("My Clock!").unwrap(), "my-clock");
        assert_eq!(crate_name("  OBS  Scenes  ").unwrap(), "obs-scenes");
        assert_eq!(crate_name("3D Tools").unwrap(), "plugin-3d-tools");
        assert_eq!(crate_name("Serde").unwrap(), "serde-plugin");
        assert!(crate_name("!!!").is_err());
        assert_eq!(
            default_id("Ada Lovelace", "My Clock"),
            "com.ada-lovelace.my-clock"
        );
        assert!(check_id("com.example.a").is_ok());
        assert!(check_id("nodots").is_err());
        assert!(check_id("com/example").is_err());
        assert!(check_name("").is_err());
        assert!(check_name(&"x".repeat(65)).is_err());
    }

    #[test]
    fn without_a_terminal_missing_answers_are_named() {
        let args = CreateArgs {
            name: Some("Clock".into()),
            ..CreateArgs::default()
        };
        let e = unattended(&args, Sdk::CratesIo).unwrap_err();
        assert!(e.contains("--author") && e.contains("--yes"), "{e}");
        let yes = CreateArgs {
            yes: true,
            author: Some("Ada".into()),
            ..args
        };
        let a = unattended(&yes, Sdk::CratesIo).unwrap();
        assert_eq!(
            (a.crate_name.as_str(), a.id.as_str()),
            ("clock", "com.ada.clock")
        );
        assert_eq!(a.example, Example::Counter);
        let nameless = CreateArgs {
            yes: true,
            ..CreateArgs::default()
        };
        assert!(unattended(&nameless, Sdk::CratesIo).is_err());
    }

    #[test]
    fn years_follow_the_calendar() {
        assert_eq!(year_of_day(0), 1970);
        assert_eq!(year_of_day(20_771), 2026); // 2026-11-14
        assert_eq!(year_of_day(20_453), 2025); // 2025-12-31
        assert_eq!(year_of_day(20_454), 2026); // 2026-01-01
    }

    #[test]
    fn a_full_folder_is_refused_before_anything_is_written() {
        let dir = std::env::temp_dir().join(format!("deckly-cli-full-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(check_folder(&dir).is_ok());
        std::fs::write(dir.join("x"), "x").unwrap();
        assert!(check_folder(&dir).is_err());
        assert!(check_folder(&dir.join("x")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
