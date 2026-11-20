//! Hyprland: the compositor starts the program itself, with rules for the
//! windows that program opens (`exec` with rules). Hyprland ties a window to
//! the rules through the process that opened it, so a program that only
//! wakes up a running copy, or that D-Bus starts for `gio launch`, opens
//! without them.
//!
//! Hyprland is asked over its control socket. Since 0.55 it takes Lua
//! (`eval hl.dispatch(hl.dsp.exec_cmd(...))`); older versions take
//! `dispatch exec [rules] command`. Deckly tries Lua first and falls back
//! when Hyprland does not know it.

use super::super::unix::{home, home_path, is_program, is_url};
use super::{Layout, Monitor, Placement, Support, WindowState};
use serde::Deserialize;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Hyprland answers in milliseconds; a hung compositor must not hang a key.
const TIMEOUT: Duration = Duration::from_secs(2);
/// A monitor list is a few kilobytes per monitor.
const MAX_REPLY: u64 = 1024 * 1024;
const MAX_MONITORS: usize = 32;
/// Longest command handed to Hyprland.
const MAX_COMMAND: usize = 16 * 1024;

fn socket() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    // It names a folder; anything else is not Hyprland's.
    if signature.is_empty()
        || signature.len() > 256
        || !signature
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d).join("hypr"));
    // Before 0.40 the sockets were in /tmp.
    [runtime, Some(PathBuf::from("/tmp/hypr"))]
        .into_iter()
        .flatten()
        .map(|dir| dir.join(&signature).join(".socket.sock"))
        .find(|path| path.exists())
}

pub fn running() -> bool {
    socket().is_some()
}

pub fn support() -> Support {
    Support {
        monitor: true,
        states: vec![WindowState::Maximized, WindowState::Fullscreen],
        layout: true,
        note: Some(
            "Hyprland has no minimized windows. The window settings apply to a new \
             window of the started program: a program that is already running keeps \
             its window where it is."
                .into(),
        ),
    }
}

fn request(text: &str) -> Result<String, String> {
    let path = socket().ok_or("Hyprland is not running")?;
    let talk = || -> std::io::Result<String> {
        let mut stream = UnixStream::connect(&path)?;
        stream.set_read_timeout(Some(TIMEOUT))?;
        stream.set_write_timeout(Some(TIMEOUT))?;
        stream.write_all(text.as_bytes())?;
        let mut reply = Vec::new();
        stream.take(MAX_REPLY).read_to_end(&mut reply)?;
        Ok(String::from_utf8_lossy(&reply).into_owned())
    };
    talk().map_err(|e| format!("cannot reach Hyprland: {e}"))
}

#[derive(Deserialize)]
struct HyprMonitor {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    disabled: bool,
}

pub fn monitors() -> Result<Vec<Monitor>, String> {
    parse_monitors(&request("j/monitors")?)
}

/// A monitor is known by its description (make, model, serial), which stays
/// when the cable moves to another port; two monitors that describe
/// themselves the same are known by their port.
fn parse_monitors(json: &str) -> Result<Vec<Monitor>, String> {
    let list: Vec<HyprMonitor> =
        serde_json::from_str(json).map_err(|e| format!("unexpected monitor list: {e}"))?;
    let list: Vec<HyprMonitor> = list
        .into_iter()
        .filter(|m| !m.disabled && valid_name(&m.name))
        .take(MAX_MONITORS)
        .collect();
    let unique = |m: &HyprMonitor| {
        !m.description.trim().is_empty()
            && list
                .iter()
                .filter(|o| o.description == m.description)
                .count()
                == 1
    };
    Ok(list
        .iter()
        .map(|m| Monitor {
            id: if unique(m) {
                format!("desc:{}", m.description)
            } else {
                m.name.clone()
            },
            label: if m.model.trim().is_empty() {
                m.name.clone()
            } else {
                format!("{} ({})", m.model.trim(), m.name)
            },
            port: m.name.clone(),
        })
        .collect())
}

