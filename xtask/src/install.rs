//! `cargo dev install`: builds this checkout and installs it for the current
//! user where a release install puts it, replacing that install in place, so
//! unreleased work can be tried as the installed app.
//!
//! What gets installed comes from [`dist::stage_programs`], the same staging
//! the release archive uses; where it goes follows `install.sh` (a test
//! below reads the script to keep the two in step). The app is marked as a
//! local build with the commit, so it names that commit and never updates
//! itself to an older release.
//!
//! Linux and macOS only. Windows installs through the Inno Setup program,
//! which this command does not run: it says so and suggests `cargo dev dist`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::dist::{self, Os, Profile};

const HELP: &str = "cargo dev install [--debug] [--no-cli] [--prefix <dir>]\n\
Builds this checkout and installs it for the current user the way install.sh\n\
does, replacing an installed Deckly in place, then installs deckly-cli and\n\
cargo-deckly into Cargo's bin folder. Only what changed is rebuilt.\n\
--debug          Build and install debug binaries (faster to compile).\n\
--no-cli         Leave the plugin tool alone.\n\
--prefix <dir>   Linux: install prefix (default: DECKLY_PREFIX or ~/.local).\n\
                 macOS: the folder for Deckly.app (default: ~/Applications).\n\
Linux and macOS only; on Windows use cargo dev dist.";

/// Where `install.sh` installs the device rules.
const UDEV_RULES: &str = "/etc/udev/rules.d/60-deckly.rules";
/// Where a system package (such as deckly-bin) installs them.
const PACKAGE_UDEV_RULES: &str = "/usr/lib/udev/rules.d/60-deckly.rules";

#[derive(Debug, Default, PartialEq, Eq)]
struct Options {
    debug: bool,
    no_cli: bool,
    prefix: Option<PathBuf>,
    help: bool,
}

impl Options {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, String> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--debug") => options.debug = true,
                Some("--no-cli") => options.no_cli = true,
                Some("--prefix") => {
                    options.prefix = Some(crate::path_value("--prefix", &mut args)?);
                }
                Some("--help" | "-h") => options.help = true,
                _ => {
                    return Err(format!(
                        "unknown install option {}; use --help",
                        arg.to_string_lossy()
                    ));
                }
            }
        }
        Ok(options)
    }
}

pub fn run(args: impl Iterator<Item = OsString>) -> Result<bool, String> {
    let options = Options::parse(args)?;
    if options.help {
        println!("{HELP}");
        return Ok(true);
    }
    let os = this_os()?;
    let root = crate::root()?;
    let target = dist::host_target()?;
    let place = match options.prefix {
        Some(dir) => dir,
        None => default_place(os)?,
    };
    let place = std::path::absolute(&place)
        .map_err(|e| format!("cannot resolve {}: {e}", place.display()))?;

    let profile = if options.debug {
        Profile::Debug
    } else {
        Profile::Release
    };
    let build_id = local_build_id(root);
    say(&format!("build {build_id} ({profile:?})"));
    dist::build(
        root,
        &target,
        profile,
        &[(dist::LOCAL_BUILD_ENV, build_id.as_str())],
    )?;
    let stage = root
        .join("target")
        .join("dev-install")
        .join(format!("deckly-{target}"));
    dist::stage_programs(
        root,
        os,
        &profile.dir(root, &target),
        &stage,
        env!("CARGO_PKG_VERSION"),
    )?;

    for step in plan(os, &stage, &place) {
        step.apply()?;
    }
    match os {
        Os::Linux => after_linux_install(root, &place),
        Os::Mac | Os::Windows => {}
    }
    say(&format!("installed {}", installed(os, &place).display()));

    if !options.no_cli && !crate::plugin_tools::install_tool(options.debug)? {
        return Ok(false);
    }
    if deckly_running() {
        say("Deckly is running: quit it from the tray and start it again to use the new build");
    }
    Ok(true)
}

fn say(text: &str) {
    println!("deckly dev: {text}");
}

