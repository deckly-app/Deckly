# Deckly plugin tool: proposal

Status: built on branch `feature/plugin-cli` (2026-11-14), with the owner's
decisions of the same day: a separate `deckly-cli` (also `cargo-deckly`),
prompts with `inquire`, all four SDK changes of section 7, the restart over
the instance port with a token, run from a copy, the `dev-plugins.json`
link file, `.decklyPlugin` files, Counter and Minimal templates, the CI
release workflow, `notify` and `lexopt`. The plugin author's guide is
`docs/sdk/getting-started.md` and `docs/sdk/reference/cli.md`; this page
stays as the research and design record. Evidence and sources are in
[the clean-room entry](clean-room/2026-11-14-stream-deck-cli.md).

Where the build differs from the proposal below:

- The wizard uses `inquire` (the owner's choice), not `dialoguer`.
- The control line is `deckly-control 1 <token> reload <id>` or `rescan`
  (`deckly_plugin_protocol::dev`); `reload` re-reads the link file and
  restarts the plugin from a fresh copy.
- `build` puts the program in `bin/<target>/` so packs of different systems
  merge without clashing file names (Linux and macOS programs have the same
  name).
- The Plugins tab entry is **Add File…**: "Add Plugin File…" did not fit
  beside **Add Folder…** in the side panel (seen in the running app).
- Deckly's installers do not carry `deckly-cli` yet; install it with
  `cargo install`.
- `Plugin::check` also finds form fields that are not settings fields
  (item 7 of section 7) and action ids outside the plugin id.
- A project made inside another Cargo workspace gets its own `[workspace]`
  table; without it Cargo refuses to build it.

## Install once, create anywhere

The owner's main requirement: install the tool once, then run `create` in any
folder, on Windows, Linux and macOS, like `npm install -g` and `streamdeck
create`. Elgato's tool fails on Linux in `create` because it reads a Windows
path; ours takes every path from the rules below and never from a constant.

### The command name

The app's binary is `deckly`, so there are two ways to get `... create`.
**Decision for the owner.**

| | A. The app does it: `deckly create` | B. A separate program: `deckly-cli create` (recommended) |
| --- | --- | --- |
| Install | Nothing extra for anyone who has Deckly | Shipped next to the app (below), or `cargo install` |
| Windows console | The release app is built as a windows-subsystem program (`crates/deckly/src/main.rs`, line 4), so a terminal gets no output from it. Fixing it means attaching a console with OS calls (FFI) | A normal console program; prompts and logs just work |
| `cargo install` | Would compile the whole egui/wgpu app (and on Linux need its system libraries) to get a command-line tool | Small crate (about 15 to 20 crates plus ours), no GUI libraries |
| Weight at start | The tool dispatches before the GUI starts, so little; but the file is the whole app | Small |
| Version match | Always matches the installed app | Matches if shipped together; a separate `cargo install` can drift, so the tool prints its own and the SDK version and warns when Deckly is older |
| Name | The best | Longer; `cargo deckly ...` works too, because the crate builds a second binary named `cargo-deckly` |

If B is chosen, the app can later forward `deckly create` to `deckly-cli`
when started from a console.

### The one install command

| Way | Command | For whom |
| --- | --- | --- |
| Deckly's installers (recommended for a newcomer) | The existing one-liners, `curl -fsSL .../install.sh \| sh` (Linux, macOS) and `irm .../install.ps1 \| iex` (Windows), plus Homebrew and the AUR package. The release archives and installers also carry `deckly-cli` | Anyone who installs Deckly (they must, to see their plugin) |
| `cargo install` (the same line on all three systems) | `cargo install --git https://github.com/deckly-app/Deckly deckly-cli` now; `cargo install deckly-cli` once the crates are published; `cargo binstall deckly-cli` for a prebuilt file | Rust authors, CI, a computer without a Stream Deck |

What each existing channel has to change (`docs/releasing.md`): the archives
add `deckly-cli`; `install.sh` and the Homebrew formula link it into the same
`bin` folder as `deckly`; the AUR `PKGBUILD` links it into `/usr/bin`; the
Windows setup program adds its folder to the user's PATH (it has no PATH entry
today, so a newly opened terminal would not find `deckly-cli`, and the
installer's own tests must check it). Nothing is signed today (the docs
say so), and the same warnings apply to the tool.

A newcomer without Rust needs two things either way: Deckly, and Rust from
rustup.rs, since a plugin is a Rust program. `deckly-cli new` checks for
`cargo` first and, when it is missing, still writes the project and says what
to install. So the steps are: install Deckly (one line), install Rust (one
line from rustup.rs), then `deckly-cli new` in any folder. A Rust author who
has Rust installs only the tool with `cargo install`. We recommend the
installers because every plugin author already needs Deckly itself.

### Where it finds Deckly's folders

