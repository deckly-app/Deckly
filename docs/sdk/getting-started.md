# Getting started

In about ten minutes you build a plugin with one action, **Wave**: a key
that greets a name the user types and counts its presses. Then you install
it in Deckly and write a test for it.

## 1. What you need

- **Rust** 1.95 or newer, from [rustup.rs](https://rustup.rs).
- **Deckly** that runs Deckly plugins. Until the next release that is a
  build from the `dev` branch:

  ```sh
  git clone -b dev https://github.com/deckly-app/Deckly
  cd Deckly
  cargo dev --release
  ```

  The [Deckly README](https://github.com/deckly-app/Deckly#build) lists the
  system libraries the build needs on Linux.
- A Stream Deck to see the key. Without one you can still build, install
  and test the plugin (step 6).

## 2. Make the crate

```sh
cargo new hello-deckly
cd hello-deckly
```

Add the SDK, and `serde` for the settings type:

```sh
cargo add deckly-sdk
cargo add serde --features derive
```

## 3. Write the action

Replace `src/main.rs` with:

<!-- example: ../../examples/hello-deckly/src/main.rs:plugin -->
```rust,no_run
use deckly_sdk::prelude::*;

/// What the user sets for each key. Deckly saves it in the profile.
#[derive(Clone, Default, Serialize, Deserialize)]
struct WaveSettings {
    name: String,
}

/// Deckly makes one `Wave` for each key that shows the action.
#[derive(Default)]
struct Wave {
    presses: u32,
}

impl Action for Wave {
    const ID: &'static str = "wave";
    const NAME: &'static str = "Wave";
    type Settings = WaveSettings;

    // The settings form Deckly shows for the key.
    fn form(form: &mut Form) {
        form.text("name", "Name").placeholder("World");
    }

    fn key_down(&mut self, _key: &Key<Self>) {
        self.presses += 1;
    }

    // What the key shows. Deckly draws it again whenever it changes.
    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let mut name = key.settings().name;
        if name.is_empty() {
            name = "World".into();
        }
        Some(
            Face::new()
                .label(format!("Hello, {name}"))
                .value(self.presses.to_string())
                .detail("presses"),
        )
    }
}

deckly_sdk::plugin!("com.example.hello", "Hello", [Wave]);
```

What it does:

- `Wave` is the action. `ID` is short: the SDK puts the plugin id in front,
  so Deckly knows it as `com.example.hello.wave`.
- `form` describes the settings form. The field key `name` is the field of
  `WaveSettings` it edits. A key starts with `WaveSettings::default()`, and
  saved settings missing a field keep its default.
- `key_down` is an event: it changes the action's state. After every event
  the SDK calls `face`, which turns the state into what the key shows: a
  label, a big value and a detail line, drawn by Deckly. The face goes to
  the key only when it changed.
- `plugin!` names the plugin (its id and name) and its actions. It writes
  `main`, and a `plugin()` function the tests in step 6 use. The version,
  authors, licence and description come from `Cargo.toml`.

Never use `println!` in a plugin: standard output is how the plugin talks to
Deckly. Log with `eprintln!` instead.

## 4. Build the plugin folder

A plugin is a folder named by its id. It holds the program and
`deckly-plugin.json`, the manifest. The program writes its own manifest when
run with `--describe`.

Linux and macOS:

```sh
cargo build --release
mkdir -p dist/com.example.hello
cp target/release/hello-deckly dist/com.example.hello/
./target/release/hello-deckly --describe > dist/com.example.hello/deckly-plugin.json
```

Windows (PowerShell):

```powershell
cargo build --release
New-Item -ItemType Directory -Force dist\com.example.hello | Out-Null
Copy-Item target\release\hello-deckly.exe dist\com.example.hello\
.\target\release\hello-deckly.exe --describe | Out-File -Encoding utf8 dist\com.example.hello\deckly-plugin.json
```

Add `/dist` to `.gitignore`.

## 5. Install it and press the key

1. In Deckly, open the **Plugins** tab and click **Add Folder…**.
2. Pick `dist/com.example.hello`. Deckly copies it and starts the plugin;
   **Hello** shows up in the list.
3. Back in the editor, find **Hello → Wave** in the action list and drag it
   onto a key.
4. Press the key. The count goes up. Select the key and type a name in its
   settings: the label changes.

After a change to the code, repeat step 4 and **Add Folder…** again. Deckly
replaces the installed copy and restarts the plugin.

If nothing happens, right-click the plugin in the Plugins tab and choose
**Open Log**. See [Logs and troubleshooting](guide/troubleshooting.md).

## 6. Test it without Deckly

The SDK has a test harness that plays Deckly, so tests need neither Deckly
nor a device. Add this to the end of `src/main.rs`:

<!-- example: ../../examples/hello-deckly/src/main.rs:tests -->
```rust,ignore
#[cfg(test)]
mod tests {
    use super::*;
    use deckly_sdk::testing::Harness;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn greets_and_counts() {
        let mut deck = Harness::start(plugin());

        // The key appears with its saved settings and draws its first face.
        deck.appear("key-1", Wave::ID, serde_json::json!({ "name": "Ada" }));
        let face = deck.face("key-1", WAIT).unwrap();
        assert_eq!(face.label.as_deref(), Some("Hello, Ada"));
        assert_eq!(face.value.as_deref(), Some("0"));

        deck.press("key-1");
        let face = deck.face("key-1", WAIT).unwrap();
        assert_eq!(face.value.as_deref(), Some("1"));

        deck.finish().unwrap();
    }
}
```

`deck.face` waits for the next face the plugin draws on that key. The test
uses `serde_json`, so add it for tests only, then run it:

```sh
cargo add --dev serde_json
cargo test
```

## Next

- [Actions and keys](guide/actions.md): every event, and what a key can do.
- [Drawing keys](guide/drawing.md): titles, faces, images and states.
- [Settings forms](guide/forms.md): all field kinds.
- [Packaging and installing](guide/packaging.md): icons, several
  systems, sharing the plugin.
