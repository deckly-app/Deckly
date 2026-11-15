//! Icons by meaning: a small table of the words people use for what a key
//! does, and suggestions taken from the key itself. Everything here is pure
//! and bounded; key settings are untrusted JSON.

use deckly_core::KeyConfig;
use egui_phosphor::regular as icon;
use serde_json::Value;

/// At most this many icons are suggested for a key.
pub const MAX_SUGGESTED: usize = 8;
/// Texts read from one key, its settings and its steps together.
const MAX_HINTS: usize = 48;
/// Characters read from one text.
const MAX_HINT_CHARS: usize = 256;
/// Words read from one text; longer words are skipped.
const MAX_WORDS: usize = 32;
const MAX_WORD: usize = 32;
/// Nesting of settings objects and of steps inside steps.
const MAX_DEPTH: usize = 4;

/// Words, written compact (lowercase letters and digits only), and the
/// icons that fit them, best first. Our own list; extend it as needed.
const MEANINGS: &[(&[&str], &[&str])] = &[
    (
        &[
            "shutdown",
            "shutoff",
            "poweroff",
            "powerdown",
            "turnoff",
            "switchoff",
            "halt",
            "power",
        ],
        &["POWER"],
    ),
    (
        &["reboot", "restart", "reload", "refresh"],
        &["ARROW_CLOCKWISE", "ARROWS_CLOCKWISE", "POWER"],
    ),
    (
        &["sleep", "suspend", "hibernate", "standby", "night"],
        &["MOON", "MOON_STARS"],
    ),
    (
        &["lock", "lockscreen", "screenlock", "lockscreensaver"],
        &["LOCK", "LOCK_SIMPLE", "LOCK_KEY"],
    ),
    (
        &["logout", "logoff", "signout", "exit", "quit"],
        &["SIGN_OUT"],
    ),
    (
        &["mute", "unmute", "silence", "silent"],
        &["SPEAKER_SLASH", "SPEAKER_X", "MICROPHONE_SLASH"],
    ),
    (
        &["volume", "volumeup", "louder", "sound", "audio", "speaker"],
        &["SPEAKER_HIGH", "SPEAKER_SIMPLE_HIGH", "SPEAKER_LOW"],
    ),
    (
        &["volumedown", "quieter"],
        &["SPEAKER_LOW", "SPEAKER_SIMPLE_LOW"],
    ),
    (&["mic", "microphone"], &["MICROPHONE", "MICROPHONE_SLASH"]),
    (
        &["lamp", "light", "lights", "bulb", "hue"],
        &["LIGHTBULB", "LAMP", "LIGHTBULB_FILAMENT"],
    ),
    (
        &[
            "notification",
            "notifications",
            "notify",
            "alert",
            "alerts",
            "dnd",
            "donotdisturb",
        ],
        &["BELL", "BELL_RINGING", "BELL_SLASH"],
    ),
    (
        &[
            "terminal",
            "console",
            "shell",
            "bash",
            "zsh",
            "powershell",
            "cmd",
            "konsole",
        ],
        &["TERMINAL_WINDOW", "TERMINAL"],
    ),
    (
        &[
            "browser", "web", "website", "internet", "url", "http", "https", "www", "firefox",
            "chromium", "safari",
        ],
        &["GLOBE", "BROWSER", "BROWSERS"],
    ),
    (&["chrome"], &["GOOGLE_CHROME_LOGO", "GLOBE"]),
    (
        &[
            "screenshot",
            "screengrab",
            "screencapture",
            "printscreen",
            "capture",
            "snip",
            "spectacle",
            "flameshot",
        ],
        &["CAMERA", "SELECTION", "SCAN", "CROP"],
    ),
    (
        &["playpause", "play", "resume", "toggleplay"],
        &["PLAY_PAUSE", "PLAY", "PAUSE"],
    ),
    (&["pause"], &["PAUSE", "PLAY_PAUSE"]),
    (&["stop"], &["STOP"]),
    (
        &["next", "nexttrack", "skip", "forward"],
        &["SKIP_FORWARD", "FAST_FORWARD"],
    ),
    (
        &["previous", "prev", "previoustrack", "rewind"],
        &["SKIP_BACK", "REWIND"],
    ),
    (
        &[
            "music",
            "song",
            "songs",
            "media",
            "multimedia",
            "player",
            "playlist",
        ],
        &["MUSIC_NOTES", "MUSIC_NOTE", "PLAY_PAUSE", "HEADPHONES"],
    ),
    (
        &["camera", "webcam", "cam", "video"],
        &["VIDEO_CAMERA", "CAMERA", "VIDEO_CAMERA_SLASH"],
    ),
    (
        &[
            "record",
            "recording",
            "rec",
            "stream",
            "streaming",
            "live",
            "obs",
        ],
        &["RECORD", "BROADCAST", "VIDEO_CAMERA"],
    ),
    (
        &["brightness", "bright", "brighter", "dim", "dimmer"],
        &["SUN", "SUN_DIM"],
    ),
    (
        &["mail", "email", "inbox", "thunderbird", "outlook"],
        &["ENVELOPE", "ENVELOPE_SIMPLE"],
    ),
    (
        &["chat", "message", "messages", "messenger"],
        &["CHAT_CIRCLE", "CHATS"],
    ),
    (
        &["calendar", "schedule", "meeting", "agenda"],
        &["CALENDAR", "CALENDAR_BLANK"],
    ),
    (
        &["timer", "stopwatch", "countdown", "delay", "wait"],
        &["TIMER", "CLOCK", "ALARM"],
    ),
    (&["clock", "time", "alarm"], &["CLOCK", "ALARM"]),
    (
        &[
            "folder",
            "folders",
            "directory",
            "files",
            "explorer",
            "finder",
            "nautilus",
            "dolphin",
        ],
        &["FOLDER_OPEN", "FOLDER"],
    ),
    (
        &[
            "settings",
            "preferences",
            "config",
            "configure",
            "options",
            "controlpanel",
        ],
        &["GEAR", "GEAR_SIX", "SLIDERS"],
    ),
    (&["home", "desktop"], &["HOUSE", "DESKTOP"]),
    (&["delete", "remove", "trash", "recycle"], &["TRASH"]),
    (&["copy", "duplicate"], &["COPY", "CLIPBOARD"]),
    (&["paste", "clipboard"], &["CLIPBOARD", "CLIPBOARD_TEXT"]),
    (&["cut"], &["SCISSORS"]),
    (&["keyboard", "hotkey", "shortcut", "keys"], &["KEYBOARD"]),
    (&["search", "find", "lookup"], &["MAGNIFYING_GLASS"]),
    (&["calculator", "calc"], &["CALCULATOR"]),
    (
        &["code", "editor", "vscode", "ide", "programming"],
        &["CODE", "TERMINAL_WINDOW"],
    ),
    (
        &["game", "games", "gaming", "steam"],
        &["GAME_CONTROLLER", "STEAM_LOGO"],
    ),
    (&["print", "printer"], &["PRINTER"]),
    (&["wifi", "network", "wireless"], &["WIFI_HIGH"]),
    (&["monitor", "display", "screen"], &["MONITOR", "DESKTOP"]),
    (&["temperature", "heating", "thermostat"], &["THERMOMETER"]),
    (&["tv", "television"], &["TELEVISION"]),
    (&["note", "notes", "notepad", "memo"], &["NOTE", "NOTEPAD"]),
    (&["user", "account", "profile"], &["USER"]),
    (&["random", "shuffle"], &["SHUFFLE"]),
    (&["repeat", "loop"], &["REPEAT"]),
    (&["text", "type", "typing"], &["TEXT_T", "KEYBOARD"]),
    (&["nextpage"], &["ARROW_SQUARE_RIGHT"]),
    (&["previouspage"], &["ARROW_SQUARE_LEFT"]),
    (&["gotopage", "page"], &["HASH"]),
    (&["launch", "app", "application", "run"], &["ROCKET_LAUNCH"]),
];

