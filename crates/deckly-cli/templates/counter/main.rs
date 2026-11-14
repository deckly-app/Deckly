//! %%name_comment%%: a Deckly plugin with one action, a key that counts its
//! presses. Run `deckly-cli dev` to see it in Deckly; `cargo test` runs the
//! tests at the end of this file.

use deckly_sdk::prelude::*;

/// What the user sets for each key, in its settings form. Deckly saves it in
/// the profile, so the count survives restarts.
#[derive(Clone, Serialize, Deserialize)]
struct CounterSettings {
    /// Shown at the top of the key.
    label: String,
    /// Added on each press.
    step: u32,
    count: u64,
}

impl Default for CounterSettings {
    fn default() -> Self {
        Self {
            label: "Count".into(),
            step: 1,
            count: 0,
        }
    }
}

/// Deckly makes one `Counter` for each key that shows the action.
#[derive(Default)]
struct Counter;

impl Action for Counter {
    // The plugin id goes in front: %%id_comment%%.counter.
    const ID: &'static str = "counter";
    const NAME: &'static str = "Counter";
    const TOOLTIP: &'static str = "Counts how often the key is pressed";
    type Settings = CounterSettings;

    // The settings form Deckly shows for the key. Each field's key names a
    // field of CounterSettings.
    fn form(form: &mut Form) {
        form.text("label", "Label").length(1..=12).default("Count");
        form.number("step", "Step").range(1..=100).default(1);
        form.button("reset", "Reset");
    }

    fn key_down(&mut self, key: &Key<Self>) {
        key.update_settings(|s| s.count += u64::from(s.step.max(1)));
    }

    fn button(&mut self, key: &Key<Self>, button: &str) {
        if button == "reset" {
            key.update_settings(|s| s.count = 0);
        }
    }

    // What the key shows. The SDK asks after every event and sends the face
    // to Deckly only when it changed.
    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let s = key.settings();
        Some(
            Face::new()
                .label(&s.label)
                .value(s.count.to_string())
                .detail(format!("step {}", s.step.max(1))),
        )
    }
}

deckly_sdk::plugin!(%%id%%, %%name%%, [Counter]);

#[cfg(test)]
mod tests {
    use super::*;
    use deckly_sdk::testing::Harness;
    use serde_json::json;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn counts_presses_and_saves_the_count() {
        // The harness plays Deckly: no Deckly or device needed.
        let mut deck = Harness::start(plugin());
        deck.appear("key-1", Counter::ID, json!({ "step": 5 }));
        let face = deck.face("key-1", WAIT).unwrap();
        assert_eq!(face.value.as_deref(), Some("0"));
        assert_eq!(face.label.as_deref(), Some("Count"));

        deck.press("key-1");
        let saved = deck.settings("key-1", WAIT).unwrap();
        assert_eq!(saved["count"], 5);
        assert_eq!(deck.face("key-1", WAIT).unwrap().value.as_deref(), Some("5"));

        deck.button("key-1", "reset");
        assert_eq!(deck.settings("key-1", WAIT).unwrap()["count"], 0);
        deck.finish().unwrap();
    }
}
