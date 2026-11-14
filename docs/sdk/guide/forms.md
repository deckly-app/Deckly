# Settings forms

The plugin describes a form in code, and Deckly draws it: in the key editor
for a key's settings, and from the Plugins tab for the plugin's
[global settings](global-settings.md). No HTML, and the form looks like the
rest of Deckly.

## A form and its settings

Each field has a **key**: the name of the settings property it edits. Keep
the keys equal to the field names of your settings type.

```rust
use deckly_sdk::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct PingSettings {
    host: String,
    every: u32,
    mode: String,
    alert: bool,
    alert_ms: u32,
}

impl Default for PingSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            every: 5,
            mode: "icmp".into(),
            alert: false,
            alert_ms: 200,
        }
    }
}

#[derive(Default)]
struct Ping;

impl Action for Ping {
    const ID: &'static str = "com.example.net.ping";
    const NAME: &'static str = "Ping";
    type Settings = PingSettings;

    fn form(form: &mut Form) {
        form.text("host", "Host")
            .placeholder("example.com")
            .required();
        form.number("every", "Every")
            .range(1..=3600)
            .default(5)
            .help("Seconds between pings.");
        form.choice("mode", "Method")
            .option("icmp", "ICMP")
            .option("tcp", "TCP port 443")
            .default("icmp");

        form.section("Alert");
        form.checkbox("alert", "Flash the key when it is slow");
        form.slider("alert_ms", "Slow above", 10..=2000)
            .step(10)
            .default(200)
            .visible_when("alert", true);
    }
}
# fn main() {
#     Plugin::new("com.example.net", "Net").action::<Ping>().check().unwrap();
# }
```

When the user changes a field, Deckly stores the new settings and the key's
`settings_changed` handler runs.

Keep each field's `default` equal to the `Default` of the settings type:
the form shows the default while the settings hold no value, and the
plugin reads the type's default in that case.

## Field kinds

| Method | Edits | Value in settings |
| --- | --- | --- |
| `text` | One line, or several with `.multiline()` | string |
| `secret` | A password or token, kept in the system's secret store | string |
| `number` | A number box | number |
| `slider` | A slider between two numbers | number |
| `checkbox` | On or off | `true` / `false` |
| `choice` | One option of a list | string (the option's value) |
| `file` | A file's path, with `.extensions(&["wav"])` | string |
| `folder` | A folder's path | string |
| `color` | A colour | `"#rrggbb"` |
| `hotkey` | A key combination | object, see [Form fields](../reference/form-fields.md#hotkey) |
| `button` | Calls the action's `button` handler | none |
| `section` | A heading over the fields after it | none |
| `note` | A line of text | none |

Every builder and property is in the [Form fields](../reference/form-fields.md)
reference.

## Checking input

- `.required()`: an empty value is refused.
- `.range(a..=b)`, `.min(a)`, `.max(b)`, `.step(s)`: number bounds. An
  integer range (`1..=100`) also makes the field whole numbers only, so it
  never stores `2.5` in a `u32`.
- `.length(1..=12)`: the length of a text, in characters.

Deckly shows the problem next to the field and keeps the value out of the
settings until it fits.

## Showing a field only sometimes

`.visible_when("alert", true)` shows the field only while the field
`alert` holds `true`. It works with any value: `.visible_when("mode", "tcp")`.

## Options from the plugin

A choice marked `.dynamic()` asks the plugin for its options each time the
form opens: the scenes in OBS, the audio devices, the files in a folder.
The action's `choices` handler answers:

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)] #[serde(default)] struct SceneSettings { scene: String }
# fn scene_names() -> Result<Vec<String>, String> { Ok(vec!["Live".into()]) }
# #[derive(Default)] struct Scene;
impl Action for Scene {
#     const ID: &'static str = "com.example.obs.scene"; const NAME: &'static str = "Scene";
#     type Settings = SceneSettings;
    // ...
    fn form(form: &mut Form) {
        form.choice("scene", "Scene").dynamic().required();
    }

    fn choices(
        field: &str,
        _settings: &SceneSettings,
        _globals: &Globals,
    ) -> Result<Vec<Choice>, String> {
        match field {
            "scene" => {
                let names = scene_names().map_err(|e| format!("OBS: {e}"))?;
                Ok(names.into_iter().map(|n| Choice::new(n.clone(), n)).collect())
            }
            _ => Err(format!("no options for {field}")),
        }
    }
}
# fn main() {}
```

- `choices` runs on a thread of its own, so a slow answer does not hold up
  key presses. It cannot change the action's fields: it gets the key's
  settings and the global settings, not `&mut self`.
- Deckly stops waiting after a few seconds and offers a retry.
- An `Err` is shown to the user in the form. Write it for them.
- `Choice::new(value, label)`: the settings keep `value`; the user sees
  `label`.

## Buttons

`form.button("reset", "Reset")` draws a button. Pressing it calls the
action's `button` handler with the button's key:

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)] #[serde(default)] struct CountSettings { count: u64 }
# #[derive(Default)] struct Count;
# impl Action for Count {
#     const ID: &'static str = "com.example.count.count"; const NAME: &'static str = "Count";
#     type Settings = CountSettings;
fn button(&mut self, key: &Key<Self>, button: &str) {
    if button == "reset" {
        key.update_settings(|s| s.count = 0);
    }
}
# }
# fn main() {}
```

The key may be in the editor only, with no device showing it. Then the
handler can change settings, but drawing does nothing.

## Check the form in a test

`Plugin::check()` validates the manifest as Deckly will: a form over a
limit, or a default that its field refuses, fails here instead of in
Deckly. Call it in a test:

```rust,ignore
#[test]
fn manifest_is_valid() {
    plugin().check().unwrap();
}
```