/// `text` in the form the table uses: lowercase ASCII letters and digits,
/// so "Shut down", "shut-down" and "SHUTDOWN" are the same word.
pub fn compact(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_HINT_CHARS)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The icons the table gives for a search `query`, best first. A query of
/// three or more letters also matches the start of a word ("shut" finds
/// the power icon), and each word of a longer query is looked up alone.
pub fn by_meaning(query: &str) -> Vec<&'static str> {
    let whole = compact(query);
    let mut out = Vec::new();
    if whole.is_empty() {
        return out;
    }
    meaning(&whole, &mut out);
    let words: Vec<String> = query
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(MAX_WORDS)
        .map(compact)
        .collect();
    if words.len() > 1 {
        for word in &words {
            meaning(word, &mut out);
        }
    }
    if whole.len() >= 3 {
        for (words, icons) in MEANINGS {
            if words.iter().any(|w| w.starts_with(whole.as_str())) {
                push_all(icons, &mut out);
            }
        }
    }
    out
}

/// Icons that fit `key`, best first, from its title, its settings, the
/// steps inside it and its action. Empty when the key gives no hint.
pub fn suggestions(key: &KeyConfig) -> Vec<&'static str> {
    let mut hints = Vec::new();
    collect_hints(key, 0, &mut hints);
    let mut out = Vec::new();
    for hint in &hints {
        let words: Vec<String> = hint
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty() && w.len() <= MAX_WORD)
            .take(MAX_WORDS)
            .map(compact)
            .collect();
        for (i, word) in words.iter().enumerate() {
            // Two words first, so "shut down" and "power off" count as one.
            if let Some(next) = words.get(i + 1) {
                meaning(&format!("{word}{next}"), &mut out);
            }
            meaning(word, &mut out);
            if word.len() >= 3 {
                let upper = word.to_ascii_uppercase();
                for name in [upper.clone(), format!("{upper}_LOGO")] {
                    if let Some(&(name, _)) = icon::ICONS.iter().find(|(n, _)| *n == name) {
                        push_all(&[name], &mut out);
                    }
                }
            }
            if out.len() >= MAX_SUGGESTED {
                out.truncate(MAX_SUGGESTED);
                return out;
            }
        }
    }
    out
}

