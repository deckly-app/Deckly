# Changelog

All notable changes to Deckly. Newest first.

## 0.0.6 — Keys that ask first

Important keys can now ask for a deliberate gesture before they run, and
the Open action chooses the monitor and window state of the app it starts.

### Keys

- **Protect a key from accidental presses.** The shield at the top right of
  the key form (or **Protect…** in a key's right-click menu) makes an
  important key, such as one that shuts the computer down, ask for a
  deliberate gesture first. A brush of the key runs nothing.
  - **Hold to run** (recommended): hold the key for a set time (1 s by
    default, 0.3–5 s). A ring fills over the key; letting go early cancels.
  - **Press twice**: the first press arms the key and shows "Again" with a
    countdown; a second press within the window (3 s by default, 1–10 s)
    runs it. Any other key disarms it.
  - **Confirm screen**: the device asks Yes, No or Back in place of the
    page, over the page's wallpaper, and closes by itself after a set time
    (10 s by default, 3–30 s). **Shuffle button positions** places the
    answers anew each time.
  - Works for every kind of key: built-in actions, Multi Actions, folders
    and plugin actions. A plugin hears its key only once the gesture is
    complete. Editing the page, changing page, folder or profile, locking
    the computer or unplugging the device cancels a gesture. Protected keys
    show a shield in the editor.
- **Hotkeys record the whole combination.** Recording a Hotkey stopped at
  the first modifier pressed (Ctrl, Shift, Alt or Super); it now waits for
  the main key and keeps every modifier held with it.

### Open action

- **Choose where the app opens.** The Open action has new **Monitor**,
  **Window** and **Layout** settings. On Hyprland: any monitor, maximized
  or fullscreen, floating or tiled; this also works for apps started by a
  launcher that exits, such as Spotify. On Windows: any monitor, normal,
  maximized or minimized. An app that is already running keeps its window
  where it is. Other desktops do not support it yet and say so.

### Images

- **Transparent pictures.** New key pictures start transparent, as page
  icons already did: on a device without a wallpaper they look the same as
  black, and a wallpaper shows through. Unticking **Transparent** is kept
  for that picture.
- **Icons found by meaning.** The Icon Library also finds icons by what
  they mean ("shut down", "power off", "reboot", "mute", ...), and with an
  empty search it suggests icons from the key's title, settings and action.

### For plugin authors

- **deckly-sdk and deckly-cli 0.0.2.** `pack` keeps programs' file modes on
  Windows, and a shared-state update in a key handler redraws the keys
  before the next event. New projects use `deckly-sdk` 0.0.2.

### For contributors

- **`cargo dev install`** builds this checkout and installs it where a
  release goes, replacing that install; such a build says so in
  Preferences and never updates itself to a release.
- **Simpler branches.** There is no `dev` branch any more: task branches go
  into `main` after the owner's approval, the changelog is written when
  releasing, and publishing a release updates the Homebrew tap by itself.

## 0.0.5 — Plugins in Rust

Deckly now has its own plugin system: write a plugin in Rust, start it with
one command, and set it up from Deckly's own settings forms. Stream Deck SDK
plugins keep running as before.

### Deckly plugins

- **Deckly plugins.** A new kind of plugin, written in Rust with the
  `deckly-sdk` crate. Deckly runs them over pipes instead of a WebSocket.
  A plugin can draw a key's face (title, icon, colours, marks) without
  sending images, and keep plugin-wide state that redraws its keys.
- **Settings without a web view.** A plugin describes its fields and Deckly
  draws the form: one per action in the key editor and one per plugin in the
  Plugins tab, with choices the plugin fills in and buttons it answers.
- **Secrets stay secret.** Passwords and tokens a plugin asks for go to the
  system's secret store (Credential Manager, the Keychain, the Secret
  Service) or a private file, never into a profile, an export or a backup.
- **Install from a file.** Plugins tab > **Add File…**, or drop the file on
  the window, installs a `.decklyPlugin`, `.streamDeckPlugin` or `.zip`.
- **Development plugins.** A plugin linked from its project folder is marked
  "Development" and restarts from a fresh build.

### For plugin authors

- **On crates.io.** `deckly-sdk`, `deckly-cli` and `deckly-plugin-protocol`
  0.0.1 are published; install the tool with `cargo install deckly-cli`.
- **`deckly-cli`** (also `cargo deckly`): `create` starts a new plugin from
  a template (Counter or Minimal), then `build`, `dev`, `link`, `validate`,
  `pack` and `log`.
- **The Plugin SDK book**, with getting started, guides and a reference, at
  <https://deckly-app.github.io/Deckly/>.

### Fixes

- The key settings sit in the centre of the panel.
- Long plugin settings forms scroll.
- Recording a hotkey takes the whole combination: pressing Ctrl, Shift and
  R one after another records Ctrl+Shift+R, not just the first Ctrl.

### Install

- **Homebrew tap moved.** Install with `brew install deckly-app/tap/deckly`.
  Macs that tapped `auuruum/tap` keep getting updates.
- **Windows setup in one click.** The README links straight to
  `deckly-setup-windows-x64.exe`; PowerShell is no longer the only way shown.
- The Windows installer names Deckly as the publisher.

## 0.0.4 — Make it yours

Wallpapers that follow you everywhere, feedback marks in your own style, and
video import that no longer sends you hunting for FFmpeg.

### Look and style

- **Global wallpaper.** Set one picture for every profile, page and folder in
  Preferences > Style. A profile, page or folder can still use its own image or
  opt out; the nearest setting wins.
- **Feedback marks in your style.** The failure mark, the success check and the
  folder auto-exit ring can be hidden or restyled: icon, color, size and
  duration, with live previews and a Reset button. The device and the editor
  both draw them your way.
- **Backups carry it all.** The global wallpaper and the mark styles travel
  with a full backup.
- **Cleaner previews.** The editor shows one preview entry per key layout.

### Video import

- **No more FFmpeg hunt.** When FFmpeg 7+ is missing, a "Turn on video import"
  window opens (also under Preferences > General > Video Import). It installs
  through your system's package manager, copies the command for you, downloads
  a pinned build into Deckly's own folder, or uses an `ffmpeg` you pick.

### Fixes

- The update banner no longer covers the installed version with its buttons.

### Notes

- macOS has no pinned FFmpeg download yet; use Homebrew or pick your own.
- Builds are not code-signed, so Gatekeeper and SmartScreen may warn on first
  start.

## 0.0.3 and earlier

See the [GitHub releases](https://github.com/deckly-app/Deckly/releases).
