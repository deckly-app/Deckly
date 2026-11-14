# Deckly plugin tool: proposal

Status: proposal, 2026-11-14. Nothing here is built. Evidence and sources are
in [the clean-room entry](clean-room/2026-11-14-stream-deck-cli.md); the SDK
itself is in [plugin-sdk.md](plugin-sdk.md) and the author's book in
`docs/sdk` (branch `docs/sdk-book`). Items that need the owner are marked
**Decision**.

## 1. What makes the official flow pleasant

Three things, and nothing else dominates:

1. **One command to a working project.** A short wizard, then a ready folder
   with an example, built and installed in the app.
2. **One command for the loop.** A watch mode rebuilds on save and restarts
   the plugin in the running app. The author never installs by hand again.
3. **One file to share.** `pack` validates and makes a single installer file.

Our current flow has none of the three: `cargo new`, edit `Cargo.toml`, build,
`mkdir`/`cp`/`--describe` (different commands on Windows), **Add Folder…**,
and again for every change (`docs/sdk/getting-started.md`, steps 2 to 5).

The official tool also has a hole we must not copy: on Linux it fails in
`create` (the owner's run), because it looks in a Windows/macOS folder. Every
command here must work on Windows, Linux and macOS, and a missing Deckly
folder is never an error.

## 2. The tool

One crate, `deckly-cli`, installed as a cargo subcommand:

```sh
cargo install deckly-cli      # gives: cargo deckly ...
cargo deckly new
```

(Before the crates are published: `cargo install --git
https://github.com/deckly-app/Deckly deckly-cli`. Prebuilt binaries on the
GitHub release and `cargo binstall` are a cheap addition.)

Why a cargo subcommand: plugin authors have Rust and cargo by definition, so
there is nothing else to install, no Node-style global package, and no clash
with the app's `deckly` binary. **Decision:** the name. AGENTS.md reserves
`deckly-cli` for "headless control and inspection"; this proposal gives the
crate the plugin tooling and leaves headless control for later as extra
subcommands of the same tool (or a second crate then).

| Command | Does |
| --- | --- |
| `new [name]` | The wizard (section 3). Alias `create`. |
| `dev` | `build` + `link` + rebuild on every save + follow the plugin's log. The everyday command. |
| `build [--release]` | `cargo build`, then assemble the plugin folder `target/deckly/<id>/` (program, `deckly-plugin.json` from `--describe`, `assets/`). Used by `dev`, `pack` and CI. |
| `link` / `unlink` / `list` | Show this plugin's folder to the local Deckly, remove it, list links. |
| `validate` | Check the assembled folder, then start the program against a fake host (section 6). |
| `pack` | `validate`, then `target/deckly/<id>-<version>-<target>.decklyPlugin`. `pack --merge a b ...` joins per-system files into one. |
| `log [-f]` | Print or follow the plugin's log, wherever Deckly keeps it. |

All commands take the plugin from the current directory (any folder inside it)
and print plain lines that work in a non-interactive terminal.

## 3. `cargo deckly new`

### Questions

Same order of work as the official wizard (a few questions, then a summary
and a yes/no), with defaults so Enter-Enter-Enter gives a valid plugin:

| # | Question | Default | Check |
| --- | --- | --- | --- |
| 1 | Author | `git config user.name`, else the login name | not empty |
| 2 | Plugin name (shown in Deckly) | none | 1 to 64 characters |
| 3 | Plugin id | `com.<author>.<name>` in lowercase, spaces to `-` | the host's own id rule (the code in `deckly-plugin-protocol`, so the wizard and Deckly cannot disagree) |
| 4 | Description | `<Name> for Deckly.` | at most 2000 characters |
| 5 | Example | `Counter` (a key with a setting and a face); the other choice is `Minimal` (one key press) | choice |
| 6 | Licence | `MIT` | choice: MIT, Apache-2.0, MIT or Apache-2.0, none |

Then a summary (folder, crate name, id) and "Create?" (default yes). The
folder is `./<name-as-crate-name>`; an existing non-empty folder is an error
that says so, before anything is written.

Rules from the observation:

- Everything the wizard asks can also be given as a flag (`--author`,
  `--id`, `--example`, ...), and `--yes` takes every default. Without a
  terminal and without flags it stops and lists the missing flags; it never
  waits on a prompt that nobody sees. This makes the generator testable in CI.
- The id is checked by rule only. A clash with an installed plugin is a
  warning, never an error, and a missing plugins folder is silently fine.
- After the summary, switches (all on): `--no-git`, `--no-ci`, `--no-build`,
  `--no-link`.

### What it does, and prints

Steps printed one per line with a mark and a time: create the files, `git
init` and the first commit, `cargo build` (the first build downloads and
compiles dependencies; the line says so), link into Deckly. Then the next
steps in three lines: `cd <folder>`, `cargo deckly dev`, and "in Deckly:
Plugins tab, drag *Name → Counter* onto a key". If Deckly is not installed or
not running the link still succeeds; the message says the plugin appears the
next time Deckly starts.

`git` is used only through the `git` command when it is on the PATH; when it
is missing or has no user name set, the files are still made and the line says
what was skipped.

### Files

```text
<folder>/
├── Cargo.toml                  package, deckly-sdk and serde, print lint (section 7)
├── src/main.rs                 the plugin: one action, its form, its tests
├── assets/
│   ├── plugin.svg              plugin icon (Deckly's own art, ours to ship)
│   └── counter.svg             action icon
├── .gitignore                  /target
├── LICENSE
├── README.md                   dev, test, pack, release, in five short lines each
└── .github/workflows/release.yml
```

The CI workflow builds and packs on Linux, Windows and macOS (matrix), joins
the files with `pack --merge`, and attaches one `.decklyPlugin` to the GitHub
release when a `v*` tag is pushed. It is what `plugin-sdk.md` already promises
for "Plugin repositories".

The templates are text files inside the `deckly-cli` crate, written by us,
under the project's MIT licence. The generated `Cargo.toml` names the SDK
version the tool was released with (`deckly-sdk = "0.1"`; a git dependency
until the crates are published).

## 4. `dev`: how a running Deckly reloads a plugin

Elgato's loop is "the bundler rebuilds, then the CLI tells the app to restart
the plugin". We need the same, and the app today has no way to be told
anything: no control channel, no folder watching, and `Host::rescan` and
`Host::restart` are called only from the UI (`crates/deckly/src/app.rs`).

Proposal, all in the app and host, none in the tool except writing files:

1. **A link file.** `<config>/dev-plugins.json` lists `{ id: absolute folder }`.
   `link` adds the entry, `unlink` removes it; both write the file through a
   temporary name and a rename. No symlinks: they need a privilege on Windows.
2. **Host.** `rescan` also reads the link file and adds each linked folder as a
   plugin marked "development". A linked folder that is gone shows as failed
   with "the source folder is missing; unlink it", not as a crash.
3. **Run from a copy.** A development plugin is started from a fresh copy of
   its folder under `<config>/dev-run/<id>/`, not from the source. The build
   can then overwrite the program while the old one still runs. This removes
   the "Text file busy" problem the book describes for Linux and the locked
   `.exe` on Windows, so no platform needs a special rule.
4. **Trigger.** While at least one link exists, the app watches the link file
   and each linked folder's `deckly-plugin.json` and program (debounced about
   300 ms) and calls `Host::restart` (or `rescan` when the manifest changed).
   With no links nothing is watched, so idle cost for normal users stays zero.
   `build` writes the program first and the manifest last, so a restart never
   sees a new manifest with an old program.
