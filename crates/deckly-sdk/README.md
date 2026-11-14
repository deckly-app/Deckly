# deckly-sdk

Write plugins for [Deckly](https://github.com/deckly-app/Deckly) in Rust.
A plugin adds actions that a user places on the keys of a Stream Deck: show
the CPU load, count presses, talk to OBS.

```rust,no_run
use deckly_sdk::prelude::*;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct CounterSettings {
    step: u32,
}

#[derive(Default)]
struct Counter {
    count: u32,
}

impl Action for Counter {
    const ID: &'static str = "com.example.counter.count";
    const NAME: &'static str = "Counter";
    type Settings = CounterSettings;

    fn key_down(&mut self, key: &Key<Self>) {
        self.count += key.settings().step.max(1);
        key.set_face(Face::new().value(self.count.to_string()).label("Count"));
    }
}

fn main() -> deckly_sdk::Result<()> {
    Plugin::new("com.example.counter", "Counter")
        .action::<Counter>()
        .run()
}
```

- [Getting started](https://deckly-app.github.io/Deckly/getting-started.html):
  from `cargo new` to a key that reacts, in about ten minutes.
- [The SDK book](https://deckly-app.github.io/Deckly/): guides and reference.
- [`llms-full.txt`](https://deckly-app.github.io/Deckly/llms-full.txt): the
  whole book in one file, for AI agents.

Versions 0.0.x are previews: the API may still change with each release.

Deckly is an independent project and is not made, approved or supported by
Elgato or Corsair. Licensed under MIT.
