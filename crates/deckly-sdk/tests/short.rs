//! The short forms: faces drawn by the SDK, settings read over their
//! defaults, short action ids, and the `plugin!` macro.

use deckly_sdk::prelude::*;
use deckly_sdk::protocol::PluginMessage;
use deckly_sdk::testing::Harness;
use serde_json::json;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(300);

#[derive(Clone, Serialize, Deserialize)]
struct Greeting {
    name: String,
    loud: bool,
}

impl Default for Greeting {
    fn default() -> Self {
        Self {
            name: "World".into(),
            loud: false,
        }
    }
}

#[derive(Default)]
struct Wave {
    presses: u32,
}

impl Action for Wave {
    const ID: &'static str = "wave";
    const NAME: &'static str = "Wave";
    type Settings = Greeting;

    fn form(form: &mut Form) {
        form.text("name", "Name");
        form.checkbox("loud", "Loud");
        form.button("reset", "Reset");
    }

    fn key_down(&mut self, _key: &Key<Self>) {
        self.presses += 1;
    }

    fn button(&mut self, _key: &Key<Self>, _button: &str) {
        self.presses = 0;
    }

    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let s = key.settings();
        let label = if s.loud {
            s.name.to_uppercase()
        } else {
            s.name
        };
        Some(Face::new().label(label).value(self.presses.to_string()))
    }
}

deckly_sdk::plugin!("com.example.short", "Short", [Wave]);

fn faces(deck: &Harness) -> Vec<Face> {
    let mut faces = Vec::new();
    while let Some(m) = deck.recv(QUIET) {
        if let PluginMessage::SetFace { face: Some(f), .. } = m.message {
            faces.push(f);
        }
    }
    faces
}

#[test]
fn the_sdk_draws_the_face_after_each_event_and_only_when_it_changed() {
    let mut deck = Harness::start(plugin());
    deck.appear("k", Wave::ID, json!({ "name": "Ada" }));
    let first = deck.face("k", WAIT).unwrap();
    assert_eq!(first.label.as_deref(), Some("Ada"));
    assert_eq!(first.value.as_deref(), Some("0"));
    // Down changes the face; up asks again and sends nothing.
    deck.press("k");
    let after = faces(&deck);
    assert_eq!(after.len(), 1, "{after:?}");
    assert_eq!(after[0].value.as_deref(), Some("1"));
    deck.change_settings("k", json!({ "name": "Ada", "loud": true }));
    assert_eq!(deck.face("k", WAIT).unwrap().label.as_deref(), Some("ADA"));
    deck.button("k", "reset");
    assert_eq!(deck.face("k", WAIT).unwrap().value.as_deref(), Some("0"));
    deck.finish().unwrap();
}

#[test]
fn short_ids_get_the_plugin_id_in_front() {
    let mut deck = Harness::start(plugin());
    assert_eq!(deck.manifest().actions[0].id, "com.example.short.wave");
    assert_eq!(deck.action_id(Wave::ID), "com.example.short.wave");
    assert!(deck.form(Wave::ID).is_some());
    // Deckly sends the full id; the harness does the same for a short one.
    deck.appear("k", "com.example.short.wave", json!({}));
    assert_eq!(
        deck.face("k", WAIT).unwrap().label.as_deref(),
        Some("World")
    );
    deck.finish().unwrap();
}

#[test]
fn missing_settings_keep_their_defaults() {
    let mut deck = Harness::start(plugin());
    deck.appear("k", Wave::ID, json!({ "loud": true }));
    assert_eq!(
        deck.face("k", WAIT).unwrap().label.as_deref(),
        Some("WORLD")
    );
    // One field that does not fit leaves the others alone.
    deck.change_settings("k", json!({ "name": "Ada", "loud": "yes" }));
    assert_eq!(deck.face("k", WAIT).unwrap().label.as_deref(), Some("Ada"));
    deck.finish().unwrap();
}

#[test]
fn the_macro_takes_the_package_from_cargo() {
    let m = plugin().manifest();
    assert_eq!(
        (m.id.as_str(), m.name.as_str()),
        ("com.example.short", "Short")
    );
    assert_eq!(m.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(m.license, "MIT");
    assert_eq!(m.description, env!("CARGO_PKG_DESCRIPTION"));
    assert!(plugin().check().is_ok());
}

#[derive(Default)]
struct Typo;

impl Action for Typo {
    const ID: &'static str = "typo";
    const NAME: &'static str = "Typo";
    type Settings = Greeting;

    fn form(form: &mut Form) {
        form.text("nmae", "Name");
        form.button("go", "Go");
    }
}

#[derive(Default)]
struct Elsewhere;

impl Action for Elsewhere {
    const ID: &'static str = "com.example.other.go";
    const NAME: &'static str = "Elsewhere";
    type Settings = serde_json::Value;
}

#[test]
fn check_finds_form_typos_and_foreign_ids() {
    let typo = Plugin::new("com.example.short", "Short").action::<Typo>();
    let e = typo.check().unwrap_err().to_string();
    assert!(e.contains("\"nmae\""), "{e}");
    let foreign = Plugin::new("com.example.short", "Short").action::<Elsewhere>();
    let e = foreign.check().unwrap_err().to_string();
    assert!(e.contains("does not start with the plugin id"), "{e}");
}
