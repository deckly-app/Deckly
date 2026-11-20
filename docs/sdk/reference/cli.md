# deckly-cli

The plugin tool. Install it once:

```sh
cargo install deckly-cli
```

It is also installed as `cargo-deckly`, so `cargo deckly <command>` works
the same. `deckly-cli help <command>` prints a command's options.

| Command | Does |
| --- | --- |
| `create [name]` | Asks a few questions and makes a new project in `./<name>`: the plugin with an example action and its tests, icons, a README, a licence, a release workflow and a git repository; builds it and links it into Deckly. Alias: `new`. |
| `dev` | Builds the plugin, links it and restarts it in the running Deckly; then builds and restarts it on every save of `src/`, `assets/` or `Cargo.toml`, and prints its log. A failed build leaves the running plugin alone. |
| `build` | Builds the program and assembles the plugin folder `target/deckly/<plugin id>/`. |
| `link` | Builds the plugin and links its folder into Deckly as a development plugin. |
| `unlink [id]` | Removes the link: this project's plugin, or the one with that id. |
| `list` | Lists the linked plugins and their folders. |
| `validate` | Builds, checks the plugin folder as Deckly will, then starts the plugin once as Deckly does. `--no-start` only checks. |
| `pack` | Builds with optimizations, validates, and writes `target/deckly/<id>-<version>-<target>.decklyPlugin`. `-o <file>` writes it elsewhere. |
| `pack --merge <files>...` | Joins files of one plugin and version, packed on different systems, into one with every system's program. |
| `log [id]` | Prints the plugin's log; `-f` keeps printing new lines. |

Every command except `create` works in the plugin's folder or any folder
inside it. In a Cargo workspace with several plugins, pick one with
`-p <package>`. `--release` builds with optimizations, and
`--config-dir <folder>` names Deckly's settings folder (otherwise
`DECKLY_CONFIG_DIR`, else the [usual folder](folders.md) of the system).

## create

| Flag | Answers |
| --- | --- |
| `--author <name>` | Author; default `git config user.name`, else the login name. |
| `--id <id>` | Plugin id, reverse-DNS; default `com.<author>.<name>`. |
| `--description <text>` | One line about the plugin. |
| `--example counter\|minimal` | The example action. |
| `--license mit\|apache-2.0\|mit-or-apache-2.0\|none` | The licence. |
| `--dir <folder>` | Where to make the project folder; default the current folder. |
| `-y`, `--yes` | Take the defaults for every question without a flag. |
| `--no-git`, `--no-ci`, `--no-build`, `--no-link` | Skip the git repository, the check and release workflows, the build, or the link. |
| `--sdk-path <folder>` | Use `deckly-sdk` from a local folder (also `DECKLY_SDK_PATH`): the SDK crate or a Deckly checkout. |
| `--sdk-git <url>`, `--sdk-branch <branch>` | Use `deckly-sdk` from a git repository (also `DECKLY_SDK_GIT`). |

Without a terminal (in a script or CI) `create` never waits for an answer:
without `--yes` it stops and names the flags it is missing. A folder that
exists and is not empty stops it before anything is written. Without
`cargo` it still writes the project and says what to install.

## What dev and link change in Deckly

`link` writes the plugin's folder into `dev-plugins.json` in Deckly's
settings folder. Deckly runs a linked plugin from a fresh copy in
`dev-run/<plugin id>/`, shows it in the Plugins tab with a **Development**
mark and an **Unlink** entry, and prefers it over an installed copy with the
same id. To restart it after a build, the tool sends one line to the local
port the running Deckly listens on, with the token Deckly keeps in
`instance.token`, a file only you can read. If Deckly is not running, the
tool says so and the plugin starts with Deckly.

Ctrl+C stops `dev` and leaves the link, so the plugin keeps working in
Deckly; `deckly-cli unlink` (or **Unlink** in the Plugins tab) removes it.

## The plugin folder it builds

```text
target/deckly/<plugin id>/
├── deckly-plugin.json          from the program's --describe, written last
├── bin/<target>/<program>      the program, one folder per system
└── assets/                     a copy of the project's assets/
```

When the code names no icon, `build` uses `assets/plugin.svg` (or `.png`)
for the plugin and `assets/<action>.svg` for an action whose short id is
`<action>`.
