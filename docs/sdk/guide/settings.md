# Settings

Each key has **settings**: a JSON object that Deckly saves in the user's
profile. They survive restarts, profile switches, copy and paste, and
backups. The plugin sees them as a Rust type.

## The settings type

```rust
use deckly_sdk::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
struct TimerSettings {
    minutes: u32,
    sound: bool,
    /// Kept in settings so it survives a restart.
    finished_runs: u64,
}

impl Default for TimerSettings {
    fn default() -> Self {
        Self { minutes: 5, sound: true, finished_runs: 0 }
    }
}
# fn main() {}
```

A new key starts with `Default`. The SDK reads saved settings over
`Default`, which keeps them working across versions:

- A field the saved settings lack (a key saved by an older version of the
  plugin, a field added since) takes its default value. No
  `#[serde(default)]` is needed.
- A field whose saved value does not fit its type (text where a number
  belongs) takes its default value too, with a line in the plugin's log;
  the other fields keep their saved values. Nothing is lost: Deckly keeps
  the stored object until the plugin saves new settings.
- They must serialize to a JSON object (a struct or a map), at most
  64 KiB.
- Give the type a field for every field of the [form](forms.md);
  `Plugin::check` (run by the test harness and `--describe`) finds a form
  field whose key is not a field of the type. Storing
  settings writes the whole object, so a property the type lacks is
  dropped. To keep properties you do not read, add a catch-all:
  `#[serde(flatten)] rest: serde_json::Map<String, serde_json::Value>`.
- To rename a field, keep the old name readable:
  `#[serde(alias = "old_name")]`.

## Reading and storing

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)]
# struct TimerSettings { minutes: u32, sound: bool, finished_runs: u64 }
# #[derive(Default)] struct Timer;
# impl Action for Timer {
#     const ID: &'static str = "run"; const NAME: &'static str = "Timer";
#     type Settings = TimerSettings;
fn key_down(&mut self, key: &Key<Self>) {
    // A copy of the current settings.
    let minutes = key.settings().minutes;
    key.set_title(format!("{minutes} min"));

    // Change and store them in one step; Deckly saves them.
    key.update_settings(|s| s.finished_runs += 1);
}
# }
# fn main() {}
```

- `key.settings()` returns a copy.
- `key.set_settings(value)` replaces them; `key.update_settings(|s| ...)`
  changes them in place. Both send them to Deckly, which saves them.
- When the user changes the settings in Deckly, the key's
  `settings_changed` handler runs and `key.settings()` already holds the new
  value. Redraw the key there.

## Settings or a field?

| Put it in | When it should |
| --- | --- |
| Settings | Survive a restart, be part of the profile, be edited by the user. |
| A field of the action | Live only while the key is shown: a running timer, a cached image. |
| [Global settings](global-settings.md) | Be shared by every key: a server address, a password. |

The user edits settings in a [settings form](forms.md) that you describe
in code.
