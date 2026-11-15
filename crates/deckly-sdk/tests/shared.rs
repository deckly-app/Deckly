//! Plugin-wide shared state changed from another thread.
#![allow(clippy::panic)] // The probe helper fails the test.

use deckly_sdk::prelude::*;
use deckly_sdk::protocol::PluginMessage;
use deckly_sdk::testing::{Harness, Sent};
use std::sync::Mutex;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

/// What a program the plugin listens to last reported.
#[derive(Default)]
struct Obs {
    recording: bool,
    scene: String,
}

/// Every key whose face was asked for, in order.
static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn asked(key: &str) -> usize {
    ASKED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(|k| *k == key)
        .count()
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Empty {}

#[derive(Default)]
struct Record;

impl Action for Record {
    const ID: &'static str = "record";
    const NAME: &'static str = "Record";
    type Settings = Empty;

    fn key_down(&mut self, key: &Key<Self>) {
        key.shared::<Obs>().update(|o| o.recording = !o.recording);
    }

    fn face(&self, key: &Key<Self>) -> Option<Face> {
        ASKED
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(key.context().to_owned());
        let on = key.shared::<Obs>().read(|o| o.recording);
        Some(
            Face::new()
                .label("Record")
                .value(if on { "On" } else { "Off" }),
        )
    }
}

#[derive(Default)]
struct Scene;

impl Action for Scene {
    const ID: &'static str = "scene";
    const NAME: &'static str = "Scene";
    type Settings = Empty;

    fn face(&self, key: &Key<Self>) -> Option<Face> {
        let scene = key.shared::<Obs>().read(|o| o.scene.clone());
        Some(Face::new().label("Scene").value(scene))
    }
}

/// Answers a press with a title, so a test knows the plugin handled
/// everything sent before it.
#[derive(Default)]
struct Probe {
    presses: u32,
}

impl Action for Probe {
    const ID: &'static str = "probe";
    const NAME: &'static str = "Probe";
    type Settings = Empty;

    fn key_down(&mut self, key: &Key<Self>) {
        // A new title each time: one equal to the last is not sent.
        self.presses += 1;
        key.set_title(self.presses.to_string());
    }
}

fn plugin(handle: Sender<Shared<Obs>>) -> Plugin {
    Plugin::new("com.example.obs", "OBS")
        .action::<Record>()
        .action::<Scene>()
        .action::<Probe>()
        .on_start(move |plugin| {
            let _ = handle.send(plugin.shared::<Obs>());
        })
}

/// Everything the plugin sends until the probe key answers.
fn until_probe(deck: &mut Harness, probe: &str) -> Vec<Sent> {
    deck.press(probe);
    let mut seen = Vec::new();
    loop {
        let Some(m) = deck.recv(WAIT) else {
            panic!("the probe did not answer; got {seen:?}")
        };
        if matches!(&m.message, PluginMessage::SetTitle { context, .. } if context == probe) {
            return seen;
        }
        seen.push(m);
    }
}

fn faces_of<'a>(sent: &'a [Sent], key: &str) -> Vec<&'a Face> {
    sent.iter()
        .filter_map(|m| match &m.message {
            PluginMessage::SetFace {
                context,
                face: Some(face),
            } if context == key => Some(face),
            _ => None,
        })
        .collect()
}

fn touches(sent: &[Sent], key: &str) -> bool {
    sent.iter().any(|m| match &m.message {
        PluginMessage::SetFace { context, .. }
        | PluginMessage::SetTitle { context, .. }
        | PluginMessage::SetState { context, .. }
        | PluginMessage::SetImage { context, .. }
        | PluginMessage::SetSettings { context, .. }
        | PluginMessage::ShowOk { context }
        | PluginMessage::ShowAlert { context } => context == key,
        _ => false,
    })
}

#[test]
fn an_update_from_another_thread_redraws_only_changed_faces() {
    let (tx, rx) = channel();
    let mut deck = Harness::start(plugin(tx));
    let obs = rx.recv_timeout(WAIT).expect("the start hook ran");
    deck.appear("rec", Record::ID, serde_json::json!({}));
    deck.appear("scene", Scene::ID, serde_json::json!({}));
    deck.appear("probe", Probe::ID, serde_json::json!({}));
    let first = until_probe(&mut deck, "probe");
    assert_eq!(faces_of(&first, "rec").len(), 1);
    assert_eq!(faces_of(&first, "scene").len(), 1);

    // The program reports that recording started.
    std::thread::spawn(move || obs.update(|o| o.recording = true))
        .join()
        .expect("the update thread");
    let face = deck
        .face("rec", WAIT)
        .expect("the record key is drawn again");
    assert_eq!(face, Face::new().label("Record").value("On"));
    let after = until_probe(&mut deck, "probe");
    assert!(
        faces_of(&after, "scene").is_empty(),
        "the scene key did not change, so no face is sent: {after:?}"
    );
    deck.finish().expect("the plugin stops cleanly");
}

#[test]
fn a_key_handler_changes_what_other_keys_show() {
    let (tx, _rx) = channel();
    let mut deck = Harness::start(plugin(tx));
    deck.appear("rec-a", Record::ID, serde_json::json!({}));
    deck.appear("rec-b", Record::ID, serde_json::json!({}));
    deck.appear("probe-2", Probe::ID, serde_json::json!({}));
    until_probe(&mut deck, "probe-2");
    deck.press("rec-a");
    let sent = until_probe(&mut deck, "probe-2");
    let on = Face::new().label("Record").value("On");
    assert_eq!(faces_of(&sent, "rec-a"), vec![&on]);
    assert_eq!(faces_of(&sent, "rec-b"), vec![&on]);
    deck.finish().expect("the plugin stops cleanly");
}

#[test]
fn keys_that_disappeared_are_not_drawn() {
    let (tx, rx) = channel();
    let mut deck = Harness::start(plugin(tx));
    let obs = rx.recv_timeout(WAIT).expect("the start hook ran");
    deck.appear("gone", Record::ID, serde_json::json!({}));
    deck.appear("stays", Record::ID, serde_json::json!({}));
    deck.appear("probe-3", Probe::ID, serde_json::json!({}));
    deck.disappear("gone");
    until_probe(&mut deck, "probe-3");
    let gone = asked("gone");

    obs.update(|o| o.recording = true);
    let sent = until_probe(&mut deck, "probe-3");
    assert_eq!(faces_of(&sent, "stays").len(), 1);
    assert!(!touches(&sent, "gone"), "nothing is sent for it: {sent:?}");
    assert_eq!(asked("gone"), gone, "its face is not asked for");
    deck.finish().expect("the plugin stops cleanly");
}

#[test]
fn a_handle_that_outlives_the_plugin_is_harmless() {
    let (tx, rx) = channel();
    let mut deck = Harness::start(plugin(tx));
    let obs = rx.recv_timeout(WAIT).expect("the start hook ran");
    deck.appear("rec-late", Record::ID, serde_json::json!({}));
    deck.finish().expect("the plugin stops cleanly");

    obs.update(|o| o.scene = "Live".into());
    obs.update(|o| o.recording = true);
    assert!(obs.read(|o| o.recording));
    assert_eq!(obs.read(|o| o.scene.clone()), "Live");
}
