# Getting started

In about ten minutes you make a plugin with one action, run it in Deckly,
change it while it runs, test it, and pack it into one file to share. The
plugin tool, `deckly-cli`, does the setup; you write the action.

## 1. What you need

- **Rust** 1.95 or newer, from [rustup.rs](https://rustup.rs). A plugin is
  a Rust program.
- **Deckly** that runs Deckly plugins. Until the next release that is a
  build from the `dev` branch:

  ```sh
  git clone -b dev https://github.com/deckly-app/Deckly
  cd Deckly
  cargo dev --release
  ```

  The [Deckly README](https://github.com/deckly-app/Deckly#build) lists the
  system libraries the build needs on Linux.
- A Stream Deck to press the key. Without one you can still build, run and
  test the plugin; the editor shows the key.

## 2. Install the plugin tool

Once, on Windows, Linux or macOS:

```sh
cargo install --git https://github.com/deckly-app/Deckly deckly-cli
```

Once the crates are published on crates.io this becomes
`cargo install deckly-cli`. The tool is also a Cargo subcommand: every
`deckly-cli ...` below works as `cargo deckly ...` too.

## 3. Create the project

In any folder:

```sh
deckly-cli create
```

It asks a few questions; Enter takes the suggestion each time:

| Question | Suggested |
| --- | --- |
| Author | your `git config user.name` |
| Plugin name | none: type one, such as `Hello` |
| Plugin id | `com.<author>.<name>`; never change it after the first release |
| Description | `<name> for Deckly.` |
| Example | **Counter** (a key with settings, a face and tests) or **Minimal** (one key) |
| Licence | MIT, Apache-2.0, both, or none |

Then it shows a summary, and on yes it makes the folder `hello/`, builds
the plugin, makes a git repository with a first commit, and links the
plugin into Deckly:

```text
hello/
├── Cargo.toml          deckly-sdk and serde; a lint that refuses println!
├── src/main.rs         the plugin: the example action and its tests
├── assets/             the plugin's icon and the action's icon (SVG)
├── README.md           how to develop, test, pack and release it
├── LICENSE
└── .github/workflows/release.yml   packs it for every system on a tag
```

Every question is also a flag, and `--yes` takes the defaults without
asking: `deckly-cli create Hello --yes --example minimal`. See
`deckly-cli help create`, or [deckly-cli](reference/cli.md) for every
command.

## 4. Run it in Deckly

```sh
cd hello
deckly-cli dev
```

`dev` builds the plugin and tells the running Deckly to start it. In
Deckly, the Plugins tab now lists **Hello** with a **Development** mark.
In the editor, find **Hello → Counter** in the action list, drag it onto a
key and press the key: the count goes up.

Leave `dev` running. Every time you save a file in `src/` or `assets/`, it
builds again and Deckly restarts the plugin with the new build. A build
that fails leaves the running plugin as it was; fix it and save again.
Whatever the plugin writes with `eprintln!` shows up in the same terminal.

If Deckly is not running, `dev` says so; the plugin starts with Deckly.

## 5. Write your own action

Replace `src/main.rs` with this action, **Wave**: a key that greets a name
the user types and counts its presses. Keep the plugin id and name that
`create` wrote on the last line, in place of `"com.example.hello", "Hello"`.

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

Save it: `dev` builds it and Deckly restarts the plugin, now with **Wave**
in the action list.

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
  `main`, and a `plugin()` function the tests use. The version, authors,
  licence and description come from `Cargo.toml`.

Never use `println!` in a plugin: standard output is how the plugin talks to
Deckly. Log with `eprintln!` instead. The project's `Cargo.toml` makes
`cargo clippy` refuse `println!`.

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

`deck.face` waits for the next face the plugin draws on that key. Run the
tests, then let the tool check the plugin as Deckly will:

```sh
cargo test
deckly-cli validate
```

`validate` checks the plugin folder (the manifest, its icons and program)
and starts the plugin once, as Deckly does: it must answer, and stop when
Deckly closes its input. A `println!` or slow work before the plugin
answers shows up here, with a plain explanation.

## 7. Share it

```sh
deckly-cli pack
```

`pack` builds with optimizations, validates, and writes one file,
`target/deckly/<plugin id>-<version>-<system>.decklyPlugin`. Anyone with
Deckly installs it with **Add File…** in the Plugins tab, or by dropping it
on the window.

To ship one file for Windows, Linux and macOS, push a tag such as `v0.0.1`
to GitHub: the workflow `create` wrote packs the plugin on each system,
joins the files with `deckly-cli pack --merge`, and attaches the result to
a GitHub release. See [Packaging and installing](guide/packaging.md).

## What the tool does for you

Nothing in `deckly-cli` is magic; by hand the steps are:

1. `cargo build`.
2. Make a plugin folder named by the plugin id, with the program in it and
   `deckly-plugin.json`, which the program prints when run with
   `--describe`. `deckly-cli build` makes it in `target/deckly/<plugin id>/`,
   with the program under `bin/<system>/` and a copy of `assets/`.
3. Show the folder to Deckly: **Add Folder…** in the Plugins tab copies it
   and starts it. `deckly-cli link` instead lists it in Deckly's
   `dev-plugins.json`, and Deckly runs it as a development plugin from a
   fresh copy each time it restarts, so a new build can replace the program
   while the old one runs.
4. After each change, build again and restart the plugin. `deckly-cli dev`
   watches the files and asks the running Deckly to restart the plugin over
   the local port it already listens on.

The [manifest](reference/manifest.md) and [plugin folders](reference/folders.md)
references describe each part.

## Next

- [Actions and keys](guide/actions.md): every event, and what a key can do.
- [Drawing keys](guide/drawing.md): titles, faces, images and states.
- [Settings forms](guide/forms.md): all field kinds.
- [Packaging and installing](guide/packaging.md): icons, several
  systems, sharing the plugin.
