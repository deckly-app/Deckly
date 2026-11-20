//! What the terminal can show: whether to ask questions and whether to use
//! colour, the colours themselves, and the banner.
//!
//! Text uses only the 16 basic colours, so every theme (light or dark)
//! picks its own shades; `NO_COLOR`, `CLICOLOR=0` and `TERM=dumb` turn the
//! colours off, `CLICOLOR_FORCE` and `FORCE_COLOR` turn them on. The
//! banner is plain ASCII: it reads the same in every font and terminal.
//! A terminal that says it shows 24-bit colour gets the banner in the
//! logo's blue-to-violet gradient; any other gets it in plain cyan.

use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

/// The program's name in plain ASCII, six lines of 35 columns.
const BANNER: [&str; 6] = [
    r" ____               _     _",
    r"|  _ \   ___   ___ | | __| | _   _",
    r"| | | | / _ \ / __|| |/ /| || | | |",
    r"| |_| ||  __/| (__ |   < | || |_| |",
    r"|____/  \___| \___||_|\_\|_| \__, |",
    r"                             |___/",
];

/// The banner's width in columns.
const BANNER_WIDTH: usize = 35;

/// The logo's colours from left to right (`assets/logo/deckly.svg`), the
/// stops of the banner's gradient.
const GRADIENT: [[u8; 3]; 5] = [
    [0x3d, 0x8b, 0xfd],
    [0x51, 0x7f, 0xfb],
    [0x64, 0x74, 0xfa],
    [0x78, 0x68, 0xf8],
    [0x8b, 0x5c, 0xf6],
];

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

/// Whether the terminal shows 24-bit colour. Terminals that do say so in
/// `COLORTERM`; Windows Terminal does not, but sets `WT_SESSION`.
fn truecolor(get: impl Fn(&str) -> Option<String>) -> bool {
    matches!(get("COLORTERM").as_deref(), Some("truecolor" | "24bit"))
        || get("WT_SESSION").is_some_and(|v| !v.is_empty())
}

/// The gradient's colour at `x` of `width` columns, from the first stop at
/// the left edge to the last at the right.
fn gradient_at(x: usize, width: usize) -> [u8; 3] {
    let last = GRADIENT.len() - 1;
    let pos = if width > 1 {
        x.min(width - 1) as f32 / (width - 1) as f32 * last as f32
    } else {
        0.0
    };
    let i = (pos as usize).min(last - 1);
    let t = pos - i as f32;
    let (a, b) = (GRADIENT[i], GRADIENT[i + 1]);
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

/// One banner line in the gradient, each column in its own colour.
fn gradient_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len() * 20);
    for (x, c) in line.chars().enumerate() {
        if c == ' ' {
            out.push(c);
        } else {
            let [r, g, b] = gradient_at(x, BANNER_WIDTH);
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{c}"));
        }
    }
    out.push_str("\x1b[0m");
    out
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
        let gradient = color() && truecolor(env);
        for line in BANNER {
            if gradient {
                println!("{}", gradient_line(line));
            } else {
                println!("{}", cyan(line));
            }
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
    fn the_banner_starts_with_a_capital_d() {
        // The capital D's left stem starts each letter row; the lowercase d
        // it replaced started most of them with a space.
        for line in &BANNER[1..=4] {
            assert!(line.starts_with('|'), "{line}");
        }
    }

    #[test]
    fn only_a_terminal_that_says_so_gets_truecolor() {
        let with = |vars: &'static [(&str, &str)]| {
            truecolor(|name| {
                vars.iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            })
        };
        assert!(with(&[("COLORTERM", "truecolor")]));
        assert!(with(&[("COLORTERM", "24bit")]));
        assert!(with(&[("WT_SESSION", "1b2c")]));
        assert!(!with(&[]));
        assert!(!with(&[("COLORTERM", "")]));
        assert!(!with(&[("COLORTERM", "yes")]));
        assert!(!with(&[("TERM", "xterm-256color")]));
    }

    #[test]
    fn the_gradient_runs_from_the_first_stop_to_the_last() {
        assert_eq!(gradient_at(0, BANNER_WIDTH), GRADIENT[0]);
        assert_eq!(gradient_at(BANNER_WIDTH - 1, BANNER_WIDTH), GRADIENT[4]);
        // Past the edge and a one-column width stay on the stops.
        assert_eq!(gradient_at(BANNER_WIDTH + 10, BANNER_WIDTH), GRADIENT[4]);
        assert_eq!(gradient_at(5, 1), GRADIENT[0]);
        assert_eq!(gradient_at(0, 0), GRADIENT[0]);
        // Blue fades into violet: red rises and green falls column by column.
        for x in 1..BANNER_WIDTH {
            let (a, b) = (
                gradient_at(x - 1, BANNER_WIDTH),
                gradient_at(x, BANNER_WIDTH),
            );
            assert!(b[0] >= a[0] && b[1] <= a[1], "{x}: {a:?} {b:?}");
        }
    }

    #[test]
    fn a_gradient_line_keeps_the_text_and_resets() {
        for line in BANNER {
            let painted = gradient_line(line);
            assert!(painted.ends_with("\x1b[0m"));
            let mut plain = String::new();
            let mut chars = painted.chars();
            while let Some(c) = chars.next() {
                if c == '\x1b' {
                    for c in chars.by_ref() {
                        if c == 'm' {
                            break;
                        }
                    }
                } else {
                    plain.push(c);
                }
            }
            assert_eq!(plain, line);
        }
    }

    #[test]
    fn only_a_dumb_terminal_is_dumb() {
        assert!(is_dumb(Some("dumb")));
        assert!(!is_dumb(Some("xterm-kitty")));
        assert!(!is_dumb(None));
    }
}