5. **Plugins tab.** A development plugin shows a small "development" mark and
   its source folder, and "Unlink" instead of "Uninstall". **Restart** and
   **Open Log** work as now.

`dev` itself is then plain: build, assemble, link; watch `src/`, `assets/`,
`Cargo.toml` (debounced); on a change run `build` again; print cargo's errors
and keep the old plugin running when the build fails; print new lines of the
plugin's log file as they appear (Deckly already writes `logs/plugins/<id>.log`
with standard error and its own start/stop/protocol notes). Ctrl+C stops the
tool and leaves the link, so the plugin stays usable; `unlink` removes it.

Measured, not promised: on this Linux machine (16 cores, rustc 1.99.1) a clean
debug build of `examples/counter-plugin` with its dependencies took 2.7 s, and
a rebuild after changing one line took 0.1 s. Windows and macOS were not
measured. Compile time is the one place Rust cannot match a JavaScript
bundler; `dev` uses debug builds by default and `--release` is a flag.

**Decision:** the trigger. Alternatives:

- A local control socket in the app (the tool says "restart X" like the
  official `restart` command). Needs a Unix socket on Linux/macOS and a named
  pipe on Windows (a crate or FFI), a port or path to find, and a place to
  secure it; and it works only while the app is running. Not recommended.
- Doing without a watcher: the tool copies into the plugins folder and the
  author presses **Restart**. This is today's flow with less typing; it is a
  fallback, not the goal.

