# Deckly Plugin SDK

Write plugins for [Deckly](https://github.com/deckly-app/Deckly) in Rust.
A plugin adds actions that a user places on keys: show the CPU load, count
presses, talk to OBS.

```rust,no_run
use deckly_sdk::prelude::*;

/// What the user sets for each key; Deckly saves it in the profile.
#[derive(Clone, Default, Serialize, Deserialize)]
struct CounterSettings {
    step: u32,
}

/// One value per key, so per-key state is a plain field.
#[derive(Default)]
struct Counter {
    count: u32,
}

impl Action for Counter {
    const ID: &'static str = "count";
    const NAME: &'static str = "Counter";
    type Settings = CounterSettings;

    fn key_down(&mut self, key: &Key<Self>) {
        self.count += key.settings().step.max(1);
    }

    /// What the key shows; Deckly draws it again whenever it changes.
    fn face(&self, _key: &Key<Self>) -> Option<Face> {
        Some(Face::new().value(self.count.to_string()).label("Count"))
    }
}

deckly_sdk::plugin!("com.example.counter", "Counter", [Counter]);
```

> **Preview.** The SDK is at version 0.0.x and its API may still change. Deckly
> plugins need a Deckly built from the `dev` branch until the next release
> (see [Getting started](getting-started.md)).

## How it works

A plugin is a normal program. Deckly starts it, and the two talk over the
plugin's standard input and output. The SDK hides that: you write one Rust
type per action, and Deckly makes one value of it for each key that shows
the action.

Deckly draws everything itself: the plugin describes a key's **face**
(label, value, progress bar) and its **settings form** (text fields,
choices, sliders), and Deckly draws them with the user's fonts and theme. No
HTML, no web view.

A plugin that crashes or hangs never takes Deckly with it: Deckly restarts
it and keeps the rest of the app working.

## Where to go

| You want to | Read |
| --- | --- |
| Make a first plugin, from `deckly-cli create` to a key that reacts | [Getting started](getting-started.md) |
| Learn one topic: keys, faces, timers, settings, forms, tests | [Guides](guide/actions.md) |
| Look up a field, a limit or a message | [Reference](reference/manifest.md) |
| See every type and method | [API reference](reference/api.md) |
| Run an existing Stream Deck plugin in Deckly | [Stream Deck plugins in Deckly](stream-deck-plugins.md) |

A complete example plugin, with settings, forms, a timer and tests, is
[`examples/counter-plugin`](https://github.com/deckly-app/Deckly/tree/dev/examples/counter-plugin).

## For AI agents

The whole book is also plain Markdown, made for reading by tools:

- [`llms.txt`](https://deckly-app.github.io/Deckly/llms.txt): an index of
  every page, with links to their Markdown.
- [`llms-full.txt`](https://deckly-app.github.io/Deckly/llms-full.txt):
  every page in one file. Give an agent this one link and it has the whole
  SDK.
- The sources: [`docs/sdk`](https://github.com/deckly-app/Deckly/tree/dev/docs/sdk)
  in the Deckly repository.

Deckly is an independent project and is not made, approved or supported by
Elgato or Corsair.