/// Output names as Hyprland makes them (`DP-1`, `HDMI-A-1`, `HEADLESS-2`).
/// Anything else could break out of a rule, so it is never sent.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Starts `target` through Hyprland with the placement's rules. Returns
/// notes on what it could not apply.
pub fn open(target: &str, placement: &Placement) -> Result<Vec<String>, String> {
    let command = shell_command(target)?;
    let mut notes = Vec::new();
    match start(&command, placement, &mut notes) {
        Ok(()) => Ok(notes),
        // The key still opens the app, only without the window settings.
        Err(e) => {
            super::super::open(target)?;
            Ok(vec![format!("the window settings were not applied: {e}")])
        }
    }
}

fn start(command: &str, placement: &Placement, notes: &mut Vec<String>) -> Result<(), String> {
    let mut monitor = None;
    if !placement.monitor.is_empty() {
        let connected = monitors()?;
        match super::find(&connected, &placement.monitor) {
            Some(found) => monitor = Some(found.port.clone()),
            None => notes.push(super::missing_monitor(placement)),
        }
    }
    let rules = Rules {
        monitor,
        state: placement.state,
        layout: placement.layout,
    };
    let reply = request(&format!("eval {}", rules.lua(command)))?;
    if reply.trim() == "ok" {
        return Ok(());
    }
    // Hyprland before Lua. A Lua error comes before anything starts, so
    // this does not start the program twice.
    let classic = request(&rules.classic(command))?;
    if classic.trim() == "ok" {
        return Ok(());
    }
    let reply = if reply.trim() == "unknown request" {
        classic
    } else {
        reply
    };
    Err(format!(
        "Hyprland refused it: {}",
        reply.trim().chars().take(300).collect::<String>()
    ))
}

struct Rules {
    monitor: Option<String>,
    state: WindowState,
    layout: Layout,
}

impl Rules {
    fn lua(&self, command: &str) -> String {
        let mut rules = Vec::new();
        if let Some(m) = &self.monitor {
            rules.push(format!("monitor={}", lua_string(m)));
        }
        match self.layout {
            Layout::Floating => rules.push("float=true".into()),
            Layout::Tiled => rules.push("tile=true".into()),
            Layout::Default => {}
        }
        match self.state {
            WindowState::Maximized => rules.push("maximize=true".into()),
            WindowState::Fullscreen => rules.push("fullscreen=true".into()),
            _ => {}
        }
        format!(
            "hl.dispatch(hl.dsp.exec_cmd({}, {{{}}}))",
            lua_string(command),
            rules.join(", ")
        )
    }

    fn classic(&self, command: &str) -> String {
        let mut rules = Vec::new();
        if let Some(m) = &self.monitor {
            rules.push(format!("monitor {m}"));
        }
        match self.layout {
            Layout::Floating => rules.push("float".into()),
            Layout::Tiled => rules.push("tile".into()),
            Layout::Default => {}
        }
        match self.state {
            WindowState::Maximized => rules.push("maximize".into()),
            WindowState::Fullscreen => rules.push("fullscreen".into()),
            _ => {}
        }
        if rules.is_empty() {
            format!("dispatch exec {command}")
        } else {
            format!("dispatch exec [{}] {command}", rules.join("; "))
        }
    }
}

