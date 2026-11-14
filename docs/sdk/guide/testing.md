# Testing

`deckly_sdk::testing::Harness` plays Deckly: it starts your plugin on a
thread, talks to it over the real protocol, and lets you press keys and look
at what the plugin draws. Tests need neither Deckly nor a device and run in
`cargo test` on every system.

## A first test

```rust
use deckly_sdk::prelude::*;
use deckly_sdk::testing::Harness;
use serde_json::json;
use std::time::Duration;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct CountSettings {
    count: u64,
}

#[derive(Default)]
struct Count;

impl Action for Count {
    const ID: &'static str = "com.example.count.count";
    const NAME: &'static str = "Count";
    type Settings = CountSettings;

    fn key_down(&mut self, key: &Key<Self>) {
        key.update_settings(|s| s.count += 1);
        key.set_title(key.settings().count.to_string());
    }
}

fn plugin() -> Plugin {
    Plugin::new("com.example.count", "Count").action::<Count>()
}

// In your crate this is a #[test] function.
fn presses_are_counted_and_saved() {
    let wait = Duration::from_secs(5);
    let mut deck = Harness::start(plugin());

    deck.appear("k1", Count::ID, json!({ "count": 41 }));
    deck.press("k1");

    // What the plugin saved, and what it drew.
    assert_eq!(deck.settings("k1", wait), Some(json!({ "count": 42 })));
    assert_eq!(deck.title("k1", wait).as_deref(), Some("42"));

    deck.finish().unwrap();
}
# fn main() { presses_are_counted_and_saved() }
```

A plugin is easiest to test when a function builds the `Plugin` (like
`plugin()` above) and `main` only calls `plugin().run()`. Put the plugin in
`src/lib.rs` and keep `src/main.rs` to those lines if you want tests in
`tests/`.

## Driving the plugin

| Harness method | Plays |
| --- | --- |
| `appear(key, action, settings)` | A key with that action is shown. |
| `disappear(key)` | The key goes away. |
| `press(key)`, `key_down(key)`, `key_up(key)` | Pressing the key. |
| `change_settings(key, settings)` | The user edits the key's settings. |
| `place(key, action, settings)` | A key in the editor only, with no device. |
| `button(key, field)` | A button in the key's form. |
| `choices(key, field, timeout)` | The form asks for a dynamic choice's options. |
| `set_globals(settings)` | The user edits the global settings. |
| `plugin_button(field)`, `plugin_choices(field, timeout)` | The same for the plugin-wide form. |
| `send(message)` | Any protocol message. |

## Looking at what the plugin did

Each of these waits up to `timeout` for the next matching message and
returns `None` if none comes:

| Harness method | Returns |
| --- | --- |
| `face(key, timeout)` | The next face drawn on the key. |
| `title(key, timeout)` | The next title. |
| `settings(key, timeout)` | The next settings the plugin stored for the key. |
| `stored_globals(timeout)` | The next global settings the plugin stored. |
| `recv(timeout)` | The next message of any kind. |
| `wait_for(timeout, pick)` | The first message `pick` accepts. |

They skip messages that do not match, so wait for things in the order the
plugin sends them.

Also: `globals()` is the global settings as last stored, `manifest()`,
`form(action)` and `plugin_form()` are what `--describe` would print, and
`ready` is the plugin's answer to the handshake.

## Ending the test

`deck.finish()` closes the plugin's input, as Deckly does when it quits,
and waits for `run` to return. It returns the plugin's error, if any.

## Timers in tests

A key's timer runs in real time in the harness too. Keep periods short in
tests, or test the drawing code directly and leave the timer to one test.

## The real host

The example plugins in the Deckly repository also run under the real plugin
host in Deckly's own tests
([`examples/counter-plugin/tests/host.rs`](https://github.com/deckly-app/Deckly/blob/dev/examples/counter-plugin/tests/host.rs)).
Plugin authors do not need that: the harness speaks the same protocol.