The same rule as `config_dir()` in `crates/deckly/src/store.rs`, copied into
the tool (the tool cannot depend on the app) and tested against the table in
`docs/sdk/reference/folders.md`:

1. `DECKLY_CONFIG_DIR`, when set and not empty, is the folder.
2. Windows: `%APPDATA%\Deckly`.
3. macOS: `$HOME/Library/Application Support/Deckly`.
4. Linux and others: `$XDG_CONFIG_HOME/deckly`, else `$HOME/.config/deckly`.

Inside it: `plugins/` (installed plugins), `logs/plugins/<id>.log`, and the
files the tool adds (`dev-plugins.json`, section 4). If the variables that the
rule needs are missing, the tool says which one and accepts `--config-dir`;
it never falls back to a fixed path. A folder that does not exist yet is not an
error: `new` does not touch it at all, and `link` creates it (the app does the
same at start). To know whether Deckly is running the tool tries the same lock
file the app holds, `instance.lock` (the app's one-instance check), without
starting anything.

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

One crate, `deckly-cli`, with the binary `deckly-cli` (and a second name for
the same program, `cargo-deckly`, so `cargo deckly new` works too). How it is
installed and why it is a separate program from the app: see "Install once,
create anywhere" above. AGENTS.md reserves `deckly-cli` for "headless control
and inspection"; this proposal gives the crate the plugin tooling and leaves
headless control for later as extra subcommands of the same tool.

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

## 3. `deckly-cli new`

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
steps in three lines: `cd <folder>`, `deckly-cli dev`, and "in Deckly:
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
├── AGENTS.md                   instructions for AI coding agents (below)
├── CLAUDE.md                   `@AGENTS.md`, so Claude Code reads the same file
└── .github/workflows/
    ├── check.yml               fmt, clippy and tests on Linux, Windows and macOS
    └── release.yml
```

The release workflow builds and packs on Linux, Windows and macOS (matrix), joins
the files with `pack --merge`, and attaches one `.decklyPlugin` to the GitHub
release when a `v*` tag is pushed. It is what `plugin-sdk.md` already promises
for "Plugin repositories". The check workflow runs `cargo fmt --check`,
`cargo clippy -D warnings` and `cargo test` on the same three systems on
every push, so a change that breaks one system shows before a release.

`AGENTS.md` is for people who write the plugin with an AI coding agent. It
tells the agent to write every feature for Windows, macOS, Linux X11 and
Linux Wayland from the start (portable crates and paths, no shell commands,
per-system code behind one function, a clear "unsupported" only where the
system lacks the feature), and repeats the SDK rules that matter most: one
event thread, settings for state, no output on standard output, no panics
on outside data, harness tests, and a per-system report. It links the book
and `llms.txt` instead of copying the API. `CLAUDE.md` holds only
`@AGENTS.md`, so both kinds of agent read one text. `tests/agents.rs` checks
that the book pages and SDK calls it names still exist.

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
4. **Trigger.** After `build` the tool tells the running Deckly to restart
   the plugin, over a channel the app already has (see the decision below).
   `build` writes the program first and the manifest last, so a restart never
   sees a new manifest with an old program. If Deckly is not running the tool
   says so; the link file is read when Deckly starts.
5. **Plugins tab.** A development plugin shows a small "development" mark and
   its source folder, and "Unlink" instead of "Uninstall". **Restart** and
   **Open Log** work as now.

`dev` itself is then plain: build, assemble, link; watch `src/`, `assets/`,
`Cargo.toml` (debounced; this is the tool's own watcher, section 5); on a change
run `build` again, then ask Deckly to restart the plugin; print cargo's errors
and keep the old plugin running when the build fails; print new lines of the
plugin's log file as they appear (Deckly already writes `logs/plugins/<id>.log`
with standard error and its own start/stop/protocol notes). Ctrl+C stops the
tool and leaves the link, so the plugin stays usable; `unlink` removes it.

Measured, not promised: on this Linux machine (16 cores, rustc 1.99.1) a clean
debug build of `examples/counter-plugin` with its dependencies took 2.7 s, and
a rebuild after changing one line took 0.1 s. Windows and macOS were not
measured. Compile time is the one place Rust cannot match a JavaScript
bundler; `dev` uses debug builds by default and `--release` is a flag.

**Decision:** how the tool reaches the running Deckly.

- **(a) A message over the port the app already listens on (recommended).**
  `crates/deckly/src/os/instance.rs` keeps `instance.lock` (a lock held while
  Deckly runs) and `instance.port` (a localhost TCP port) in the config
  folder; a second start connects to the port, sending nothing, and the first
  Deckly shows its window. Extend it: after connecting, the tool may send one
  line, `restart <id>` or `rescan`, with a token that the app writes at start
  to a private file next to the port (mode 0600 on Unix, the user's own
  folder on Windows). A connection with no data still means "show window", so
  old and new copies keep working together. The tool learns "restarted" or
  "not running" (it can also test the lock). No new dependency in the app, the
  same on all three systems, exact instead of guessing from file events.
  Cost: a small message format, and care with the token, because restarting
  plugins is more than showing a window.
- **(b) A file watcher in the app** (`notify`), restarting when a linked
  folder's program or manifest changes. No message format and no token, but a
  dependency in the app, a debounce rule, and no answer back to the tool.
- Not recommended: a Unix socket plus a Windows named pipe (a crate or FFI,
  when a TCP port already exists), or no trigger at all (the author presses
  **Restart**, which is today's flow with less typing).

## 5. Dependencies and AGENTS.md

AGENTS.md asks for pure-Rust dependencies and a small tree, with OS libraries
only for device access and desktop integration. Counted with `cargo tree -e
normal` on the three targets (distinct crates, not counting ours):

| Need | Crate | Crates added | Notes |
| --- | --- | --- | --- |
| Prompts for the wizard | `dialoguer`, default features off | 5 (Linux, macOS) to 8 (Windows): `console`, `libc`, `shell-words`, `unicode-width`, plus Windows bindings | Input, Select, Confirm with validators. Recommended. |
| Same, nicer | `inquire` | 26 to 32 | Better validation display and autocomplete, bigger. |
| Full-screen TUI | `ratatui` | 62 to 69 | Not needed: the official wizard is prompts, not a screen. Not recommended. |
| File watching in `dev` (the tool only) | `notify` | 9 to 10 | inotify, FSEvents and ReadDirectoryChangesW behind one API; Rust over OS bindings. Without it `dev` could rebuild on a key press instead. |
| Arguments | `lexopt` (or by hand) | 1 | `clap` is bigger; seven commands do not need it. |
| `.decklyPlugin` zip | `zip` | 0 new | Already in the tree through the host. |
| JSON, versions | `serde`, `serde_json`, `semver` | 0 new | Already in the workspace. |
| Manifest and id rules | `deckly-plugin-protocol` | 0 new | Path dependency; no `deckly-core`, no app. |

The tool never links the SDK, the host or the app. `git` is run as a command,
not through a library (no `libgit2`). Total for the tool: about 15 to 20 new
crates, all Rust; with decision (a) above the app gains none (with (b) it
gains `notify`). A "TUI" in the sense of one screen is not worth 60 crates; if the owner wants more than
prompts, that is a **Decision**.

Layering for AGENTS.md section 3: `deckly-cli` is a tool beside L2; it depends
on `deckly-plugin-protocol` (L1) only. Add the row when the work starts.

Where Deckly keeps its folders is in "Install once, create anywhere" above.
Program names use `EXE_SUFFIX`, the executable bit is set on Unix only, the
target is read from `rustc -vV`.

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

1. Command name and shape: the separate binary `deckly-cli` (recommended)
   versus subcommands of the app's `deckly`; what AGENTS.md's planned
   `deckly-cli` row means.
2. How newcomers get the tool: shipped in the Deckly release archives and
   installers (recommended), with `cargo install` as the other path; a PATH
   entry in the Windows installer.
3. Publish `deckly-sdk`, `deckly-plugin-protocol` and `deckly-cli` to
   crates.io (needed for `cargo install` and for templates that just work);
   until then a git dependency.
4. How the tool reaches the running Deckly: a message over the existing
   instance port with a token (recommended), a file watcher in the app, and
   the run-from-a-copy rule for development plugins.
5. Dependencies: `dialoguer` (recommended) or `inquire`; `notify` in the tool;
   `lexopt` or hand-written arguments. Prompts only, no full-screen TUI.
6. SDK changes 1 to 4 before 1.0, in particular short ids.
7. `.decklyPlugin` as the package format, and the **Add Plugin File…** entry.
8. Templates: Counter and Minimal; CI workflow on by default; licences offered.

## 10. Suggested order of work

1. SDK changes (section 7, items 1 to 4), then update the book and examples,
   so the templates are written once against the final API.
2. Host and app: `.decklyPlugin` install; link file, run-from-copy, the
   restart message on the instance port, Plugins tab marks.
3. `deckly-cli`: `build`, `link`, `dev`, `validate`, `pack`.
4. `new` with templates and the CI workflow.
5. Rewrite `getting-started.md` around `deckly-cli new` and `dev`; keep the
   manual steps as "what the tool does for you".

Tests: the wizard's generator runs in CI on all three systems (generate with
`--yes`, `cargo test`, `build`, `validate`, `pack`), and `dev` is tested against a
fake Deckly that listens on a port; the app side with the existing host test pattern in
`examples/counter-plugin/tests/host.rs`. Compile checks and real runs are
reported separately per system, as AGENTS.md requires.