fn this_os() -> Result<Os, String> {
    if cfg!(target_os = "linux") {
        Ok(Os::Linux)
    } else if cfg!(target_os = "macos") {
        Ok(Os::Mac)
    } else {
        Err(format!(
            "cargo dev install is not supported on {} yet; use cargo dev dist \
             --setup and run the setup it writes",
            std::env::consts::OS
        ))
    }
}

/// `install.sh`'s default: the prefix on Linux, the Applications folder on
/// macOS.
fn default_place(os: Os) -> Result<PathBuf, String> {
    if os == Os::Linux
        && let Some(prefix) = std::env::var_os("DECKLY_PREFIX").filter(|p| !p.is_empty())
    {
        return Ok(PathBuf::from(prefix));
    }
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .ok_or("HOME is not set; pass --prefix")?;
    Ok(match os {
        Os::Mac => home.join("Applications"),
        Os::Linux | Os::Windows => home.join(".local"),
    })
}

/// The folder that holds the installed programs.
fn installed(os: Os, place: &Path) -> PathBuf {
    match os {
        Os::Mac => place.join("Deckly.app"),
        Os::Linux | Os::Windows => place.join("lib").join("deckly"),
    }
}

/// One file put in place.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// A program, replaced by a rename so a running copy keeps its file.
    Program {
        from: PathBuf,
        to: PathBuf,
    },
    /// A symbolic link at `link` to `to`.
    Link {
        link: PathBuf,
        to: PathBuf,
    },
    /// The menu entry, pointed at the installed program.
    Desktop {
        from: PathBuf,
        to: PathBuf,
        program: PathBuf,
    },
    File {
        from: PathBuf,
        to: PathBuf,
    },
    /// macOS: the app bundle, swapped in whole.
    Bundle {
        from: PathBuf,
        to: PathBuf,
    },
}

/// What goes where for a staged install, as `install.sh` places it.
fn plan(os: Os, stage: &Path, place: &Path) -> Vec<Step> {
    match os {
        Os::Mac => vec![Step::Bundle {
            from: stage.join("Deckly.app"),
            to: place.join("Deckly.app"),
        }],
        Os::Linux | Os::Windows => {
            let lib = installed(os, place);
            let mut steps: Vec<Step> = dist::PROGRAMS
                .iter()
                .map(|name| Step::Program {
                    from: stage.join(name),
                    to: lib.join(name),
                })
                .collect();
            steps.push(Step::Link {
                link: place.join("bin").join("deckly"),
                to: lib.join("deckly"),
            });
            for (_, staged) in dist::LINUX_SHARE {
                let from = stage.join(staged);
                let to = place.join(staged);
                steps.push(if staged.ends_with(".desktop") {
                    Step::Desktop {
                        from,
                        to,
                        program: lib.join("deckly"),
                    }
                } else {
                    Step::File { from, to }
                });
            }
            steps
        }
    }
}

impl Step {
    fn apply(&self) -> Result<(), String> {
        match self {
            Step::Program { from, to } => {
                let new = sibling(to, ".new")?;
                dist::copy(from, &new)?;
                set_executable(&new)?;
                rename(&new, to)
            }
            Step::Link { link, to } => make_link(to, link),
            Step::Desktop { from, to, program } => {
                let text = fs::read_to_string(from)
                    .map_err(|e| format!("cannot read {}: {e}", from.display()))?;
                dist::write(to, desktop_entry(&text, program)?.as_bytes())
            }
            Step::File { from, to } => dist::copy(from, to),
            Step::Bundle { from, to } => replace_dir(from, to),
        }
    }
}

/// `path` with `suffix` added to its file name.
fn sibling(path: &Path, suffix: &str) -> Result<PathBuf, String> {
    let mut name = path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?
        .to_os_string();
    name.push(suffix);
    Ok(path.with_file_name(name))
}

fn rename(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to)
        .map_err(|e| format!("cannot move {} to {}: {e}", from.display(), to.display()))
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot make {} executable: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<(), String> {
    Ok(())
}

