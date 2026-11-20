# %%name_text%% — agent instructions

Read this file before changing the project. It applies to every human and AI
contributor. `CLAUDE.md` only points here; keep the instructions in this file.

This is a plugin for [Deckly](https://github.com/deckly-app/Deckly), written in
Rust with the Deckly plugin SDK (`deckly-sdk`). Plugin id: `%%id_text%%`.

## The SDK docs

Use the docs, not memory, for the SDK's API. They describe the SDK version
in `Cargo.toml`.

- Book: https://deckly-app.github.io/Deckly/
- For agents: https://deckly-app.github.io/Deckly/llms.txt (an index of
  Markdown pages) and https://deckly-app.github.io/Deckly/llms-full.txt
  (every page in one file)
- Most used pages: "Actions and keys", "Timers and threads", "Shared state
  and background events", "Settings", "Settings forms", "Global settings and
  secrets", "Testing", "Packaging and installing", "Logs and
  troubleshooting", "Limits".

## Windows, macOS and Linux from the start

The plugin ships one program per system, and users install it on all of
them. Write every feature for Windows, macOS, Linux on X11 and Linux on
Wayland from the first version, without waiting to be asked.

- Prefer pure-Rust crates that build on all three systems. Before adding a
  dependency, check that it supports Windows, macOS and Linux; avoid crates
  that need a C library or SDK installed on the user's computer.
- Build paths with `std::path::Path` / `PathBuf`, never with `/` or `\` in a
  string. Find user folders through a crate such as `dirs`, not hard-coded
  `~/...`, `%APPDATA%` or `/home/...`. Do not write files next to the
  plugin's program: Deckly may run it from a copy that is replaced.
- Do not shell out to system commands (`sh`, `bash`, `cmd`, `powershell`,
  `osascript`, `xdotool`, `notify-send`, ...) for things a crate or the
  system's API does. If running a program is the feature itself (for
  example "open this app"), handle each system explicitly.
- Keep code that differs per system in one small module with
  `#[cfg(target_os = "windows")]`, `#[cfg(target_os = "macos")]` and
  `#[cfg(target_os = "linux")]` branches behind one shared function. Every
  system gets a branch; there is no silent fallback.
- On Linux, X11 and Wayland are both first-class. Wayland lets a program
  neither read global input, inject keys or text, nor see or move other
  windows on its own. A feature that works only on X11 is not done for
  Linux.
- Hotkeys, typing text, the active window and similar desktop control are
  planned as Deckly host services (the manifest's `services`, empty for
  now). Until they exist, do not work around Wayland with `/dev/uinput`,
  root access or X11-only tricks.
- When a system really offers no way to do something, the action must say
  so instead of failing quietly: log the reason, call `key.show_alert()`,
  and make the settings form or README say it is unsupported there. Only
  use "unsupported" when the system lacks the feature, not because the code
  for it is more work.

## Writing actions

- All key events of the plugin run one after another on one thread. Never
  block a handler (network, files, processes, `sleep`, long loops): start a
  thread and send the result to the key from there. Timer ticks run on the
  same thread; keep them fast. See "Timers and threads".
- State that must survive a restart or a profile switch goes in the key's
  settings (`key.settings()`, `key.update_settings(...)`), or in global
  settings for the whole plugin. Fields on the action struct are lost when
  the key disappears.
- Settings must keep reading what older versions saved: give new fields a
  default, and keep old field names readable when renaming.
- Never change the plugin id or an action's `ID` after release; keys saved
  by users refer to them.
- Passwords and tokens go in secret form fields, which Deckly keeps in the
  system's secret store. Never log them or put them in titles.

## Errors and logs

- No `unwrap`, `expect`, `panic!` or unchecked indexing on data from the
  network, files, other programs, settings or the user. Return errors,
  log them and show `key.show_alert()`; the plugin keeps running.
- Standard output carries the protocol to Deckly. Never print to it
  with `println!` or `print!` (the `print_stdout` lint in `Cargo.toml`
  refuses it; keep that lint), and do not let a dependency print there.
  Log with `eprintln!`; standard error is the plugin's log, shown by
  `deckly-cli log` and Deckly's **Open Log**.
- Bound what you read from outside: sizes, counts, lengths and timeouts.

## Tests and checks

Every change that has logic comes with a test against the SDK's test
harness (`deckly_sdk::testing::Harness`), which plays Deckly without Deckly
or a device. Before finishing:

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
deckly-cli validate
```

Then check that the code compiles for the other systems, with the targets
installed through `rustup target add`:

```sh
cargo check --target x86_64-pc-windows-msvc
cargo check --target aarch64-apple-darwin
cargo check --target x86_64-unknown-linux-gnu
```

A target that cannot be checked here (a dependency needs that system's
SDK) is checked by CI: `.github/workflows/check.yml`, when present, builds
and tests on Linux, Windows and macOS on every push.

When reporting a change, say per system (Windows, macOS, Linux X11, Linux
Wayland) whether it was run, only compiled, or only reasoned about from the
code, and what does not work there and why. Never claim a system was
tested when it was not.

## Running and shipping

- `deckly-cli dev` builds, links into the Deckly on this computer and
  restarts the plugin on every save.
- `deckly-cli pack` makes a `.decklyPlugin` for this system.
- A `v*` tag runs `.github/workflows/release.yml` (when present), which
  packs on Linux, Windows and macOS and joins them into one file.
