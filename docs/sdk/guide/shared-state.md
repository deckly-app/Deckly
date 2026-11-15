# Shared state and background events

Some plugins talk to one other program for all of their keys: a streaming
app, a smart-home hub, a music player. Every key needs the same connection
and the same picture of that program (is it recording, which scene is
live), and when the program changes on its own, say the user presses
Record in it, the keys must show it.

`Shared<T>` is that picture. The plugin has one value of each type `T`,
created with `T::default()` the first time anyone asks for it. Every key
and every thread that asks for a `T` gets the same value. After a change,
the SDK asks every visible key's `face` again and sends only the faces
that differ from what the key shows, so a key whose face did not change
gets nothing.

## Reading and changing it

In a handler or in `face`, `key.shared::<T>()` gives the value:

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
#[derive(Default)]
struct Player {
    playing: bool,
}

#[derive(Default)]
struct PlayPause;

impl Action for PlayPause {
    const ID: &'static str = "play";
    const NAME: &'static str = "Play/Pause";
    type Settings = Empty;

    fn key_down(&mut self, key: &Key<Self>) {
        // Every key that shows `playing` is drawn again.
        key.shared::<Player>().update(|p| p.playing = !p.playing);
    }

    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let playing = key.shared::<Player>().read(|p| p.playing);
        Some(Face::new().label("Player").value(if playing { "Pause" } else { "Play" }))
    }
}
# fn main() {}
```

- `read(|v| ..)` looks at the value, `update(|v| ..)` changes it and has
  the keys drawn again, `set(v)` replaces it, and `get()` copies it when
  `T` is `Clone`.
- Many updates in a row lead to one redraw.
- The value sits behind a lock. Keep the closures short, and do not call
  `read` or `update` on the same `T` inside one of them: the thread would
  wait for itself.

Per-key state still belongs in the action's fields; settings the user edits
for the whole plugin are [global settings](global-settings.md).

## Listening on a thread of its own

`Plugin::on_start` runs once when the plugin starts, before any key
appears. Start the thread that listens to the other program there. A
`Shared` is cheap to clone and can move to any thread; an `update` from
that thread wakes the SDK, which draws the keys on its own thread. There is
no polling: when nothing changes, nothing runs.

The example below stands a fake event source in for a real streaming app.
The keys never talk to it; they only read `Studio`.

```rust,no_run
# use deckly_sdk::prelude::*;
use std::sync::mpsc::{Receiver, channel};

/// What the streaming app last reported.
#[derive(Default)]
struct Studio {
    recording: bool,
    scene: String,
}

/// An event from the streaming app.
enum StudioEvent {
    Recording(bool),
    Scene(String),
}

/// Stands in for a real connection: in a plugin this reads a socket.
fn connect() -> Receiver<StudioEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(StudioEvent::Scene("Intro".into()));
        let _ = tx.send(StudioEvent::Recording(true));
    });
    rx
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Empty {}

#[derive(Default)]
struct Record;

impl Action for Record {
    const ID: &'static str = "record";
    const NAME: &'static str = "Record";
    type Settings = Empty;

    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let on = key.shared::<Studio>().read(|s| s.recording);
        Some(Face::new().label("Record").value(if on { "On" } else { "Off" }))
    }
}

#[derive(Default)]
struct Scene;

impl Action for Scene {
    const ID: &'static str = "scene";
    const NAME: &'static str = "Scene";
    type Settings = Empty;

    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let scene = key.shared::<Studio>().read(|s| s.scene.clone());
        Some(Face::new().label("Scene").value(scene))
    }
}

fn plugin() -> Plugin {
    Plugin::new("com.example.studio", "Studio")
        .action::<Record>()
        .action::<Scene>()
        .on_start(|plugin| {
            let studio = plugin.shared::<Studio>();
            std::thread::spawn(move || {
                for event in connect() {
                    studio.update(|s| match event {
                        StudioEvent::Recording(on) => s.recording = on,
                        StudioEvent::Scene(name) => s.scene = name,
                    });
                }
            });
        })
}

fn main() {
    plugin().run_or_exit()
}
```

When recording starts, only the Record keys get a new face; the Scene keys
show the same scene and get nothing.

- Keys that disappeared are not drawn, and a key that appears later reads
  the current value in its first `face`.
- `on_start` runs on the thread that handles key events. Do not connect
  there; start a thread and return.
- A `Shared` kept after the plugin stopped still holds its value, but
  changing it draws nothing. Let the listening thread end when its
  connection closes.
- `plugin.globals()` in `on_start` gives the [global settings](global-settings.md),
  such as the address to connect to. Deckly sends them right after the
  start, so they may not be loaded yet: connect from
  `Plugin::on_global_settings` when they arrive, or have the thread wait.

## Testing it

Hand the `Shared` from `on_start` to the test through a channel, change it,
and check what the [harness](testing.md) receives:

```rust
# use deckly_sdk::prelude::*;
# #[derive(Default)] struct Studio { recording: bool }
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
# #[derive(Default)] struct Record;
# impl Action for Record {
#     const ID: &'static str = "record"; const NAME: &'static str = "Record";
#     type Settings = Empty;
#     fn face(&self, key: &Key<Self>) -> Option<Face> {
#         let on = key.shared::<Studio>().read(|s| s.recording);
#         Some(Face::new().label("Record").value(if on { "On" } else { "Off" }))
#     }
# }
use deckly_sdk::testing::Harness;
use std::time::Duration;

let (tx, rx) = std::sync::mpsc::channel();
let plugin = Plugin::new("com.example.studio", "Studio")
    .action::<Record>()
    .on_start(move |plugin| {
        let _ = tx.send(plugin.shared::<Studio>());
    });
let mut deck = Harness::start(plugin);
let studio = rx.recv().unwrap();
deck.appear("k1", Record::ID, serde_json::json!({}));
let wait = Duration::from_secs(5);
assert_eq!(deck.face("k1", wait).unwrap().value.as_deref(), Some("Off"));

std::thread::spawn(move || studio.update(|s| s.recording = true));
assert_eq!(deck.face("k1", wait).unwrap().value.as_deref(), Some("On"));
deck.finish().unwrap();
```
