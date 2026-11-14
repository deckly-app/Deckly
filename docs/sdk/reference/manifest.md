# deckly-plugin.json

The manifest describes a plugin to Deckly. It sits at the top of the
plugin folder. The SDK writes it: run the plugin program with
`--describe`. See [Packaging and installing](../guide/packaging.md).

```json
{
  "id": "com.example.hello",
  "name": "Hello",
  "version": "0.1.0",
  "author": "Ada Lovelace",
  "license": "MIT",
  "description": "Greets people.",
  "icon": "assets/plugin.svg",
  "category": "",
  "protocol": 1,
  "sdk": "0.1.0",
  "services": [],
  "programs": {
    "x86_64-unknown-linux-gnu": "hello-deckly"
  },
  "actions": [
    {
      "id": "com.example.hello.wave",
      "name": "Wave",
      "icon": "assets/wave.svg",
      "tooltip": "",
      "states": [],
      "form": {
        "fields": [
          { "kind": "text", "key": "name", "label": "Name", "placeholder": "World" }
        ]
      }
    }
  ]
}
```

## Plugin

| Field | Set with | Meaning |
| --- | --- | --- |
| `id` | `Plugin::new(id, _)` | Reverse-DNS id, also the folder name. Letters, digits, `.`, `-`, `_`; at most 128 bytes. Never change it after release. |
| `name` | `Plugin::new(_, name)` | Shown in the Plugins tab. Required. |
| `version` | `.version(..)` | The plugin's version; `0.1.0` if not set. |
| `author` | `.author(..)` | Shown with the plugin. |
| `license` | `.license(..)` | The plugin's licence, such as `MIT`. |
| `description` | `.description(..)` | Up to 2000 characters. |
| `icon` | `.icon(..)` | An image in the plugin folder. |
| `category` | `.category(..)` | The group in the action list; the plugin's name when empty. |
| `protocol` | the SDK | The protocol number the plugin needs. Deckly refuses to start a plugin that needs a newer one. |
| `sdk` | the SDK | The SDK version that wrote the manifest. |
| `services` | | Host services the plugin uses. Empty for now. |
| `programs` | `--describe` | The program to start per target triple, as a path in the plugin folder. See [Several systems](../guide/packaging.md#several-systems). |
| `actions` | `.action::<A>()` | The actions, below. At most 256. |
| `form` | `.form(..)` | The plugin-wide settings form. Absent when there is none. |

## Action

| Field | Set with | Meaning |
| --- | --- | --- |
| `id` | `Action::ID` | Reverse-DNS, starting with the plugin id. Unique in the plugin. Profiles store it, so never change it after release. |
| `name` | `Action::NAME` | Shown in the action list. |
| `icon` | `Action::ICON` | An image in the plugin folder for the action list. |
| `tooltip` | `Action::TOOLTIP` | Hover text in the action list. |
| `states` | `Action::states()` | `[{ "title": "...", "image": "..." }]`; at most 16. |
| `form` | `Action::form()` | The key's settings form; see [Form fields](form-fields.md). |

## Rules

- Unknown fields are ignored, so a newer SDK can add fields that an older
  Deckly skips.
- Paths are relative to the plugin folder and must stay in it: no leading
  `/`, no `..`, no drive letter.
- The file is at most 1 MiB. A UTF-8 byte order mark is accepted.
- Every limit is in [Limits](limits.md).
