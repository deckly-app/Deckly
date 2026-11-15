# Global settings and secrets

**Global settings** belong to the whole plugin, not to one key: a server
address, an account, a password. Every key sees the same ones. The user
edits them in the plugin's own form, opened from the gear button next to the
plugin in Deckly's Plugins tab.

## The plugin-wide form

Describe it on the `Plugin`, like an action's form:

```rust,no_run
use deckly_sdk::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
struct ObsGlobals {
    address: String,
    password: String,
}

impl Default for ObsGlobals {
    fn default() -> Self {
        Self { address: "ws://localhost:4455".into(), password: String::new() }
    }
}

# #[derive(Default)] struct Scene;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
# impl Action for Scene { const ID: &'static str = "scene"; const NAME: &'static str = "Scene"; type Settings = Empty; }
fn main() -> deckly_sdk::Result<()> {
    Plugin::new("com.example.obs", "OBS")
        .action::<Scene>()
        .form(|form| {
            form.text("address", "Address").default("ws://localhost:4455");
            form.secret("password", "Password");
            form.button("test", "Test the connection");
        })
        .on_button(|button, globals| {
            if button == "test" {
                let g = globals.get::<ObsGlobals>();
                eprintln!("testing {}", g.address);
            }
        })
        .on_global_settings(|globals| {
            let g = globals.get::<ObsGlobals>();
            eprintln!("now using {}", g.address);
        })
        .run()
}
```

| On `Plugin` | Does |
| --- | --- |
| `form(\|form\| ...)` | Describes the plugin-wide form. |
| `on_button(\|button, globals\| ...)` | Answers a button of that form. |
| `on_choices(\|field, globals\| ...)` | Lists the options of a dynamic choice in that form. Runs on its own thread. |
| `on_global_settings(\|globals\| ...)` | Hears that the global settings changed, before the keys do. |
| `on_start(\|plugin\| ...)` | Runs once at the start; see [Shared state](shared-state.md). |

## Reading and storing

Every handler can reach the global settings through `key.globals()`, which
returns a [`Globals`]:

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)]
# struct ObsGlobals { address: String, password: String }
# #[derive(Default)] struct Scene;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
# impl Action for Scene {
#     const ID: &'static str = "scene"; const NAME: &'static str = "Scene";
#     type Settings = Empty;
fn key_down(&mut self, key: &Key<Self>) {
    let globals = key.globals();
    if !globals.is_loaded() {
        return; // Deckly has not sent them yet.
    }
    let obs = globals.get::<ObsGlobals>();
    eprintln!("switching scene on {}", obs.address);
}

fn global_settings_changed(&mut self, key: &Key<Self>) {
    // The address may have changed: redraw, reconnect.
    key.set_title(key.globals().get::<ObsGlobals>().address);
}
# }
# fn main() {}
```

| On `Globals` | Does |
| --- | --- |
| `get::<G>()` | The settings as your type, read over `G::default()`. |
| `set(&g)` | Stores new settings; they reach every key. |
| `update::<G>(\|g\| ...)` | Changes them in place and stores them. |
| `value()` | The raw JSON. |
| `is_loaded()` | Whether Deckly has sent them yet. |

The SDK asks Deckly for the global settings as soon as the plugin starts.
Until they arrive, `get` returns the defaults and `is_loaded()` is false;
when they arrive, `global_settings_changed` runs on every key.

The same rules as for key settings apply: they are read over `Default`, so
a missing or unfitting field keeps its default; a field for every field of
the form (or a `#[serde(flatten)]` map for the rest), a JSON
object of at most 64 KiB.

## Secrets

A `secret` field, in a key's form or in the plugin-wide form, is for
passwords and tokens:

```rust
# use deckly_sdk::prelude::*;
# fn main() {
# let mut form = Form::default();
form.secret("token", "API token")
    .placeholder("Not set")
    .help("Create one under Settings → API on the service's site.");
# }
```

- Deckly keeps the value in the system's secret store: Credential Manager
  on Windows, the Keychain on macOS, the Secret Service on Linux. Without
  one, it falls back to a file in its data folder, readable only by the
  user.
- Profiles and backups hold only a reference to the secret, never the
  value.
- The plugin sees a plain string in its settings, like any text field. If
  the secret cannot be read (a locked keyring), the string is empty.
- Storing settings that contain the secret, for example to clear it, works
  like any other field.

Never write a secret to the log.

[`Globals`]: https://deckly-app.github.io/Deckly/api/deckly_sdk/struct.Globals.html