/// The catalog position of the icon called `name`.
pub fn index_of(name: &str) -> Option<usize> {
    icon::ICONS.iter().position(|(n, _)| *n == name)
}

fn meaning(word: &str, out: &mut Vec<&'static str>) {
    for (words, icons) in MEANINGS {
        if words.contains(&word) {
            push_all(icons, out);
        }
    }
}

fn push_all(icons: &[&'static str], out: &mut Vec<&'static str>) {
    for name in icons {
        if !out.contains(name) {
            out.push(name);
        }
    }
}

fn collect_hints(key: &KeyConfig, depth: usize, hints: &mut Vec<String>) {
    push_hint(&key.title, hints);
    setting_hints(&key.settings, 0, hints);
    if depth < MAX_DEPTH {
        for step in key.groups.iter().flatten() {
            if hints.len() >= MAX_HINTS {
                return;
            }
            collect_hints(step, depth + 1, hints);
        }
    }
    let name = crate::actions::ACTIONS
        .iter()
        .chain(crate::actions::PLACED)
        .find(|a| a.uuid == key.action)
        .map_or(key.action.as_str(), |a| a.name);
    push_hint(name, hints);
}

/// String values of the settings; keys are field names, not hints.
fn setting_hints(value: &Value, depth: usize, hints: &mut Vec<String>) {
    match value {
        Value::String(text) => push_hint(text, hints),
        Value::Array(items) if depth < MAX_DEPTH => {
            for item in items.iter().take(MAX_HINTS) {
                setting_hints(item, depth + 1, hints);
            }
        }
        Value::Object(map) if depth < MAX_DEPTH => {
            for item in map.values().take(MAX_HINTS) {
                setting_hints(item, depth + 1, hints);
            }
        }
        _ => {}
    }
}

fn push_hint(text: &str, hints: &mut Vec<String>) {
    if hints.len() < MAX_HINTS && !text.trim().is_empty() {
        hints.push(text.chars().take(MAX_HINT_CHARS).collect());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_table_icon_is_in_the_catalog_and_words_are_compact() {
        for (words, icons) in MEANINGS {
            for word in *words {
                assert_eq!(compact(word), *word);
            }
            for name in *icons {
                assert!(index_of(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn queries_are_compared_without_case_spaces_or_hyphens() {
        assert_eq!(compact("Shut down"), "shutdown");
        assert_eq!(compact("shut-down"), "shutdown");
        assert_eq!(compact(" SHUT_DOWN "), "shutdown");
        assert_eq!(compact("Play/Pause"), "playpause");
        assert_eq!(compact(&"a".repeat(10_000)).len(), MAX_HINT_CHARS);
    }

    #[test]
    fn words_find_the_fitting_icons() {
        for query in [
            "shutdown",
            "shut down",
            "Shut-Down",
            "power off",
            "turn off",
            "shut",
        ] {
            assert_eq!(by_meaning(query).first(), Some(&"POWER"), "{query}");
        }
        for (query, name) in [
            ("reboot", "ARROW_CLOCKWISE"),
            ("restart", "ARROW_CLOCKWISE"),
            ("sleep", "MOON"),
            ("lock", "LOCK"),
            ("mute", "SPEAKER_SLASH"),
            ("volume", "SPEAKER_HIGH"),
            ("lamp", "LIGHTBULB"),
            ("light", "LIGHTBULB"),
            ("notification", "BELL"),
            ("terminal", "TERMINAL_WINDOW"),
            ("browser", "GLOBE"),
            ("screenshot", "CAMERA"),
            ("play/pause", "PLAY_PAUSE"),
            ("play pause", "PLAY_PAUSE"),
        ] {
            assert!(by_meaning(query).contains(&name), "{query}");
        }
        assert!(by_meaning("").is_empty());
        assert!(by_meaning("zz").is_empty());
        assert!(by_meaning("no-such-deckly-word").is_empty());
    }

    #[test]
    fn an_open_key_that_shuts_down_suggests_power_first() {
        for command in [
            "shutdown",
            "sudo shutdown now",
            "systemctl poweroff",
            "shutdown /s /t 0",
        ] {
            let key = KeyConfig::new(crate::actions::OPEN, json!({ "path": command }));
            assert_eq!(suggestions(&key).first(), Some(&"POWER"), "{command}");
        }
        let mut key = KeyConfig::new(crate::actions::OPEN, json!({ "path": "" }));
        key.title = "Shut down".into();
        assert_eq!(suggestions(&key).first(), Some(&"POWER"));
    }

    #[test]
    fn a_multi_action_suggests_from_its_steps() {
        let mut key = KeyConfig::new(crate::actions::MULTI_ACTION, json!({}));
        key.groups = vec![vec![
            KeyConfig::new(crate::actions::MULTIMEDIA, json!({ "media": "play_pause" })),
            KeyConfig::new(
                crate::actions::OPEN,
                json!({ "path": "loginctl lock-session" }),
            ),
        ]];
        let suggested = suggestions(&key);
        assert_eq!(suggested.first(), Some(&"PLAY_PAUSE"));
        assert!(suggested.contains(&"LOCK"));
        assert!(suggested.len() <= MAX_SUGGESTED);
    }

    #[test]
    fn keys_without_hints_suggest_nothing_and_hostile_settings_are_bounded() {
        let key = KeyConfig::new(crate::actions::OPEN, json!({ "path": "" }));
        assert!(suggestions(&key).is_empty());

        let mut deep = json!("shutdown");
        for _ in 0..100 {
            deep = json!([deep]);
        }
        let mut wide = serde_json::Map::new();
        for i in 0..10_000 {
            wide.insert(format!("k{i}"), json!("x".repeat(1000)));
        }
        let long = "lock ".repeat(100_000);
        for settings in [deep, Value::Object(wide), json!({ "path": long })] {
            let key = KeyConfig::new("com.example.plugin.action", settings);
            assert!(suggestions(&key).len() <= MAX_SUGGESTED);
        }
        // Steps nested far deeper than any real key stop at the limit.
        let mut key = KeyConfig::new(crate::actions::MULTI_ACTION, json!({}));
        for _ in 0..100 {
            let mut outer = KeyConfig::new(crate::actions::MULTI_ACTION, json!({}));
            outer.groups = vec![vec![key]];
            key = outer;
        }
        assert!(suggestions(&key).len() <= MAX_SUGGESTED);
    }

    #[test]
    fn plugin_actions_suggest_from_their_uuid() {
        let key = KeyConfig::new("com.example.discord.mute", json!({}));
        let suggested = suggestions(&key);
        assert!(suggested.contains(&"DISCORD_LOGO"));
        assert!(suggested.contains(&"SPEAKER_SLASH"));
    }
}
