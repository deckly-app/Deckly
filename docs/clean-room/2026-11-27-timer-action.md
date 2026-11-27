# Timer action

Deckly's Timer action (`com.deckly.system.timer`): `timer.rs`,
`runtime/timer_audio.rs`, `actions.rs`, `render.rs`, `runtime.rs`.

## Sources and method

- Date: 2026-11-27.
- Public web search for the official app's built-in Timer: Elgato's help
  center (Zendesk search API), Elgato Marketplace listings, search result
  snippets and tutorial video titles and thumbnails. No binary, profile or
  capture of the official app was inspected for this entry.
- Deckly's own behavior was checked on CachyOS, Hyprland 0.57.2 (Wayland),
  PipeWire 1.6, without a device (`DECKLY_NO_DEVICES=1`).

## Observed

- Elgato's help center has no article on a built-in Timer action. Searches
  for "Timer" return Sleep After, Multi Action delays and Wave Link pages.
- A third-party countdown plugin's page (seen in a search result snippet)
  says the official app ships with a Timer action, that its time can only be
  changed in the Property Inspector, and that it cannot be paused.
- Marketplace has many third-party timer and countdown plugins (start,
  pause, add time, reset), which suggests the built-in one is basic.

## Inferred

- The built-in action is a countdown set in its settings and started from
  the key. This is what Deckly implements.

## Unknown

- The official action's UUID and settings in profile files, so importing it
  is not possible yet. Imported keys with an unknown official UUID stay
  unsupported, as before.
- Whether the official action plays a sound, which one, whether a second
  press pauses, resets or cancels, and what the key shows when it ends.
- Next step: put an official Timer key in a profile on the owner's machine,
  export it, and note its UUID, settings and behavior here.

## Deckly's behavior

- Settings: `seconds` (1 s to 99:59:59, out-of-range and wrong types are
  clamped or fall back to 60 s) and `chime` (default on). Test:
  `timer::tests::untrusted_settings_are_bounded_and_round_trip`.
- A press starts the countdown; a press while it runs cancels it. The key
  shows the time left (MM:SS, or HH:MM:SS from an hour), and a green 00:00
  for 3 seconds when it ends. Time comes from a fixed deadline, so it does
  not drift with redraws. Tests:
  `timer::tests::deadlines_do_not_drift_and_expiration_only_fires_once`,
  `render::tests::timer_draws_and_hashes_each_second_and_the_finished_state`.
- The runtime wakes once a second per running timer, not more. The key image
  is resent only when the shown second changes.
- Switching pages or profiles does not stop a countdown; it ends and chimes
  on time. Test:
  `runtime::tests::a_timer_counts_on_its_key_and_can_finish_on_another_page`.
- Editing or deleting the key ends its countdown; a countdown never rings
  with settings the key no longer has. Test:
  `runtime::tests::an_edited_timer_cannot_ring_and_a_muted_timer_still_finishes`.
- Timers belong to one device: the same pinned key on two devices counts
  separately. Disconnecting or disabling a device ends its timers. Test:
  `timer::tests::a_second_press_cancels_only_that_devices_timer`.
- Timer cannot go in a Multi Action or Key Logic list: it needs a key of its
  own to show the countdown and to be cancelled. Delay covers pauses in a
  sequence.
- The chime is a short two-note tone generated in code, played on the
  system's default output by its own thread through `rodio` (WASAPI on
  Windows, CoreAudio on macOS, ALSA on Linux, which PipeWire and PulseAudio
  serve). Timers ending together share one chime. No output device is a
  notice in the editor, not a failure of the timer. Test:
  `timer_audio::tests::the_generated_chime_is_short_bounded_and_fades_at_each_end`;
  `plays_on_the_default_audio_output` (ignored, plays a sound) passed on the
  system above.
- While the computer is locked, a press does not start a timer. A countdown
  already running still ends and chimes.

## Platforms

- Windows, macOS: compile-checked only (`x86_64-pc-windows-msvc`,
  `aarch64-apple-darwin`).
- Linux Wayland (Hyprland): the editor showed Timer keys; the chime played.
  Pressing a Timer key on a device was not run.
- Linux X11: the code has nothing specific to X11 or Wayland (no window,
  input or screen access). Not run under X11.
