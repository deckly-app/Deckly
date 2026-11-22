# Wallpaper slideshow

Follows [Global wallpaper](2026-11-07-global-wallpaper.md) and
[Wallpaper across all keys](2026-11-04-key-wallpaper.md).

- Date: 2026-11-22.
- Method: Deckly's own model, renderer, runtime and editor code. The
  official app was not run and its documentation was not consulted for
  this step. No proprietary code, profiles or assets were used.
- System: Linux/CachyOS. Unit tests only; no device was opened, so no
  device or firmware was involved.

## Observed

- Nothing about the official app. This is a Deckly feature, not a copy of
  official behaviour.

## Inferred

- None about the official app.

## Unknown

- Whether the official app can change a wallpaper over time at all.
  Deckly imports no slideshow from official profiles or backups.
- How fast each model takes the key images of a transition over USB, and
  so how smooth a push or crossfade looks on a real device. Not measured.

## Deckly decisions

- A slideshow lives inside a wallpaper, as the optional `slideshow` field
  of the global, profile, page or folder wallpaper. The wallpaper's own
  picture is the first one; `slides` holds the others with their own zoom
  and place. So the order global, profile, page or folder and the
  opt-outs work as before. The field is written only when set; older files
  load unchanged and a file without it is a single picture.
- Limits, applied on load: at most 32 pictures in all, each 2 s to 24 h,
  transitions of 0.1 s to 5 s. Pictures inside a slideshow carry no
  slideshow of their own. A slideshow with no picture left is dropped.
  When the first picture is missing (an export or import that cannot copy
  it), the next one takes its place.
- Transitions, all across the whole panel as one picture cut into keys:
  - Wave across and wave from the middle: each key turns to the next
    picture once, at its own moment along the wave. One image per key.
  - Wipe: an edge sweeps from left to right over the still pictures. A key
    changes when the edge reaches it and while the edge is on it.
  - Push: the next picture slides in from the right and pushes the shown
    one out. Every key changes on every frame.
  - Crossfade: every key changes on every frame.
  Wipe, push and crossfade ease in and out.
- Timing: each picture stays its interval, then the transition runs; after
  the last picture the first comes again. Between transitions the runtime
  sleeps until the next one is due (no polling). During a transition it
  wakes at most 20 times a second; the frame is taken from the clock, so a
  slow device skips frames instead of falling behind (queued images for a
  key replace the ones not yet sent). A device asleep, or under a
  confirmation screen, pauses its slideshow; it goes on from where it
  stopped. A changed slideshow starts from its first picture.
- A key's hash includes only what of the wallpaper it shows: the picture
  and its place, or for a key inside a transition the two pictures and how
  far it is. So a key is sent only when its pixels change. A key whose
  still custom picture or plugin image has no transparent pixel, drawn at
  full size, does not depend on the wallpaper and is not sent again when
  the picture changes.
- The editor canvas and key previews show the first picture. The Wallpaper
  windows show one picture at a time: numbered buttons pick it, arrows
  move it earlier or later, a minus takes it out, **Add…** adds a picture.
  Zoom and dragging act on the picture picked. Transition, time per
  picture and transition time show once there are two pictures.
- The global wallpapers folder keeps every picture of the global
  slideshow when it is pruned. Backups carry every picture of it; a file
  used twice is stored once.

## Validation

- Tests: `slideshows_round_trip_and_old_wallpapers_have_none`,
  `slideshows_are_brought_within_limits`,
  `slides_move_and_go_while_the_first_picture_stays_the_wallpaper`,
  `a_missing_first_slide_gives_way_to_the_next` (profile);
  `the_global_slideshow_round_trips_and_drops_long_paths` (settings);
  `waves_turn_every_key_once_in_order`,
  `wipes_push_and_fades_draw_both_pictures`,
  `opaque_key_pictures_hide_the_wallpaper_from_the_hash` (render);
  `a_slideshow_shows_each_picture_then_a_transition_to_the_next`,
  `a_paused_slideshow_stands_still_and_goes_on_where_it_stopped`,
  `a_slideshow_wakes_the_loop_only_when_it_has_something_to_show`
  (runtime); `slideshow_times_read_and_print_in_plain_units`,
  `a_picture_read_joins_or_replaces_as_asked` (wallpaper window);
  `every_picture_of_a_global_slideshow_goes_into_the_backup` (backup);
  `global_wallpapers_have_their_own_folder_and_old_ones_are_pruned`
  (store).
- Not run on a device. Not looked at in the running app.
