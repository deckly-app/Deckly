//! %%name_comment%%: a Deckly plugin with one action. Run `deckly-cli dev` to
//! see it in Deckly; `cargo test` runs the test at the end of this file.

use deckly_sdk::prelude::*;

/// The key's settings: none yet. Add fields here and a `form` to the action
/// to let the user set them.
#[derive(Clone, Default, Serialize, Deserialize)]
struct HelloSettings {}

/// Deckly makes one `Hello` for each key that shows the action.
#[derive(Default)]
struct Hello;

impl Action for Hello {
    // The plugin id goes in front: %%id_comment%%.hello.
    const ID: &'static str = "hello";
    const NAME: &'static str = "Hello";
    type Settings = HelloSettings;

    fn key_down(&mut self, key: &Key<Self>) {
        // Standard error is the plugin's log; never print to standard output.
        eprintln!("pressed");
        key.show_ok();
    }
}

deckly_sdk::plugin!(%%id%%, %%name%%, [Hello]);

#[cfg(test)]
mod tests {
    use super::*;
    use deckly_sdk::protocol::PluginMessage;
    use deckly_sdk::testing::Harness;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn a_press_shows_ok() {
        // The harness plays Deckly: no Deckly or device needed.
        let mut deck = Harness::start(plugin());
        deck.appear("key-1", Hello::ID, json!({}));
        deck.press("key-1");
        let ok = deck.wait_for(Duration::from_secs(5), |m| {
            matches!(m.message, PluginMessage::ShowOk { .. }).then_some(())
        });
        assert!(ok.is_some());
        deck.finish().unwrap();
    }
}
