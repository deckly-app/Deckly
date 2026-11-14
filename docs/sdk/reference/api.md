# API reference

Every public type and method, generated from the source by rustdoc:

- [**deckly_sdk**](https://deckly-app.github.io/Deckly/api/deckly_sdk/):
  `Plugin`, `Action`, `Key`, `Face`, `Image`, `Form`, `Globals`, the test
  `Harness`.
- [**deckly_plugin_protocol**](https://deckly-app.github.io/Deckly/api/deckly_plugin_protocol/):
  the messages, the manifest and the limits. The SDK re-exports it as
  `deckly_sdk::protocol`.

The quickest way in is the `prelude`, which brings in what most plugins
use:

```rust
use deckly_sdk::prelude::*;
// Action, Choice, Color, Face, Form, Globals, Image, Key, Plugin, State,
// and serde's Serialize and Deserialize.
# fn main() {}
```

## Offline

Open the same reference for the exact SDK version your plugin uses, from
your plugin's own folder:

```sh
cargo doc -p deckly-sdk --open
```
