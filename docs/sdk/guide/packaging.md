# Packaging and installing

## The plugin folder

Deckly installs a plugin as a folder named by the plugin id.
`deckly-cli build` assembles it in `target/deckly/<plugin id>/`:

```text
com.example.hello/
├── deckly-plugin.json                          the manifest
├── bin/x86_64-unknown-linux-gnu/hello-deckly   the program (.exe on Windows)
└── assets/
    ├── plugin.svg          Plugin::icon, or found by its name
    ├── wave.svg            Action::ICON, or found by its name
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

`deckly-cli build` runs it for you after every build. `--describe` also
checks the manifest as Deckly will and fails on one Deckly would refuse.
Every field is in the [manifest reference](../reference/manifest.md).

## Icons and images

| Where | Set with | Formats |
| --- | --- | --- |
| The plugin in the Plugins tab | `Plugin::icon("assets/plugin.svg")` | PNG, SVG |
| An action in the action list | `const ICON: &'static str = "assets/wave.svg"` | PNG, SVG |
| A state's image | `State { image: "assets/on.svg".into(), .. }` | PNG, SVG |
| An icon on a face | `Face::new().icon("assets/face.svg")` | PNG, JPEG, SVG |

Put the files in the project's `assets/` folder; `build` copies it. When
the code names no icon, `build` takes `assets/plugin.svg` for the plugin and
`assets/<action>.svg` for an action with that short id (PNG works too).
SVG stays sharp on every key size. Use your own artwork: do not copy icons
from other apps or plugins.

## Version

`plugin!` takes the version, authors, licence and description from
`Cargo.toml`. A plugin built with the `Plugin` builder sets them the same
way, so there is one place to change them:

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
built for, and `build` puts it in a folder for that target:

```json
"programs": {
  "x86_64-unknown-linux-gnu": "bin/x86_64-unknown-linux-gnu/hello-deckly"
}
```

To ship one file for several systems, pack on each (CI runners for Linux,
Windows and macOS are the easy way), then join the files:

```sh
deckly-cli pack --merge hello-linux.decklyPlugin hello-windows.decklyPlugin hello-macos.decklyPlugin
```

The result lists every program:

```json
"programs": {
  "x86_64-unknown-linux-gnu": "bin/x86_64-unknown-linux-gnu/hello-deckly",
  "x86_64-pc-windows-msvc": "bin/x86_64-pc-windows-msvc/hello-deckly.exe",
  "aarch64-apple-darwin": "bin/aarch64-apple-darwin/hello-deckly"
}
```

The files must be the same plugin and version, built from the same code.
The workflow that `deckly-cli create` writes,
`.github/workflows/release.yml`, does all of this when a `v*` tag is
pushed and attaches the joined file to the GitHub release.

Deckly starts the program for its own target. When the exact target is
missing, it takes one for the same processor and system, so a `musl` build
also runs where Deckly is a `gnu` build. On a system with no program, the
plugin does not start and the Plugins tab says it "has no program for" that
target.

## The plugin file

`deckly-cli pack` builds with optimizations, runs
[`validate`](../reference/cli.md), and zips the plugin folder into
`target/deckly/<id>-<version>-<target>.decklyPlugin`. The file holds the
assembled folder as `<plugin id>/`, never the repository, so nothing else
from the project (its `.git`, `.env` or logs) can end up in it.

## Installing

- **Add File…** in the Plugins tab installs a `.decklyPlugin` file (or a
  Stream Deck plugin file), and so does dropping the file on the window.
  Deckly unpacks it into its [plugins folder](../reference/folders.md) and
  starts it. A newer file replaces the installed copy and restarts the
  plugin.
- **Add Folder…** installs a plugin folder the same way.
- **Restart** in a plugin's right-click menu restarts it, and **Uninstall**
  removes it.

A plugin catalog inside Deckly is planned. Until then, share the
`.decklyPlugin` file, for example attached to a GitHub release.

## While developing

`deckly-cli dev` (or `deckly-cli link`) does not install the plugin: it
links the project's plugin folder, and Deckly runs it from a fresh copy
each time it restarts, so a build can replace the program while the old one
runs, on every system. The Plugins tab marks such a plugin
**Development**, and **Unlink** in its menu removes the link. A linked
plugin wins over an installed copy with the same id. See
[deckly-cli](../reference/cli.md#what-dev-and-link-change-in-deckly).
