# Protocol

You do not need this page to write a plugin: the SDK speaks the protocol
for you. It is here for debugging, and for anyone curious about what
travels between Deckly and a plugin. The Rust types are in
[`deckly_sdk::protocol`](api.md).

## Transport

Deckly starts the plugin program and talks to it over the program's
standard input (Deckly to plugin) and standard output (plugin to Deckly).
Standard error is the plugin's log. Deckly closes standard input to stop
the plugin.

## Frames

Each frame is:

```text
┌──────────────────────┬──────────┬──────────────┐
│ length: u32, LE      │ kind: u8 │ body         │
│ (body bytes only)    │          │ length bytes │
└──────────────────────┴──────────┴──────────────┘
```

| Kind | Body |
| --- | --- |
| 0 | A JSON message, at most 1 MiB. |
| 1 | A binary blob (image bytes) for the message right after it, at most 4 MiB. |
| other | Skipped, so a later protocol can add kinds. |

A length over the limit is refused before anything is allocated. Bytes that
look like text, not a frame, are reported as text written to standard
output.

## Messages

Messages are JSON objects tagged by `type`, in snake case. Unknown types
and unknown fields are ignored, so new features come as new types or
fields, never as changed ones. A known message with a missing or wrong
field is a protocol error, and Deckly restarts the plugin.

### Handshake

1. Deckly sends `hello` first.
2. The plugin answers `ready` within 10 seconds, before anything else.

```json
{"type": "hello", "protocol": 1, "app_version": "0.1.0", "platform": "linux",
 "session": "wayland", "language": "en", "devices": [], "services": []}
{"type": "ready", "protocol": 1, "sdk": "0.1.0"}
```

`ready.protocol` must be between Deckly's oldest and newest supported
numbers. The current protocol number is 1.

### Deckly to plugin

| `type` | Fields | When |
| --- | --- | --- |
| `hello` | `protocol`, `app_version`, `platform` (`windows`, `mac`, `linux`), `session` (`x11`, `wayland` or empty), `language`, `devices`, `services` | First message. |
| `device_connected` | `device` | A device was plugged in. |
| `device_disconnected` | `device` (its id) | A device went away. |
| `key_appear` | `key` | A key with one of the plugin's actions is shown. |
| `key_disappear` | `key` | It is no longer shown. |
| `key_down`, `key_up` | `key` | Pressed, released. |
| `settings` | `key` | The key's settings changed outside the plugin. |
| `global_settings` | `settings` | The plugin's global settings. |
| `system_wake` | | The computer woke from sleep. |
| `get_choices` | `request`, `field`, `key` (none for the plugin-wide form) | A form with a dynamic choice opened. |
| `button` | `field`, `key` (none for the plugin-wide form) | A button in a form was pressed. |

A `key` is:

```json
{"context": "a1b2…", "action": "com.example.hello.wave", "device": "…",
 "column": 2, "row": 1, "state": null, "settings": {"name": "Ada"},
 "in_multi_action": false}
```

A `device` is `{"id": "…", "name": "Stream Deck MK.2", "columns": 5, "rows": 3}`.
Its id is stable and does not reveal the serial number.

### Plugin to Deckly

| `type` | Fields | Does |
| --- | --- | --- |
| `ready` | `protocol`, `sdk` | Answers `hello`. |
| `set_title` | `context`, `title` (null clears), `state` | Sets the key's title. |
| `set_state` | `context`, `state` | Moves the key to a state. |
| `set_face` | `context`, `face` (null clears) | Shows a face. |
| `set_image` | `context`, `format` (null clears), `state` | Shows the image in the blob frame sent right before. |
| `set_settings` | `context`, `settings` | Stores the key's settings. |
| `set_global_settings` | `settings` | Stores the global settings. |
| `get_global_settings` | | Asks for a `global_settings` message. |
| `show_ok`, `show_alert` | `context` | Flashes a mark on the key. |
| `choices` | `request`, `options`, `error` | Answers `get_choices`. |

`format` is `{"kind": "png"}`, `{"kind": "jpeg"}`, `{"kind": "svg"}` or
`{"kind": "rgba", "width": 72, "height": 72}`.

A `face` holds any of `label`, `value`, `detail` (text), `progress` (0 to
1), `color`, `background`, `accent` (`"#rrggbb"`) and `icon` (a path in the
plugin folder).

## Rules Deckly enforces

- Nothing before `ready`, and `ready` within 10 seconds.
- Every message within the [limits](limits.md).
- Two blobs in a row are an error; a blob not followed by a message that
  uses it is dropped.
- Deckly's queue to a plugin holds 1024 frames. A plugin that stops reading
  its input until the queue is full is restarted.