/// A Lua string literal holding `text` exactly.
fn lua_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_control() => out.push_str(&format!("\\{:03}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A word for `sh`, quoted so nothing in it is special.
fn sh_word(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

fn sh_path(path: &Path) -> String {
    sh_word(&path.to_string_lossy())
}

/// What the Open action runs, as one `sh` command line for Hyprland's
/// `exec`. It makes the same choices as [`super::super::open`].
fn shell_command(target: &str) -> Result<String, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("nothing to open".into());
    }
    if target.contains(['\n', '\r', '\0']) {
        return Err("the path holds a line break".into());
    }
    let command = if is_url(target) {
        format!("exec xdg-open {}", sh_word(target))
    } else {
        let whole = home_path(target);
        if whole.exists() {
            if whole.extension().is_some_and(|e| e == "desktop") {
                format!("exec gio launch {}", sh_path(&whole))
            } else if is_program(&whole) {
                match whole.parent().filter(|d| !d.as_os_str().is_empty()) {
                    Some(dir) => format!("cd {} && exec {}", sh_path(dir), sh_path(&whole)),
                    None => format!("exec {}", sh_path(&whole)),
                }
            } else {
                format!("exec xdg-open {}", sh_path(&whole))
            }
        } else {
            let words = super::super::command::split_words(target)
                .ok_or_else(|| format!("unclosed quote in \"{target}\""))?;
            let Some((program, args)) = words.split_first() else {
                return Err("nothing to open".into());
            };
            let program = home_path(program);
            if !found(&program) {
                return Err(format!("cannot run \"{target}\": program not found"));
            }
            let mut line = String::new();
            if let Some(home) = home() {
                line.push_str(&format!("cd {} && ", sh_path(&home)));
            }
            line.push_str("exec ");
            line.push_str(&sh_path(&program));
            for arg in args {
                line.push(' ');
                line.push_str(&sh_word(arg));
            }
            line
        }
    };
    if command.len() > MAX_COMMAND {
        return Err("the command is too long".into());
    }
    Ok(command)
}

/// Whether `sh` will find the program: Hyprland reports nothing back once it
/// has started the shell.
fn found(program: &Path) -> bool {
    if program.components().count() > 1 {
        return is_program(program);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| is_program(&dir.join(program)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitors_are_known_by_description_unless_two_share_it() {
        let json = r#"[
            {"name":"DP-1","description":"Dell Inc. DELL AW2523HF HMSCB34","model":"DELL AW2523HF"},
            {"name":"DP-2","description":"Generic Panel","model":"Panel"},
            {"name":"DP-3","description":"Generic Panel","model":""},
            {"name":"HDMI-A-1","description":"","model":"TV","disabled":true},
            {"name":"bad] name","description":"x"}
        ]"#;
        let list = parse_monitors(json).unwrap();
        let monitor = |id: &str, label: &str, port: &str| Monitor {
            id: id.into(),
            label: label.into(),
            port: port.into(),
        };
        assert_eq!(
            list,
            [
                monitor(
                    "desc:Dell Inc. DELL AW2523HF HMSCB34",
                    "DELL AW2523HF (DP-1)",
                    "DP-1"
                ),
                monitor("DP-2", "Panel (DP-2)", "DP-2"),
                monitor("DP-3", "DP-3", "DP-3"),
            ]
        );
        assert!(parse_monitors("not json").is_err());
        assert!(parse_monitors("{}").is_err());
    }

    #[test]
    fn many_monitors_are_bounded() {
        let one = r#"{"name":"DP-1","description":"Same"}"#;
        let json = format!("[{}]", vec![one; 500].join(","));
        assert_eq!(parse_monitors(&json).unwrap().len(), MAX_MONITORS);
    }

    #[test]
    fn rules_in_lua_and_in_the_older_syntax() {
        let rules = Rules {
            monitor: Some("DP-2".into()),
            state: WindowState::Maximized,
            layout: Layout::Floating,
        };
        assert_eq!(
            rules.lua("exec 'spotify'"),
            r#"hl.dispatch(hl.dsp.exec_cmd("exec 'spotify'", {monitor="DP-2", float=true, maximize=true}))"#
        );
        assert_eq!(
            rules.classic("exec 'spotify'"),
            "dispatch exec [monitor DP-2; float; maximize] exec 'spotify'"
        );
        let none = Rules {
            monitor: None,
            state: WindowState::Minimized,
            layout: Layout::Tiled,
        };
        assert_eq!(
            none.lua("x"),
            r#"hl.dispatch(hl.dsp.exec_cmd("x", {tile=true}))"#
        );
        assert_eq!(none.classic("x"), "dispatch exec [tile] x");
    }

    #[test]
    fn lua_strings_cannot_be_escaped() {
        assert_eq!(lua_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(lua_string("x\n\u{7}"), r#""x\010\007""#);
        assert_eq!(lua_string("Музыка"), "\"Музыка\"");
        assert_eq!(lua_string("]] ..os.exit()"), "\"]] ..os.exit()\"");
    }

    #[test]
    fn shell_words_are_quoted() {
        assert_eq!(sh_word("RGB Lights"), "'RGB Lights'");
        assert_eq!(sh_word("it's; rm -rf ~"), r"'it'\''s; rm -rf ~'");
        assert_eq!(sh_word("$(id)`x`"), "'$(id)`x`'");
    }

    #[test]
    fn commands_match_what_open_runs() {
        assert_eq!(
            shell_command("https://example.com/a b").unwrap(),
            "exec xdg-open 'https://example.com/a b'"
        );
        let line = shell_command("sh -c \"echo 'hi'\"").unwrap();
        assert!(
            line.ends_with(r"exec 'sh' '-c' 'echo '\''hi'\'''"),
            "{line}"
        );
        assert!(shell_command("").is_err());
        assert!(shell_command("no-such-deckly-program --x").is_err());
        assert!(shell_command("sh \"open").is_err());
        assert!(shell_command("sh\n-c x").is_err());

        let dir = std::env::temp_dir().join(format!("deckly-hypr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("My App.desktop");
        std::fs::write(&app, "[Desktop Entry]\n").unwrap();
        assert_eq!(
            shell_command(app.to_str().unwrap()).unwrap(),
            format!("exec gio launch '{}'", app.display())
        );
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, "x").unwrap();
        assert_eq!(
            shell_command(notes.to_str().unwrap()).unwrap(),
            format!("exec xdg-open '{}'", notes.display())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_port_names_reach_a_rule() {
        assert!(valid_name("HDMI-A-1"));
        assert!(valid_name("HEADLESS-2"));
        assert!(!valid_name("DP-1; exec x"));
        assert!(!valid_name("DP-1]"));
        assert!(!valid_name(""));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Opens a kitty window floating on a monitor that does not have the
    /// focus, and checks where Hyprland put it.
    #[test]
    #[ignore = "requires Hyprland with two monitors and kitty; opens a window"]
    fn opens_floating_on_the_chosen_monitor() {
        let all: Vec<serde_json::Value> =
            serde_json::from_str(&request("j/monitors").unwrap()).unwrap();
        let target = all
            .iter()
            .find(|m| m["focused"] == false)
            .and_then(|m| m["name"].as_str())
            .expect("a second monitor");
        let monitor = monitors()
            .unwrap()
            .into_iter()
            .find(|m| m.port == target)
            .unwrap();
        let class = format!("deckly-placement-{}", std::process::id());
        let placement = Placement {
            monitor: monitor.id.clone(),
            layout: Layout::Floating,
            ..Placement::default()
        };
        assert!(
            open(&format!("kitty --class {class}"), &placement)
                .unwrap()
                .is_empty()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let find = || {
            let clients: Vec<serde_json::Value> =
                serde_json::from_str(&request("j/clients").unwrap()).unwrap();
            clients.into_iter().find(|c| c["class"] == class.as_str())
        };
        while find().is_none() {
            assert!(std::time::Instant::now() < deadline, "no window");
            std::thread::sleep(Duration::from_millis(100));
        }
        // Rules apply as the window maps; read it once it has settled.
        std::thread::sleep(Duration::from_millis(500));
        let window = find().unwrap();
        let on = all.iter().find(|m| m["name"] == target).unwrap()["id"].clone();
        let _ = std::process::Command::new("kill")
            .arg(window["pid"].to_string())
            .status();
        assert_eq!(window["floating"], true);
        assert_eq!(window["monitor"], on);
    }
}