The watcher means one new dependency in the app, `notify` (section 5).

## 5. Dependencies and AGENTS.md

AGENTS.md asks for pure-Rust dependencies and a small tree, with OS libraries
only for device access and desktop integration. Counted with `cargo tree -e
normal` on the three targets (distinct crates, not counting ours):

| Need | Crate | Crates added | Notes |
| --- | --- | --- | --- |
| Prompts for the wizard | `dialoguer`, default features off | 5 (Linux, macOS) to 8 (Windows): `console`, `libc`, `shell-words`, `unicode-width`, plus Windows bindings | Input, Select, Confirm with validators. Recommended. |
| Same, nicer | `inquire` | 26 to 32 | Better validation display and autocomplete, bigger. |
| Full-screen TUI | `ratatui` | 62 to 69 | Not needed: the official wizard is prompts, not a screen. Not recommended. |
| File watching (`dev`, and the app's trigger) | `notify` | 9 to 10 | inotify, FSEvents and ReadDirectoryChangesW behind one API; pure Rust over OS bindings, which is the desktop-integration case AGENTS.md allows. |
| Arguments | `lexopt` (or by hand) | 1 | `clap` is bigger; seven commands do not need it. |
| `.decklyPlugin` zip | `zip` | 0 new | Already in the tree through the host. |
| JSON, versions | `serde`, `serde_json`, `semver` | 0 new | Already in the workspace. |
| Manifest and id rules | `deckly-plugin-protocol` | 0 new | Path dependency; no `deckly-core`, no app. |

The tool never links the SDK, the host or the app. `git` is run as a command,
not through a library (no `libgit2`). Total for the tool: about 15 to 20 new
crates, none that is not Rust; the app gains only `notify`. A "TUI" in the
sense of one screen is not worth 60 crates; if the owner wants more than
prompts, that is a **Decision**.

Layering for AGENTS.md section 3: `deckly-cli` is a tool beside L2; it depends
on `deckly-plugin-protocol` (L1) only. Add the row when the work starts.

Where Deckly keeps its folders (`%APPDATA%\Deckly`, `~/Library/Application
Support/Deckly`, `$XDG_CONFIG_HOME/deckly`, `DECKLY_CONFIG_DIR`) the tool
computes itself, with the same rules and a test against the table in
`docs/sdk/reference/folders.md`, so the tool and the app cannot drift apart
unnoticed. Program names use `EXE_SUFFIX`, the executable bit is set on Unix only,
the target is read from `rustc -vV`.

## 6. `validate` and `pack`

`validate` checks, on the assembled folder:

- the manifest passes the host's own checks (same code, so a plugin that
  validates installs);
- every path in it (icons, programs) exists, stays inside the folder, and the
  program is executable on Unix;
- every action id starts with the plugin id; every form key is a field of the
  action's settings (typos);
- **smoke start**: the tool runs the program as Deckly would, sends `hello`,
  and expects `ready` within a few seconds, then closes the input and expects
  the program to exit. This catches the commonest author mistakes (a
  `println!`, slow work before `run()`, a plugin that ignores the closed pipe)
  before Deckly shows a "restarting" plugin. It needs only the protocol crate.

`pack` runs `validate`, then zips the assembled folder (not the repository), so
`.git`, `.env` and logs cannot leak and no ignore file is needed. A single
`pack` holds the programs for the system it ran on. In CI each system packs,
and `pack --merge` joins the per-system files into one with one `programs`
table, the thing the book says to edit by hand today. The app must learn the
format: `install.rs` already unpacks `.streamDeckPlugin` zips and Deckly
folders, so `.decklyPlugin` is the same zip with one `<id>/` folder; **Add
Folder…** gains **Add Plugin File…**, and a double click can come later.

## 7. SDK changes that make plugins shorter

The `hello-deckly` example is about 60 lines (without its tests) for a key
that greets and counts. In
order of value to the author:

1. **`Action::face(&self, key) -> Face`, drawn by the SDK.** Today every
   author writes a `draw` helper and calls it from `key_appear`,
   `key_down` and `settings_changed`. Instead the SDK calls `face` after the
   key appears, after settings change and after each handler or tick, and
   sends the result only if it differs from the last one (this is also the
   AGENTS.md rule "re-send only when changed"). `set_face` stays for async
   updates. The example loses two handlers and the helper; it becomes state
   plus one function that turns state into a face. Small to build (a compare
   after dispatch). Backward compatible: the default `face` returns nothing.
