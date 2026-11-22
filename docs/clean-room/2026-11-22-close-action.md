# Close action

Input to the Close action (`com.deckly.system.close`): `actions.rs`,
`os/close.rs` and its platform modules, and the inspector in `app.rs`.

- Date: 2026-11-22.
- Sources and method:
  - Web searches over Elgato's public pages: the 6.2 release notes on
    edge.elgato.com (read as a web page), the help center articles on System
    actions and on the 6.2, 6.9 and 7.0 release notes (help.elgato.com refused
    our fetcher, so only the search engine's summaries were read), and two
    third-party reviews of 6.9 and 7.0.
  - Not used: the official app itself (not run), its files or any asset. No
    official profile with a Close key was on the owner's computer.
  - Public OS documentation: EWMH and ICCCM (X11), the wlr foreign toplevel
    management protocol (Wayland), Win32 `WM_CLOSE`, `EnumWindows` and
    `TerminateProcess`, AppKit `NSRunningApplication.terminate`.
- Device: none needed; the action acts on the computer. Official app: not run.
- System for the runs below: CachyOS, Hyprland, Xwayland, kitty.
- Checked by: `os::close::tests::*`, `os::close::linux::tests::*` (two of
  them ignored by default, see below), `actions::tests::close_needs_an_application`,
  `actions::tests::only_device_actions_run_while_locked` and
  `app_picker::tests::a_close_key_goes_by_the_running_name`.

## Observed (from the sources above)

- Close is a System action added in Stream Deck 6.2 (May 2023), for Windows
  and macOS. Elgato describes it as closing any application with one press,
  for example to end the work day or stop a running script.
- The application is chosen with a file button or by dropping it on the
  field. No other setting was described.
- The help center warns that for some apps (Discord and Microsoft Store apps
  are named) Close can stop working after the app updates, until the app is
  chosen again.
- Open Application (6.9) closes its app on a long press; 7.0 lets that long
  press be nothing, close or force quit, and marks a running app with a dot.
  These belong to Open Application, not to Close.

## Inferred

- Close is the polite close, since force quit appears only as a separate
  choice in Open Application. Weak: no source says how Close itself ends an
  application.
- The official setting holds a path to the program, which would explain the
  warning about apps whose path changes on update.

## Unknown

- Whether the official Close closes one window or all, every instance, or
  ends processes; what it does when the app is not running.
- Its UUID and settings in profile files. `com.elgato.streamdeck.system.close`
  is a guess, so importing official profiles still keeps such keys unchanged
  and lists them, as before.
- Its key icon.

## Deckly's own choices

- Settings: `app`, what the application is called while it runs (a program,
  app bundle, package id or window class; a path counts by its last part, as
  in profiles that follow the active window), and `application_name` for the
  inspector. The picker stores the running name it knows, so an update that
  moves the program does not break the key; File… stores the program's path.
- Every window of the application is asked to close as its close button
  would, so it may still ask about unsaved work. Deckly's own windows are
  never closed.
- Only when the application has no window (a script, a program in the tray)
  is its process asked to end: `SIGTERM` on Linux, `pkill -TERM` on macOS (for
  this user only), and `TerminateProcess` on Windows, which has no polite
  request for a program without a window. On Linux a script is also matched
  by its first argument, so `python3 run.py` closes by `run.py`.
- A key whose application is not running shows the failure mark.
- Close does not run while the computer is locked, like the other System
  actions.

## Platforms

- Linux X11: `_NET_CLOSE_WINDOW` through the window manager, or the window's
  own `WM_DELETE_WINDOW` when the manager does not offer it. Run:
  `closes_an_x11_window` closed a kitty window under Xwayland on Hyprland, and
  kitty exited by itself, not by a signal.
- Linux Wayland: the wlr foreign toplevel protocol, offered by wlroots
  desktops such as Hyprland, Sway and labwc. Run: `closes_a_wayland_window`
  closed a native kitty window on Hyprland the same way. GNOME and KDE do not
  offer a way for one application to close another's window; there Deckly
  falls back to `SIGTERM` and the inspector says so. Not run on GNOME or KDE.
- Ending a windowless process with `SIGTERM` was run in
  `a_program_without_a_window_is_asked_to_end`.
- Windows: `WM_CLOSE` to each visible top-level window, Microsoft Store apps
  by the app id of their frame window. Compile-checked only.
- macOS: `NSRunningApplication.terminate`, the same request as Quit in the
  menu. Compile-checked only.
