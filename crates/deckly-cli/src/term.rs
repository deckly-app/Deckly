//! What the terminal can show: whether to ask questions and whether to use
//! colour, the colours themselves, and the banner.
//!
//! Only the 16 basic colours are used, so every theme (light or dark)
//! picks its own shades; `NO_COLOR`, `CLICOLOR=0` and `TERM=dumb` turn the
//! colours off, `CLICOLOR_FORCE` and `FORCE_COLOR` turn them on. The
//! banner is plain ASCII: it reads the same in every font and terminal.

use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

/// The program's name in plain ASCII, six lines of 35 columns.
const BANNER: [&str; 6] = [
    r"     _              _     _",
    r"  __| |  ___   ___ | | __| | _   _",
    r" / _` | / _ \ / __|| |/ /| || | | |",
    r"| (_| ||  __/| (__ |   < | || |_| |",
    r" \__,_| \___| \___||_|\_\|_| \__, |",
    r"                             |___/",
];

/// The banner's width in columns.
const BANNER_WIDTH: usize = 35;

/// Whether the tool may ask questions: a real terminal on both ends, one
/// that is not `dumb`, and (on Windows) one that understands escape codes.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && !is_dumb(env("TERM").as_deref())
        && ansi_works()
}

/// Whether to colour what goes to the terminal.
pub fn color() -> bool {
    static COLOR: OnceLock<bool> = OnceLock::new();
    *COLOR.get_or_init(|| wants_color(env, std::io::stdout().is_terminal()) && ansi_works())
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn is_dumb(term: Option<&str>) -> bool {
    term == Some("dumb")
}

/// The colour rules, with the environment passed in so they can be tested.
fn wants_color(get: impl Fn(&str) -> Option<String>, is_terminal: bool) -> bool {
    let set = |name: &str| get(name).is_some_and(|v| !v.is_empty());
    let on = |name: &str| get(name).is_some_and(|v| !v.is_empty() && v != "0");
    // https://no-color.org: any non-empty value turns colour off.
    if set("NO_COLOR") {
        return false;
    }
    if on("CLICOLOR_FORCE") || on("FORCE_COLOR") {
        return true;
    }
    if get("CLICOLOR").as_deref() == Some("0") {
        return false;
    }
    is_terminal && !is_dumb(get("TERM").as_deref())
}

/// Windows 10 and later switch escape codes on for the console; an older
/// console cannot show them, and the tool then asks no questions.
#[cfg(windows)]
fn ansi_works() -> bool {
    crossterm::ansi_support::supports_ansi()
}

#[cfg(not(windows))]
fn ansi_works() -> bool {
    true
}

/// The text in a colour or style (an SGR code), when colour is on.
fn paint(code: &str, text: impl Display) -> String {
    if color() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn green(text: impl Display) -> String {
    paint("32", text)
}

pub fn cyan(text: impl Display) -> String {
    paint("36", text)
}

pub fn red(text: impl Display) -> String {
    paint("31", text)
}

pub fn bold(text: impl Display) -> String {
    paint("1", text)
}

pub fn dim(text: impl Display) -> String {
    paint("2", text)
}

/// The columns of the terminal, 80 when it will not say.
pub fn columns() -> usize {
    crossterm::terminal::size().map_or(80, |(w, _)| usize::from(w).max(1))
}

/// The banner and a welcome line. The banner is left out of a terminal too
/// narrow for it.
pub fn welcome() {
    println!();
    if columns() > BANNER_WIDTH {
        for line in BANNER {
            println!("{}", cyan(line));
        }
        println!();
    }
    println!("Welcome to the {} plugin creation wizard.", green("Deckly"));
    println!(
        "{}",
        dim("The suggestions are filled in: Enter keeps them, or edit. Esc or Ctrl+C stops.")
    );
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(vars: &[(&str, &str)], tty: bool) -> bool {
        wants_color(
            |name| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            },
            tty,
        )
    }

    #[test]
    fn colour_follows_the_environment() {
        assert!(rules(&[("TERM", "xterm-kitty")], true));
        assert!(!rules(&[("TERM", "xterm-kitty")], false));
        assert!(!rules(&[("TERM", "dumb")], true));
        assert!(!rules(&[("NO_COLOR", "1")], true));
        assert!(!rules(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")], true));
        // An empty NO_COLOR is not set.
        assert!(rules(&[("NO_COLOR", "")], true));
        assert!(!rules(&[("CLICOLOR", "0")], true));
        assert!(rules(&[("CLICOLOR_FORCE", "1")], false));
        assert!(!rules(&[("FORCE_COLOR", "0")], false));
        assert!(rules(&[("FORCE_COLOR", "1")], false));
    }

    #[test]
    fn the_banner_is_ascii_and_as_wide_as_it_says() {
        for line in BANNER {
            assert!(line.is_ascii() && !line.contains('\t'), "{line}");
            assert!(line.chars().count() <= BANNER_WIDTH, "{line}");
        }
        assert_eq!(
            BANNER.iter().map(|l| l.chars().count()).max(),
            Some(BANNER_WIDTH)
        );
    }

    #[test]
    fn only_a_dumb_terminal_is_dumb() {
        assert!(is_dumb(Some("dumb")));
        assert!(!is_dumb(Some("xterm-kitty")));
        assert!(!is_dumb(None));
    }
}
