# Packaging and installing

## The plugin folder

Deckly installs a plugin as a folder named by the plugin id:

```text
com.example.hello/
├── deckly-plugin.json      the manifest
├── hello-deckly            the program (hello-deckly.exe on Windows)
└── assets/
    ├── plugin.svg          Plugin::icon
    ├── wave.svg            Action::ICON
    └── face.svg            Face::icon
```

Every path in the manifest and in a face is relative to this folder and
must stay inside it.

## The manifest

`deckly-plugin.json` tells Deckly what the plugin is: its name, its actions
and their forms, and which program to start. Do not write it by hand: the
program prints its own with `--describe`, built from the `Plugin` and the
actions in your code:

```sh
./target/release/hello-deckly --describe > deckly-plugin.json
```

`--describe` also checks it as Deckly will and fails on a manifest Deckly
would refuse. Every field is in the [manifest reference](../reference/manifest.md).

Rebuild the manifest whenever an action, a form or the version changes.

## Icons and images

| Where | Set with | Formats |
| --- | --- | --- |
| The plugin in the Plugins tab | `Plugin::icon("assets/plugin.svg")` | PNG, SVG |
| An action in the action list | `const ICON: &'static str = "assets/wave.svg"` | PNG, SVG |
| A state's image | `State { image: "assets/on.svg".into(), .. }` | PNG, SVG |
| An icon on a face | `Face::new().icon("assets/face.svg")` | PNG, JPEG, SVG |

Copy the files into the folder next to the program. SVG stays sharp on
every key size. Use your own artwork: do not copy icons from other apps or
plugins.

## Version

Set the version from `Cargo.toml`, so there is one place to change it:

```rust
# use deckly_sdk::prelude::*;
# fn main() {
let plugin = Plugin::new("com.example.hello", "Hello")
    .version(env!("CARGO_PKG_VERSION"))
    .author("Ada Lovelace")
    .license("MIT")
    .description("Greets people.")
    .icon("assets/plugin.svg");
# let _ = plugin;
# }
```

## Several systems

`--describe` names only the program it was run as, for the system it was
built for:

```json
"programs": {
  "x86_64-unknown-linux-gnu": "hello-deckly"
}
```

To ship one folder for several systems, build on each (CI runners for
Linux, Windows and macOS are the easy way), put each program in a folder of
its own, and list them all:

```json
"programs": {
  "x86_64-unknown-linux-gnu": "bin/linux-x64/hello-deckly",
  "aarch64-unknown-linux-gnu": "bin/linux-arm64/hello-deckly",
  "x86_64-pc-windows-msvc": "bin/windows-x64/hello-deckly.exe",
  "aarch64-apple-darwin": "bin/macos-arm64/hello-deckly",
  "x86_64-apple-darwin": "bin/macos-x64/hello-deckly"
}
```

Deckly starts the program for its own target. When the exact target is
missing, it takes one for the same processor and system, so a `musl` build
also runs where Deckly is a `gnu` build. On a system with no program, the
plugin does not start and the Plugins tab says it "has no program for" that
target.

Keep the executable bit on Linux and macOS programs (`chmod +x`).

A `deckly-cli pack` command that merges the targets is planned. Until then,
edit `programs` after `--describe`.

## Installing

- **Add Folder…** in the Plugins tab installs a plugin folder. Deckly copies
  it into its [plugins folder](../reference/folders.md) and starts it. Doing
  it again with a newer build replaces the installed copy and restarts the
  plugin.
- Or copy the folder into the plugins folder yourself and restart Deckly.
- **Restart** in a plugin's right-click menu restarts it, and **Uninstall**
  removes it.

Plugin packages (a `.decklyPlugin` file to double-click) and a plugin
catalog inside Deckly are planned. Until they exist, share the plugin
folder, for example as a zip attached to a GitHub release, and users install
it with **Add Folder…** after unpacking it.

## Faster rebuilds while developing

On Linux you can skip **Add Folder…**: put the new program in place of the
installed one in the [plugins folder](../reference/folders.md), then choose
**Restart** in the plugin's menu. Use `install`, which replaces the file;
`cp` fails with "Text file busy" while the old program runs:

```sh
install -m 755 target/release/hello-deckly ~/.config/deckly/plugins/com.example.hello/
```

On Windows and macOS, use **Add Folder…**, which stops the plugin before
it replaces the files.

A `deckly-cli dev` command that rebuilds and restarts by itself is planned.
