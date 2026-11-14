# Drawing keys

A key can show four things, from simple to free-form:

| Use | When |
| --- | --- |
| **Title** | A word or two of text. |
| **Face** | A value with a label, a detail line, an icon or a progress bar. Deckly draws it in the user's style. Use this for most keys. |
| **Image** | A picture you make yourself: PNG, JPEG, SVG or raw pixels. |
| **State** | A key with a few fixed looks, such as on and off. |

All of them are methods on [`Key`](actions.md#what-a-key-can-do). Each
update is sent only if it differs from the last one for that key.

## Titles

```rust
# use deckly_sdk::prelude::*;
# #[derive(Default)] struct Clock;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
# impl Action for Clock {
#     const ID: &'static str = "com.example.clock.time"; const NAME: &'static str = "Time";
#     type Settings = Empty;
fn key_appear(&mut self, key: &Key<Self>) {
    key.set_title("12:30");
}
# }
# fn main() {}
```

`clear_title` goes back to the title from the manifest. A title the user
typed in Deckly wins over the plugin's.

## Faces

A face is a description, not a picture: Deckly lays it out and draws it
with the key's title font and colour, so plugins look at home on every
user's deck.

```text
┌──────────────┐
│    label     │  small line at the top
│    [icon]    │  an image from the plugin folder
│    value     │  large text in the middle
│    detail    │  small line under the value
│ ▬▬▬▬▬▬▬───── │  progress bar, 0 to 1
└──────────────┘
```

```rust
# use deckly_sdk::prelude::*;
# #[derive(Default)] struct Cpu;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
# impl Action for Cpu {
#     const ID: &'static str = "com.example.stats.cpu"; const NAME: &'static str = "CPU";
#     type Settings = Empty;
fn tick(&mut self, key: &Key<Self>) {
    let load = 0.42; // read the real load here
    key.set_face(
        Face::new()
            .label("CPU")
            .value(format!("{:.0}%", load * 100.0))
            .detail("8 cores")
            .progress(load)
            .accent(Color::rgb(0x4c, 0xc2, 0xff))
            .icon("assets/cpu.svg"),
    );
}
# }
# fn main() {}
```

Every part is optional. Builders on [`Face`]:

| Builder | Shows |
| --- | --- |
| `label(text)` | A small line at the top. |
| `value(text)` | The large text in the middle. |
| `detail(text)` | A small line under the value. |
| `progress(f32)` | A bar along the bottom, from 0.0 to 1.0. |
| `icon(path)` | An image file inside the plugin folder (PNG, JPEG or SVG). |
| `color(Color)` | The text colour; the user's title colour if not set. |
| `background(Color)` | Fills the key; the key's own image shows if not set. |
| `accent(Color)` | The progress bar's colour. |

Text is cut to 32 characters and shrunk to fit the key. A title on the key
(the user's or the plugin's) takes the label's place, and the user's own
picture wins over the face's background. `clear_face` goes back to the
plain key.

Colours are `Color::rgb(r, g, b)`, or parsed from `"#rrggbb"` with
`Color::try_from(String)`, which suits a colour from a
[form field](forms.md).

## Images

When a face is not enough, send an image:

```rust
# use deckly_sdk::prelude::*;
# #[derive(Default)] struct Art;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
# impl Action for Art {
#     const ID: &'static str = "com.example.art.show"; const NAME: &'static str = "Art";
#     type Settings = Empty;
fn key_appear(&mut self, key: &Key<Self>) {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 144 144">
        <circle cx="72" cy="72" r="60" fill="#e84855"/>
    </svg>"##;
    key.set_image(&Image::svg(svg.as_bytes().to_vec()));
}
# }
# fn main() {}
```

| Constructor | Bytes |
| --- | --- |
| `Image::png(bytes)` | A PNG file. |
| `Image::jpeg(bytes)` | A JPEG file. |
| `Image::svg(bytes)` | SVG text. |
| `Image::rgba(width, height, pixels)` | Raw pixels, four bytes each (red, green, blue, alpha), row by row. |

Deckly scales the image to each device's key size (72 pixels on a
Stream Deck MK.2, 120 on a Stream Deck +), so 144 × 144 looks sharp
everywhere. An image is at most 4 MiB, and a raw image at most 1024 pixels
on a side. `clear_image` goes back to the manifest's image.

Build an `Image` once and keep it, rather than encoding the same picture on
every update: the SDK compares images by their bytes and skips repeats, but
the encoding is your cost.

## States

An action with states has a fixed title and image for each one, given in
the manifest. Deckly shows them before the plugin draws anything, and the
editor shows the first one.

```rust
# use deckly_sdk::prelude::*;
# #[derive(Clone, Default, Serialize, Deserialize)] struct Empty {}
#[derive(Default)]
struct Light;

impl Action for Light {
    const ID: &'static str = "com.example.home.light";
    const NAME: &'static str = "Light";
    type Settings = Empty;

    fn states() -> Vec<State> {
        vec![
            State { title: "Off".into(), image: "assets/light-off.svg".into() },
            State { title: "On".into(), image: "assets/light-on.svg".into() },
        ]
    }

    fn key_down(&mut self, key: &Key<Self>) {
        // Deckly never flips states by itself: the plugin decides.
        let next = if key.state() == Some(1) { 0 } else { 1 };
        key.set_state(next);
    }
}
# fn main() {}
```

`key.state()` is the state Deckly last reported. An action has at most 16
states.

## Feedback marks

`key.show_ok()` and `key.show_alert()` flash a short success or failure
mark on the key. Use `show_alert` when an action failed, such as when OBS
is not reachable, so the user sees it on the device.

[`Face`]: https://deckly-app.github.io/Deckly/api/deckly_sdk/struct.Face.html
