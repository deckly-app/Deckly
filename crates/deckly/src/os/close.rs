//! The Close action: asks a running application to close, as its own close
//! button would. Every window of the application is asked; it may still ask
//! about unsaved work, or refuse.
//!
//! An application with no window (a script, a program in the tray) gets the
//! system's request to end instead: a termination signal on Linux and macOS,
//! where a program may still clean up, and an immediate end on Windows, which
//! has no polite request for a program without a window.
//!
//! An application is named the way profiles that follow the active window
//! name it ([`deckly_core::AppRule`]): a program, bundle, package or window
//! class, of which only the last part of a path counts.

use deckly_core::AppRule;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// More windows or processes than this are not looked at; no desktop has as
/// many, and a reply that claims so is not trusted.
#[cfg(any(target_os = "linux", windows))]
const MAX_ITEMS: usize = 65_536;

/// What the action found to close.
#[derive(Debug, PartialEq, Eq)]
pub enum Closed {
    /// So many windows were asked to close.
    Windows(usize),
    /// So many processes without a window were asked to end.
    Processes(usize),
}

/// The application to close, from the action's settings: what the picker or
/// a chosen file gave. `None` when nothing names a running program.
pub fn target(app: &str) -> Option<AppRule> {
    let app = app.trim();
    // A chosen desktop file goes by its name, as its windows do.
    let app = app.strip_suffix(".desktop").unwrap_or(app);
    if app.contains("://") {
        return None;
    }
    let rule = AppRule::new("", app);
    (!rule.id.is_empty()).then_some(rule)
}

/// Asks the application's windows to close, or its processes to end when it
/// has no window. An application that is not running is an error, so the key
/// shows that nothing happened.
pub fn close(app: &str) -> Result<Closed, String> {
    let rule = target(app).ok_or("no application chosen")?;
    let closed = platform_close(&rule)?;
    match closed {
        Closed::Windows(0) | Closed::Processes(0) => Err(format!("{} is not running", rule.id)),
        closed => Ok(closed),
    }
}

/// What this desktop cannot do, for the inspector to say.
pub fn limitation() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    return linux::limitation();
    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(target_os = "linux")]
fn platform_close(rule: &AppRule) -> Result<Closed, String> {
    linux::close(rule)
}

#[cfg(target_os = "macos")]
fn platform_close(rule: &AppRule) -> Result<Closed, String> {
    macos::close(rule)
}

#[cfg(windows)]
fn platform_close(rule: &AppRule) -> Result<Closed, String> {
    windows::close(rule)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn platform_close(_: &AppRule) -> Result<Closed, String> {
    Err("Closing applications is not supported on this system.".into())
}

/// The names of a process from its command line: the program it was started
/// as and its first argument, which for a script is the script. `args` is the
/// raw, null-separated command line.
#[cfg(any(target_os = "linux", test))]
fn command_names(args: &[u8]) -> Vec<String> {
    args.split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .take(2)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_the_last_part_of_what_was_chosen() {
        assert_eq!(target("spotify").unwrap().id, "spotify");
        assert_eq!(target("  /usr/bin/kitty ").unwrap().id, "kitty");
        assert_eq!(
            target(r"C:\Program Files\App\app.exe").unwrap().id,
            "app.exe"
        );
        assert_eq!(
            target("/usr/share/applications/org.gnome.Nautilus.desktop")
                .unwrap()
                .id,
            "org.gnome.Nautilus"
        );
        assert_eq!(
            target("/Applications/Safari.app/").unwrap().id,
            "Safari.app"
        );
        assert_eq!(
            target("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")
                .unwrap()
                .id,
            "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
        );
    }

    #[test]
    fn nothing_or_a_link_is_no_target() {
        assert!(target("").is_none());
        assert!(target("   ").is_none());
        assert!(target("/").is_none());
        assert!(target(".desktop").is_none());
        assert!(target("steam://rungameid/240").is_none());
        assert_eq!(close("").unwrap_err(), "no application chosen");
    }

    #[test]
    fn names_match_without_case_or_folders() {
        let rule = target("Firefox").unwrap();
        assert!(rule.matches(&["/usr/lib/firefox/firefox".into()]));
        assert!(!rule.matches(&["/usr/lib/firefox/firefox-bin".into()]));
    }

    #[test]
    fn a_command_line_names_its_program_and_script() {
        assert_eq!(
            command_names(b"/usr/bin/python3\0/home/me/run.py\0--fast\0"),
            ["/usr/bin/python3", "/home/me/run.py"]
        );
        assert_eq!(command_names(b"top\0"), ["top"]);
        assert!(command_names(b"").is_empty());
        assert!(command_names(b"\0\0").is_empty());
        assert_eq!(command_names(b"odd\xff\0"), ["odd\u{fffd}"]);
    }

    /// A program that is surely not running.
    #[test]
    fn an_application_that_is_not_running_is_an_error() {
        let name = "deckly-close-test-no-such-program-4f1c";
        let error = close(name).unwrap_err();
        assert!(
            error == format!("{name} is not running") || error.contains("not supported"),
            "{error}"
        );
    }
}