/// `ln -sf to link`, through a rename so the link is never missing.
#[cfg(unix)]
fn make_link(to: &Path, link: &Path) -> Result<(), String> {
    dist::create_parent(link)?;
    let new = sibling(link, ".new")?;
    let _ = fs::remove_file(&new);
    std::os::unix::fs::symlink(to, &new)
        .map_err(|e| format!("cannot create {}: {e}", new.display()))?;
    rename(&new, link)
}

#[cfg(not(unix))]
fn make_link(_: &Path, link: &Path) -> Result<(), String> {
    Err(format!(
        "cannot create {}: links are for Linux installs",
        link.display()
    ))
}

/// The menu entry with `Exec=deckly` pointed at `program`, as `install.sh`
/// does: desktop sessions often lack `~/.local/bin` on `PATH`.
fn desktop_entry(text: &str, program: &Path) -> Result<String, String> {
    let program = program
        .to_str()
        .filter(|p| !p.contains(['"', '`', '$', '\\', '\n', '\r']))
        .ok_or_else(|| {
            format!(
                "{} cannot go into a menu entry; pick a plainer --prefix",
                program.display()
            )
        })?;
    let mut found = false;
    let mut out = String::with_capacity(text.len() + program.len());
    for line in text.split_inclusive('\n') {
        let (body, end) = match line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (line, ""),
        };
        if body == "Exec=deckly" {
            found = true;
            out.push_str(&format!("Exec=\"{program}\"{end}"));
        } else {
            out.push_str(line);
        }
    }
    if found {
        Ok(out)
    } else {
        Err("the menu entry has no Exec=deckly line".to_owned())
    }
}

