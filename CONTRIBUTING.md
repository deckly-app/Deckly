# Contributing to Deckly

Short guide. The full project rules are in [AGENTS.md](AGENTS.md): read them
before your first change.

## Setup

- Install the stable Rust toolchain (`rustup`).
- Close the official Stream Deck app before running Deckly: both apps cannot
  drive the same device at once.

## Run

Development build (fast to compile, not optimized):

```sh
cargo dev
```

Release build (slower to compile, runs like the shipped app; use it to check
speed, since egui can feel slow in a debug build):

```sh
cargo dev --release
```

Run these commands from the repository root. The Rust `xtask` launcher first
builds the whole workspace, including `deckly` and its separate `deckly-web`
helper, and starts the editor only if the build succeeds. Plain `cargo run`
and `cargo run --release` from the root use the same launcher.

Close existing Deckly instances before rebuilding. Windows locks running
executables, so an old editor or helper can prevent a successful build.

`cargo run -p deckly` is a direct editor launch: Cargo builds only that
package and its dependencies, so it can leave `deckly-web` out of date.
Use the developer launcher for normal development. To build everything
without opening windows or starting plugins:

```sh
cargo dev --no-run
```

To use this checkout as your installed Deckly (Linux and macOS):

```sh
cargo dev install
```

Builds the release binaries (only what changed is recompiled) and installs
them where the install script puts a release, replacing that install in
place: on Linux `~/.local/lib/deckly` with the `deckly` link in
`~/.local/bin`, the menu entry and the icon; on macOS
`~/Applications/Deckly.app`. It then installs `deckly-cli` and
`cargo-deckly` like `cargo dev install-cli --no-check`. `install.sh
--uninstall` removes it as usual.

- `--debug` installs debug binaries, `--no-cli` leaves the plugin tool
  alone, `--prefix <dir>` installs elsewhere (Linux: the prefix, default
  `DECKLY_PREFIX` or `~/.local`; macOS: the folder for `Deckly.app`).
- The app shows "Local build <commit>" at the bottom of Preferences, General
  (`-dirty` with uncommitted changes) and never updates itself to a release;
  run the install script to go back to releases.
- A running Deckly keeps the old build until you quit it from the tray and
  start it again; the command says when one is running.
- Linux device rules are never installed with sudo here: when they are
  missing or differ from the checkout, the command prints the command to run.
- Windows is not supported yet: it installs through the setup program, so use
  `cargo dev dist --setup` and run the setup it writes.

The launcher does not run the test suite automatically; use the checks below
before pushing. The web engine stays in the helper process; the launcher
adds no web dependencies to the editor.

Development switches:

- `DECKLY_NO_DEVICES=1`: open the editor without touching any device.
- `DECKLY_DEBUG=1`: print device events and action results.
- `DECKLY_INPUT=wayland|portal|x11|uinput`: force one way of pressing keys on
  Linux.
- `DECKLY_WINE=/path/to/wine`: force the Wine that runs Windows plugins.
- `DECKLY_FAKE_UPDATE=0.2.0`: pretend that version is the latest release, to
  see the update offer. Debug builds check for updates only with this set.
- `DECKLY_FAKE_NO_FFMPEG=1`: ignore FFmpeg on PATH and in the winget and
  Homebrew folders, to see the "Turn on video import" window. A chosen FFmpeg
  and Deckly's own download still count; with `DECKLY_CONFIG_DIR` set, the
  download goes into that folder.

To check simulated input on a Linux session by hand, focus a text field and
run `cargo test -p deckly -- --ignored typing`.

Set them per shell. PowerShell:

```powershell
$env:DECKLY_NO_DEVICES = '1'
cargo dev
```

Bash: `DECKLY_NO_DEVICES=1 cargo dev`.

## Plugin SDK and CLI

`deckly-sdk`, `deckly-plugin-protocol` and `deckly-cli` change often and are
not on crates.io yet. To work on them and try them from this checkout:

```sh
cargo dev sdk
```

Checks the plugin crates and the example plugins: format, clippy with
warnings as errors, and tests. `cargo dev sdk --fix` formats instead of only
checking.

```sh
cargo dev install-cli
```

Runs the same checks, then installs `deckly-cli` and `cargo-deckly` from this
checkout into Cargo's bin folder, replacing an installed copy. Nothing is
installed when a check fails. `--no-check` skips the checks, `--debug`
installs a debug build, which compiles faster. Run it again after each
change to the CLI.

A plugin takes `deckly-sdk` from this checkout in one of these ways:

- New project: `deckly-cli create --sdk-path <checkout>`, or set
  `DECKLY_SDK_PATH=<checkout>/crates/deckly-sdk` once per shell.
- Every project on this computer, including ones that ask for
  `deckly-sdk = "0.0.1"`: add to `~/.cargo/config.toml`

  ```toml
  [patch.crates-io]
  deckly-sdk = { path = "/path/to/Deckly/crates/deckly-sdk" }
  deckly-plugin-protocol = { path = "/path/to/Deckly/crates/deckly-plugin-protocol" }
  ```

  Remove it to go back to the published crates. Cargo warns that the patch
  is unused in projects that do not depend on the SDK, and in this
  repository it writes `[[patch.unused]]` entries into `Cargo.lock`. Never
  commit those: CI has no such patch, and `--locked` fails on them.

## Check before you push

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. For UI changes, also run the app and look at the result.
For platform-specific code, compile-check Windows, Linux, and macOS when you
can, and say which platforms you did not test.

## Making a change

1. Read the relevant code and docs first. Keep the change small and focused.
2. Add tests for anything testable without hardware (protocol encoding, model
   validation, serialization round-trips).
3. Fix a crash together with a regression test.
4. Write one commit per logical change, with a short imperative message
   ("Add folders to Navigation actions").

## Rules to remember

- Rust only; UI with egui. No JavaScript, webview, or Electron in the app.
- No `unwrap`, `expect`, or `panic!` on data from devices, files, plugins, or
  users. A disconnected device or a corrupt profile must not crash the app.
- Everything user-visible is an action with a stable UUID.
- Icons: SVG or egui-phosphor only, never Unicode or emoji.
- Clean-room: do not copy code, assets, or text from the official app or other
  Stream Deck libraries. Log observations in `docs/clean-room/`.
- Do not claim support for a device or OS until it is tested and listed in
  [docs/devices.md](docs/devices.md).
- Do not imply Deckly is made or endorsed by Elgato or Corsair.

## License

By contributing you agree that your work is released under the MIT license.