2. **Settings read tolerant of missing fields.** Overlay the saved JSON on the
   type's `Default` before deserializing. Authors then do not need
   `#[serde(default)]` on every settings struct, and one wrong field no longer
   resets all the others (today a mismatch reads as `Default` entirely).
3. **Short action ids.** `const ID = "wave"` means `<plugin id>.wave`; a full
   id is still accepted. The id is written once and cannot be mistyped against
   the plugin id. **Decision:** profiles store the full id, so this changes
   nothing stored, but it changes what the book teaches; do it before 1.0.
4. **One-line `main`.** A macro, `deckly_sdk::plugin!("com.example.hello",
   "Hello", [Wave])`, that builds the `Plugin` from the id and name plus
   `CARGO_PKG_VERSION`, authors, licence and description from `Cargo.toml`
   (so the wizard's answers live in one place), defines `fn plugin()` for the
   tests and `main`. The builder stays for anything unusual.
5. **Icons by convention.** `assets/plugin.svg` and `assets/<action>.svg` are
   picked up by `build` when the code names none.
6. **No `println!` by accident.** The generated `Cargo.toml` sets
   `[lints.clippy] print_stdout = "deny"`; this was tried on a scratch crate
   and `cargo clippy` refuses `println!` with a clear error. The SDK cannot
   stop it at run time without FFI, so the lint plus the smoke start in
   `validate` are the guard.
7. **Typo check in tests.** `Plugin::check` (also run by `--describe`) also
   verifies that every form field key exists in the settings type, by
   serializing `Default`.
8. Later: `deckly_sdk::testing` helpers such as `assert_face`, and plugin-wide
   state in the API (already on the list in `plugin-sdk.md`).

With 1, 2, 3 and 4 the hello plugin should come to about 30 lines (an
estimate, not measured).

## 8. Ranked by value to a plugin author

| # | Item | Why | Size |
| --- | --- | --- | --- |
| 1 | `new` that ends with a built, linked, committed project | Time to a first reacting key drops from a page of steps to one command | medium (wizard, templates, `build`) |
| 2 | `dev` with watch, restart and log (needs app side: links, run-from-copy, watcher) | The loop is what authors repeat hundreds of times | medium-large |
| 3 | SDK `face()`, tolerant settings, short ids, `plugin!` | Shorter plugins, fewer mistakes, fewer concepts to learn | small each |
| 4 | `build` that replaces the manual folder steps (also in CI) | Removes the platform-specific block in the book | small |
| 5 | `pack` and `--merge`, release workflow in the template, `.decklyPlugin` install | One file to share, one tag to release | medium |
| 6 | `validate` with the smoke start | Catches the common mistakes early | small-medium |
| 7 | `print_stdout` lint, icons by convention, typo check | Cheap guards | small |
| 8 | `log`, `doctor` (is Deckly running, where are its folders), `publish` to the planned registry | Later | |

## 9. Decisions for the owner

1. Tool name and shape: `cargo deckly` from crate `deckly-cli` (recommended),
   or a standalone binary; and what AGENTS.md's planned `deckly-cli` row means.
2. Publish `deckly-sdk` and `deckly-plugin-protocol` to crates.io (needed for
   `cargo install` of templates that just work); until then a git dependency.
3. Reload trigger: file watcher in the app (recommended) or a control socket;
   and the run-from-a-copy rule for development plugins.
4. Dependencies: `dialoguer` (recommended) or `inquire`; `notify` in the app
   and the tool; `lexopt` or hand-written arguments. Prompts only, no
   full-screen TUI.
5. SDK changes 1 to 4 before 1.0, in particular short ids.
6. `.decklyPlugin` as the package format, and the **Add Plugin File…** entry.
7. Templates: Counter and Minimal; CI workflow on by default; licences offered.

## 10. Suggested order of work

1. SDK changes (section 7, items 1 to 4), then update the book and examples,
   so the templates are written once against the final API.
2. Host and app: `.decklyPlugin` install; link file, run-from-copy, watcher,
   Plugins tab marks.
3. `deckly-cli`: `build`, `link`, `dev`, `validate`, `pack`.
4. `new` with templates and the CI workflow.
5. Rewrite `getting-started.md` around `cargo deckly new` and `dev`; keep the
   manual steps as "what the tool does for you".

Tests: the wizard's generator runs in CI on all three systems (generate with
`--yes`, `cargo test`, `build`, `validate`, `pack`), and `dev` is tested with
a fake watcher event; the app side with the existing host test pattern in
`examples/counter-plugin/tests/host.rs`. Compile checks and real runs are
reported separately per system, as AGENTS.md requires.
