// The plugin from the SDK book's Getting started page
// (docs/sdk/getting-started.md). The page shows the code between the ANCHOR
// lines; `cargo test -p xtask` fails when the two differ.

// ANCHOR: plugin
use deckly_sdk::prelude::*;

/// What the user sets for each key. Deckly saves it in the profile.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct WaveSettings {
    name: String,
}

/// Deckly makes one `Wave` for each key that shows the action.
#[derive(Default)]
struct Wave {
    presses: u32,
}

impl Action for Wave {
    const ID: &'static str = "com.example.hello.wave";
    const NAME: &'static str = "Wave";
    type Settings = WaveSettings;

    // The settings form Deckly shows for the key.
    fn form(form: &mut Form) {
        form.text("name", "Name").placeholder("World");
    }

    fn key_appear(&mut self, key: &Key<Self>) {
        self.draw(key);
    }

    fn key_down(&mut self, key: &Key<Self>) {
        self.presses += 1;
        self.draw(key);
    }

    fn settings_changed(&mut self, key: &Key<Self>) {
        self.draw(key);
    }
}

impl Wave {
    fn draw(&self, key: &Key<Self>) {
        let mut name = key.settings().name;
        if name.is_empty() {
            name = "World".into();
        }
        key.set_face(
            Face::new()
                .label(format!("Hello, {name}"))
                .value(self.presses.to_string())
                .detail("presses"),
        );
    }
}

fn plugin() -> Plugin {
    Plugin::new("com.example.hello", "Hello")
        .version(env!("CARGO_PKG_VERSION"))
        .action::<Wave>()
}

fn main() -> deckly_sdk::Result<()> {
    plugin().run()
}
// ANCHOR_END: plugin

// ANCHOR: tests
#[cfg(test)]
mod tests {
    use super::*;
    use deckly_sdk::testing::Harness;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn greets_and_counts() {
        let mut deck = Harness::start(plugin());

        // The key appears with its saved settings and draws its first face.
        deck.appear("key-1", Wave::ID, serde_json::json!({ "name": "Ada" }));
        let face = deck.face("key-1", WAIT).unwrap();
        assert_eq!(face.label.as_deref(), Some("Hello, Ada"));
        assert_eq!(face.value.as_deref(), Some("0"));

        deck.press("key-1");
        let face = deck.face("key-1", WAIT).unwrap();
        assert_eq!(face.value.as_deref(), Some("1"));

        deck.finish().unwrap();
    }
}
// ANCHOR_END: tests