/// macOS: swaps in a copy of `from` at `to`, keeping the old one until the
/// new one is in place.
fn replace_dir(from: &Path, to: &Path) -> Result<(), String> {
    let new = sibling(to, ".new")?;
    let old = sibling(to, ".old")?;
    for dir in [&new, &old] {
        if dir.exists() {
            fs::remove_dir_all(dir).map_err(|e| format!("cannot clear {}: {e}", dir.display()))?;
        }
    }
    copy_dir(from, &new)?;
    let had_old = to.exists();
    if had_old {
        rename(to, &old)?;
    }
    if let Err(e) = rename(&new, to) {
        if had_old {
            let _ = fs::rename(&old, to);
        }
        return Err(e);
    }
    if had_old {
        let _ = fs::remove_dir_all(&old);
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {e}", to.display()))?;
    let entries = fs::read_dir(from).map_err(|e| format!("cannot read {}: {e}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read {}: {e}", from.display()))?;
        let kind = entry
            .file_type()
            .map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            dist::copy(&entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// Linux: the menu cache, the device rules and the `PATH` note, as
/// `install.sh` does them, except that sudo is never run here.
fn after_linux_install(root: &Path, prefix: &Path) {
    // Optional, like in install.sh; the menu finds the entry without it.
    let _ = Command::new("update-desktop-database")
        .arg(prefix.join("share").join("applications"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    let rules = root.join(dist::LINUX_UDEV_RULES.0);
    let wanted = fs::read(&rules).ok();
    let advice = udev_advice(
        &rules,
        wanted.as_deref(),
        std::env::var_os("DECKLY_NO_UDEV").is_some_and(|v| v == "1"),
        fs::read(UDEV_RULES).ok().as_deref(),
        Path::new(PACKAGE_UDEV_RULES).is_file(),
    );
    if let Some(advice) = advice {
        say(&advice);
    }

    let bin = prefix.join("bin");
    let on_path = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir == bin));
    if !on_path {
        say(&format!(
            "note: {} is not on PATH; start Deckly from the app menu",
            bin.display()
        ));
    }
}

/// What to tell about the device rules, or nothing when they are in place.
/// `install.sh` installs them with sudo; this only prints the command.
fn udev_advice(
    rules: &Path,
    wanted: Option<&[u8]>,
    skip: bool,
    installed: Option<&[u8]>,
    from_package: bool,
) -> Option<String> {
    if skip {
        return Some("skipped the device rules (DECKLY_NO_UDEV=1)".to_owned());
    }
    let Some(wanted) = wanted else {
        return Some(format!("cannot read {}", rules.display()));
    };
    if installed == Some(wanted) || from_package {
        return None;
    }
    let what = if installed.is_some() {
        "differ from this checkout"
    } else {
        "are not installed"
    };
    Some(format!(
        "the device rules in {UDEV_RULES} {what}; to install them run:\n    \
         sudo install -m 644 {} {UDEV_RULES} && sudo udevadm control --reload && sudo udevadm trigger",
        rules.display()
    ))
}

/// The commit of this checkout for the app's version text, with `-dirty`
/// when tracked files have uncommitted changes.
fn local_build_id(root: &Path) -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let commit = git(&["rev-parse", "--short=9", "HEAD"])
        .filter(|c| !c.is_empty() && c.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or_else(|| "unknown".to_owned());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|status| !status.is_empty());
    if dirty {
        format!("{commit}-dirty")
    } else {
        commit
    }
}

/// Whether a Deckly holds its instance lock (see `os/instance.rs` in the
/// app). Only reads: a missing lock file is not created.
fn deckly_running() -> bool {
    let Some(dir) = config_dir() else {
        return false;
    };
    match fs::File::open(dir.join("instance.lock")) {
        Ok(file) => matches!(file.try_lock(), Err(fs::TryLockError::WouldBlock)),
        Err(_) => false,
    }
}

/// The app's settings folder, as `store::config_dir` finds it.
fn config_dir() -> Option<PathBuf> {
    let env = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = env("DECKLY_CONFIG_DIR") {
        return Some(dir);
    }
    if cfg!(target_os = "macos") {
        Some(env("HOME")?.join("Library/Application Support/Deckly"))
    } else {
        let base = env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?;
        Some(base.join("deckly"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTALL_SH: &str = include_str!("../../install.sh");

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn options_are_parsed_and_unknown_ones_refused() {
        let o = Options::parse(args(&["--debug", "--no-cli", "--prefix", "/tmp/x"])).unwrap();
        assert_eq!(
            o,
            Options {
                debug: true,
                no_cli: true,
                prefix: Some(PathBuf::from("/tmp/x")),
                help: false,
            }
        );
        assert_eq!(Options::parse(args(&[])).unwrap(), Options::default());
        assert!(Options::parse(args(&["--help"])).unwrap().help);
        assert!(Options::parse(args(&["--prefix"])).is_err());
        assert!(Options::parse(args(&["--prefix", "--debug"])).is_err());
        assert!(Options::parse(args(&["--no-check"])).is_err());
    }

    #[test]
    fn linux_plan_puts_files_where_install_sh_does() {
        let stage = Path::new("/s");
        let p = Path::new("/home/me/.local");
        let lib = p.join("lib/deckly");
        assert_eq!(
            plan(Os::Linux, stage, p),
            vec![
                Step::Program {
                    from: stage.join("deckly"),
                    to: lib.join("deckly"),
                },
                Step::Program {
                    from: stage.join("deckly-web"),
                    to: lib.join("deckly-web"),
                },
                Step::Link {
                    link: p.join("bin/deckly"),
                    to: lib.join("deckly"),
                },
                Step::Desktop {
                    from: stage.join("share/applications/deckly.desktop"),
                    to: p.join("share/applications/deckly.desktop"),
                    program: lib.join("deckly"),
                },
                Step::File {
                    from: stage.join("share/icons/hicolor/scalable/apps/deckly.svg"),
                    to: p.join("share/icons/hicolor/scalable/apps/deckly.svg"),
                },
            ]
        );
        assert_eq!(
            plan(Os::Mac, stage, Path::new("/Users/me/Applications")),
            vec![Step::Bundle {
                from: stage.join("Deckly.app"),
                to: PathBuf::from("/Users/me/Applications/Deckly.app"),
            }]
        );
    }

    /// Every file the plan writes is one `install.sh` writes and its
    /// uninstall removes, and the script installs the programs this does.
    #[test]
    fn plan_and_install_sh_agree() {
        let uninstall = INSTALL_SH
            .split("if [ \"$uninstall\" = 1 ]; then")
            .nth(1)
            .and_then(|rest| rest.split("exit 0").next())
            .expect("install.sh has an uninstall block");
        assert!(uninstall.contains("rm -rf \"$prefix/lib/deckly\""));
        assert!(uninstall.contains("rm -rf \"$apps/Deckly.app\""));
        assert!(INSTALL_SH.contains("lib=\"$prefix/lib/deckly\""));
        assert!(INSTALL_SH.contains("apps=\"$HOME/Applications\""));
        assert!(INSTALL_SH.contains("prefix=${DECKLY_PREFIX:-$HOME/.local}"));
        assert!(INSTALL_SH.contains(&format!("udev_rules={UDEV_RULES}")));
        assert!(INSTALL_SH.contains(PACKAGE_UDEV_RULES));
        assert!(INSTALL_SH.contains(&format!("for exe in {}; do", dist::PROGRAMS.join(" "))));
        assert!(INSTALL_SH.contains("ln -sf \"$lib/deckly\" \"$prefix/bin/deckly\""));
        assert!(INSTALL_SH.contains(r#"sed "s|^Exec=deckly\$|Exec=\"$lib/deckly\"|""#));

        let prefix = Path::new("/p");
        for step in plan(Os::Linux, Path::new("/s"), prefix) {
            let to = match step {
                Step::Program { to, .. }
                | Step::Link { link: to, .. }
                | Step::Desktop { to, .. }
                | Step::File { to, .. }
                | Step::Bundle { to, .. } => to,
            };
            let rel = to
                .strip_prefix(prefix)
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            if rel.starts_with("lib/deckly/") {
                continue;
            }
            let quoted = format!("\"$prefix/{rel}\"");
            assert!(uninstall.contains(&quoted), "uninstall misses {rel}");
            assert!(
                INSTALL_SH.contains(&format!("$src/{rel}"))
                    || INSTALL_SH.contains(&format!("> {quoted}"))
                    || rel == "bin/deckly",
                "install.sh does not install {rel}"
            );
        }
        for (_, staged) in dist::LINUX_SHARE
            .into_iter()
            .chain([dist::LINUX_UDEV_RULES])
        {
            assert!(
                INSTALL_SH.contains(&format!("$src/{staged}")),
                "install.sh does not read {staged} from the archive"
            );
        }
    }

    #[test]
    fn desktop_entry_points_at_the_installed_program() {
        let text = include_str!("../../assets/linux/deckly.desktop");
        let lib = Path::new("/home/me/.local/lib/deckly/deckly");
        let entry = desktop_entry(text, lib).unwrap();
        assert!(entry.contains("\nExec=\"/home/me/.local/lib/deckly/deckly\"\n"));
        assert!(!entry.contains("Exec=deckly"));
        assert_eq!(entry.lines().count(), text.lines().count());
        assert!(entry.ends_with('\n') == text.ends_with('\n'));
        // Only the exact line, like the sed in install.sh.
        assert_eq!(
            desktop_entry("Exec=deckly\nTryExec=deckly", lib).unwrap(),
            "Exec=\"/home/me/.local/lib/deckly/deckly\"\nTryExec=deckly"
        );
        assert!(desktop_entry("Exec=other\n", lib).is_err());
        assert!(desktop_entry(text, Path::new("/tmp/a\"b/deckly")).is_err());
        assert!(desktop_entry(text, Path::new("/tmp/$HOME/deckly")).is_err());
    }

    #[test]
    fn udev_rules_are_only_suggested_never_installed() {
        let rules = Path::new("/c/assets/linux/60-deckly.rules");
        let want = b"rules".as_slice();
        assert_eq!(
            udev_advice(rules, Some(want), false, Some(want), false),
            None
        );
        assert_eq!(udev_advice(rules, Some(want), false, None, true), None);
        let missing = udev_advice(rules, Some(want), false, None, false).unwrap();
        assert!(missing.contains("are not installed"));
        assert!(missing.contains(
            "sudo install -m 644 /c/assets/linux/60-deckly.rules /etc/udev/rules.d/60-deckly.rules"
        ));
        let old = udev_advice(rules, Some(want), false, Some(b"old"), false).unwrap();
        assert!(old.contains("differ"));
        assert!(
            udev_advice(rules, Some(want), true, None, false)
                .unwrap()
                .contains("DECKLY_NO_UDEV")
        );
    }

    /// The plan installs exactly what `cargo dev dist` stages for a release,
    /// apart from the device rules, which go in with sudo.
    #[test]
    fn plan_installs_what_dist_stages() {
        let root = std::env::temp_dir().join(format!("deckly-dev-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        for name in dist::PROGRAMS {
            dist::write(&bin.join(name), b"program").unwrap();
        }
        let stage = root.join("stage");
        dist::stage_programs(crate::root().unwrap(), Os::Linux, &bin, &stage, "0.0.0").unwrap();
        let mut staged = Vec::new();
        let mut dirs = vec![stage.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else {
                    staged.push(path);
                }
            }
        }
        let mut sources: Vec<PathBuf> = plan(Os::Linux, &stage, Path::new("/p"))
            .into_iter()
            .filter_map(|step| match step {
                Step::Program { from, .. }
                | Step::Desktop { from, .. }
                | Step::File { from, .. }
                | Step::Bundle { from, .. } => Some(from),
                Step::Link { .. } => None,
            })
            .chain([stage.join(dist::LINUX_UDEV_RULES.1)])
            .collect();
        staged.sort();
        sources.sort();
        assert_eq!(staged, sources);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn applying_the_plan_replaces_an_install_in_place() {
        let root = std::env::temp_dir().join(format!("deckly-dev-install-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage = root.join("stage");
        let prefix = root.join("prefix");
        let workspace = crate::root().unwrap();
        for name in dist::PROGRAMS {
            dist::write(&stage.join(name), format!("new {name}").as_bytes()).unwrap();
        }
        for (from, to) in dist::LINUX_SHARE {
            dist::copy(&workspace.join(from), &stage.join(to)).unwrap();
        }
        // An earlier install, with a program that is "running": its open
        // file must keep the old contents.
        let lib = prefix.join("lib/deckly");
        dist::write(&lib.join("deckly"), b"old").unwrap();
        let running = fs::File::open(lib.join("deckly")).unwrap();
        dist::write(&prefix.join("bin/deckly"), b"stale file").unwrap();

        for _ in 0..2 {
            for step in plan(Os::Linux, &stage, &prefix) {
                step.apply().unwrap();
            }
        }
        use std::io::Read;
        use std::os::unix::fs::PermissionsExt;
        let mut old = String::new();
        (&running).read_to_string(&mut old).unwrap();
        assert_eq!(old, "old");
        for name in dist::PROGRAMS {
            let path = lib.join(name);
            assert_eq!(fs::read_to_string(&path).unwrap(), format!("new {name}"));
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
            assert!(!lib.join(format!("{name}.new")).exists());
        }
        assert_eq!(
            fs::read_link(prefix.join("bin/deckly")).unwrap(),
            lib.join("deckly")
        );
        let entry = fs::read_to_string(prefix.join("share/applications/deckly.desktop")).unwrap();
        assert!(entry.contains(&format!("Exec=\"{}\"", lib.join("deckly").display())));
        assert!(
            prefix
                .join("share/icons/hicolor/scalable/apps/deckly.svg")
                .is_file()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_bundle_is_swapped_in_whole() {
        let root = std::env::temp_dir().join(format!("deckly-dev-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let from = root.join("stage/Deckly.app");
        let to = root.join("Applications/Deckly.app");
        dist::write(&from.join("Contents/MacOS/deckly"), b"new").unwrap();
        dist::write(&from.join("Contents/Info.plist"), b"plist").unwrap();
        dist::write(&to.join("Contents/MacOS/stale"), b"old").unwrap();
        replace_dir(&from, &to).unwrap();
        assert_eq!(
            fs::read_to_string(to.join("Contents/MacOS/deckly")).unwrap(),
            "new"
        );
        assert!(to.join("Contents/Info.plist").is_file());
        assert!(!to.join("Contents/MacOS/stale").exists());
        assert!(!root.join("Applications/Deckly.app.old").exists());
        assert!(!root.join("Applications/Deckly.app.new").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
