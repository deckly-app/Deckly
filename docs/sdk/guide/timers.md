# Timers and threads

## Timers

`key.every(period)` calls the action's `tick` handler on that key every
`period`, until `key.stop_timer()` or until the key disappears. `tick` gets
`&mut self`, so it reads and changes the key's state with no locks.

```rust
# use deckly_sdk::prelude::*;
# use std::time::{Duration, Instant};
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
#[derive(Default)]
struct Stopwatch {
    started: Option<Instant>,
}

impl Action for Stopwatch {
    const ID: &'static str = "com.example.time.stopwatch";
    const NAME: &'static str = "Stopwatch";
    type Settings = Empty;

    fn key_down(&mut self, key: &Key<Self>) {
        if self.started.take().is_some() {
            key.stop_timer();
        } else {
            self.started = Some(Instant::now());
            key.every(Duration::from_millis(100));
        }
        self.draw(key);
    }

    fn tick(&mut self, key: &Key<Self>) {
        self.draw(key);
    }
}

impl Stopwatch {
    fn draw(&self, key: &Key<Self>) {
        let secs = self.started.map_or(0.0, |t| t.elapsed().as_secs_f32());
        key.set_face(Face::new().label("Stopwatch").value(format!("{secs:.1}")));
    }
}
# fn main() {}
```

- A key has one timer. Calling `every` again replaces it.
- The shortest period is 10 ms.
- The timer stops by itself when the key disappears, so there is nothing to
  clean up.
- Ticks run on the same thread as key events. Keep `tick` fast.

## Slow work on another thread

All of a plugin's events run on one thread, one after another. A handler
that waits for the network holds up every key of the plugin. Do slow work
on a thread of its own and send the result to the key from there: a
[`Key`](actions.md#what-a-key-can-do) can be cloned and moved to any
thread.

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)] #[serde(default)] struct WeatherSettings { city: String }
# fn fetch_temperature(_city: &str) -> Result<f32, String> { Ok(21.0) }
# #[derive(Default)] struct Weather;
# impl Action for Weather {
#     const ID: &'static str = "com.example.weather.now"; const NAME: &'static str = "Weather";
#     type Settings = WeatherSettings;
fn key_down(&mut self, key: &Key<Self>) {
    let key = key.clone();
    std::thread::spawn(move || {
        let city = key.settings().city;
        match fetch_temperature(&city) {
            Ok(t) => key.set_face(Face::new().label(city).value(format!("{t:.0}°"))),
            Err(e) => {
                eprintln!("weather for {city}: {e}");
                key.show_alert();
            }
        }
    });
}
# }
# fn main() {}
```

If the key disappears before the thread finishes, the calls on it do
nothing. Check `key.is_visible()` to stop a long loop early.

For work that repeats, such as polling a server every few seconds, combine
both: a timer that starts the work, and a thread (or one shared worker)
that does it. Do not start a new thread on every tick if the last one may
still be running.

## State shared by every key

Per-key state is a field of the action. State the whole plugin shares, such
as a connection to OBS, lives outside the actions: a `static` with a
`Mutex` or `OnceLock`, or a type you keep in one. Settings that the user
edits for the whole plugin are [global settings](global-settings.md).

```rust
# use deckly_sdk::prelude::*;
use std::sync::{Mutex, OnceLock};

struct Connection { /* a socket, a client */ }

fn connection() -> &'static Mutex<Option<Connection>> {
    static CONNECTION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
    CONNECTION.get_or_init(|| Mutex::new(None))
}
# fn main() { let _ = connection(); }
```

## When the plugin stops

Deckly closes the plugin's standard input when it quits or restarts the
plugin; `run()` then returns and `main` ends. Every key gets
`key_disappear` first. A plugin that does not exit within 3 seconds is
killed, so do not count on long clean-up.
