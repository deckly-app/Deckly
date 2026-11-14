# Actions and keys

An **action** is one kind of key a plugin offers: "Wave", "CPU load",
"Start recording". A **key** is one place on a device where the user put an
action. One action can be on many keys, each with its own settings.

## The `Action` trait

Each action is a Rust type that implements [`Action`]:

```rust
use deckly_sdk::prelude::*;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct MuteSettings {
    device: String,
}

#[derive(Default)]
struct Mute {
    muted: bool,
}

impl Action for Mute {
    /// Reverse-DNS, starts with the plugin id. Never change it after
    /// release: profiles store it.
    const ID: &'static str = "com.example.audio.mute";
    /// Shown in Deckly's action list.
    const NAME: &'static str = "Mute";
    /// Optional: a hover text in the action list.
    const TOOLTIP: &'static str = "Mutes the microphone";
    /// Optional: an image inside the plugin folder, for the action list.
    const ICON: &'static str = "assets/mute.svg";

    type Settings = MuteSettings;

    fn key_down(&mut self, key: &Key<Self>) {
        self.muted = !self.muted;
        key.set_title(if self.muted { "Muted" } else { "Live" });
    }
}
#
# fn main() {
#     let _ = Plugin::new("com.example.audio", "Audio").action::<Mute>();
# }
```

Register every action on the plugin:

```rust,no_run
# use deckly_sdk::prelude::*;
# #[derive(Default)] struct Mute;
# impl Action for Mute { const ID: &'static str = "com.example.audio.mute"; const NAME: &'static str = "Mute"; type Settings = Empty; }
# #[derive(Default)] struct Volume;
# impl Action for Volume { const ID: &'static str = "com.example.audio.volume"; const NAME: &'static str = "Volume"; type Settings = Empty; }
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
fn main() -> deckly_sdk::Result<()> {
    Plugin::new("com.example.audio", "Audio")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Ada Lovelace")
        .action::<Mute>()
        .action::<Volume>()
        .run()
}
```

## One value per key

Deckly makes one value of your type (with `Default`) for each key that shows
the action, and drops it when the key goes away. So per-key state is a plain
field, like `muted` above, with no map from keys to state.

That state lives only while the key is shown. Switching pages, folders or
profiles drops it. Anything that must survive belongs in the key's
[settings](settings.md).

## Events

Every handler has an empty default; implement the ones you need.

| Handler | Called when |
| --- | --- |
| `key_appear` | The key is shown on a device. Draw it here. |
| `key_disappear` | The key is no longer shown. The value is dropped right after. |
| `key_down` | The key is pressed. |
| `key_up` | The key is released. |
| `settings_changed` | The user changed the key's settings in Deckly. |
| `tick` | The key's timer fired; see [Timers and threads](timers.md). |
| `button` | The user pressed a button in the key's [form](forms.md). |
| `global_settings_changed` | The plugin's [global settings](global-settings.md) changed. |

Two more are not about one key: `form` describes the settings form, and
`choices` lists the options of a dynamic choice. See
[Settings forms](forms.md).

A plugin hears of a key only while a device shows it. In the editor with no
device connected, the key shows the action's icon, and no events arrive.

All key events of a plugin run one after another on one thread. A handler
that blocks (a network call, a long computation) holds up every key of the
plugin. Move slow work to a thread of its own; see
[Timers and threads](timers.md).

## What a key can do

Each handler gets a [`Key`]. It is the handle for that one key:

| Method | What it does |
| --- | --- |
| `set_title`, `clear_title` | Text on the key. See [Drawing keys](drawing.md). |
| `set_face`, `clear_face` | A face drawn by Deckly. |
| `set_image`, `clear_image` | An image of your own. |
| `set_state` | Moves a key with several states to another state. |
| `show_ok`, `show_alert` | A short success or failure mark. |
| `settings`, `set_settings`, `update_settings` | The key's settings. |
| `globals` | The plugin's global settings. |
| `every`, `stop_timer` | A timer that calls `tick`. |
| `column`, `row`, `device` | Where the key is. |
| `state` | The state Deckly last reported. |
| `in_multi_action` | Whether the key is a step of a Multi Action. |
| `is_visible` | Whether the key is still shown. |
| `context` | Deckly's id for the key, stable while it exists. |

A `Key` is cheap to clone and can move to another thread. Calls on a key
that has disappeared do nothing, so a late update from a worker thread is
harmless.

The SDK sends an update only when it differs from the last one for that
key: calling `set_face` with the same face every second costs nothing.

## Multi Actions

A key can be a step of a user's Multi Action. When the Multi Action runs,
the step gets `key_down` and `key_up` like a pressed key, and
`key.in_multi_action()` is true. Do the action's work in `key_down`, so it
works the same on a key and in a Multi Action.

[`Action`]: https://deckly-app.github.io/Deckly/api/deckly_sdk/trait.Action.html
[`Key`]: https://deckly-app.github.io/Deckly/api/deckly_sdk/struct.Key.html
