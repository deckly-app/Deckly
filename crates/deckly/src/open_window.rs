//! The Open action's window rows in the inspector: the monitor, the window
//! state and, on tiling desktops, floating or tiled. They edit the key's
//! settings and nothing else; the monitor list is read from the desktop
//! while the rows are shown, at most every few seconds, and more often
//! while the monitor list is open.

use crate::actions;
use crate::os::placement::{self, Layout, Monitor, Placement, Support, WindowState};
use crate::theme::{TEXT_DIM, WARNING};
use eframe::egui::{self, RichText, Ui};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(3);
const REFRESH_WHILE_OPEN: Duration = Duration::from_millis(750);
const ANYWHERE: &str = "Where the desktop puts it";

/// What the desktop reported last.
#[derive(Default)]
pub struct Monitors {
    read: Option<Instant>,
    support: Support,
    list: Option<Result<Vec<Monitor>, String>>,
}

impl Monitors {
    fn refresh(&mut self, every: Duration) {
        if self.read.is_some_and(|read| read.elapsed() < every) {
            return;
        }
        self.read = Some(Instant::now());
        self.support = placement::support();
        self.list = self.support.monitor.then(placement::monitors);
    }

    fn connected(&self) -> &[Monitor] {
        match &self.list {
            Some(Ok(list)) => list,
            _ => &[],
        }
    }
}

/// Adds the rows to the inspector's grid. `label` draws a row's name.
pub fn rows(ui: &mut Ui, s: &mut Value, monitors: &mut Monitors, label: &dyn Fn(&mut Ui, &str)) {
    monitors.refresh(REFRESH);
    let support = monitors.support.clone();
    let placement = Placement::from_settings(s);
    if !support.any() && placement.is_default() {
        label(ui, "Window");
        note(ui, support.note.as_deref().unwrap_or_default(), TEXT_DIM);
        ui.end_row();
        return;
    }

    if support.monitor || !placement.monitor.is_empty() {
        label(ui, "Monitor");
        ui.vertical(|ui| monitor_row(ui, s, monitors, &support, &placement));
        ui.end_row();
    }

    label(ui, "Window");
    ui.vertical(|ui| {
        let mut states = vec![WindowState::Default];
        states.extend(support.states.iter().copied());
        let available = states.contains(&placement.state);
        if !available {
            states.push(placement.state);
        }
        egui::ComboBox::from_id_salt("open-window-state")
            .selected_text(placement.state.label())
            .width(260.0)
            .show_ui(ui, |ui| {
                for state in states {
                    if ui
                        .selectable_label(state == placement.state, state.label())
                        .clicked()
                    {
                        actions::set_setting(s, "window", json!(state.id()));
                    }
                }
            });
        if !available {
            note(ui, "Not available on this desktop.", WARNING);
        }
    });
    ui.end_row();

    if support.layout || placement.layout != Layout::Default {
        label(ui, "Layout");
        ui.vertical(|ui| {
            egui::ComboBox::from_id_salt("open-window-layout")
                .selected_text(placement.layout.label())
                .width(260.0)
                .show_ui(ui, |ui| {
                    for layout in Layout::ALL {
                        if ui
                            .selectable_label(layout == placement.layout, layout.label())
                            .clicked()
                        {
                            actions::set_setting(s, "layout", json!(layout.id()));
                        }
                    }
                });
            if !support.layout {
                note(ui, "Not available on this desktop.", WARNING);
            }
        });
        ui.end_row();
    }

    let placed = !Placement::from_settings(s).is_default();
    let mut notes = Vec::new();
    if placed && actions::str_setting(s, "path").contains("://") {
        notes.push(
            "A link opens in the program that handles it. When that program is \
             already running, its window stays where it is.",
        );
    }
    if placed && let Some(text) = support.note.as_deref() {
        notes.push(text);
    }
    if !notes.is_empty() {
        label(ui, "");
        ui.vertical(|ui| {
            for text in notes {
                note(ui, text, TEXT_DIM);
            }
        });
        ui.end_row();
    }
}

fn monitor_row(
    ui: &mut Ui,
    s: &mut Value,
    monitors: &mut Monitors,
    support: &Support,
    placement: &Placement,
) {
    let current = placement::find(monitors.connected(), &placement.monitor).cloned();
    let selected = if placement.monitor.is_empty() {
        ANYWHERE.to_owned()
    } else {
        match &current {
            Some(monitor) => monitor.label.clone(),
            None => format!("{} (not connected)", stored_name(placement)),
        }
    };
    let mut choice: Option<Option<Monitor>> = None;
    egui::ComboBox::from_id_salt("open-monitor")
        .selected_text(selected)
        .width(260.0)
        .show_ui(ui, |ui| {
            monitors.refresh(REFRESH_WHILE_OPEN);
            if ui
                .selectable_label(placement.monitor.is_empty(), ANYWHERE)
                .clicked()
            {
                choice = Some(None);
            }
            match &monitors.list {
                Some(Ok(list)) => {
                    for monitor in list {
                        if ui
                            .selectable_label(monitor.id == placement.monitor, &monitor.label)
                            .clicked()
                        {
                            choice = Some(Some(monitor.clone()));
                        }
                    }
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(WARNING));
                }
                None => {}
            }
        });
    match choice {
        Some(Some(monitor)) => {
            actions::set_setting(s, "monitor", json!(monitor.id));
            actions::set_setting(s, "monitor_name", json!(monitor.label));
        }
        Some(None) => {
            actions::set_setting(s, "monitor", json!(""));
            actions::set_setting(s, "monitor_name", json!(""));
        }
        None => {}
    }
    if !support.monitor {
        note(ui, "Not available on this desktop.", WARNING);
    } else if !placement.monitor.is_empty() && current.is_none() {
        if let Some(Err(e)) = &monitors.list {
            note(ui, e, WARNING);
        } else {
            note(
                ui,
                "Not connected now: the app opens where the desktop puts it.",
                WARNING,
            );
        }
    }
}

fn stored_name(placement: &Placement) -> &str {
    if placement.monitor_name.is_empty() {
        &placement.monitor
    } else {
        &placement.monitor_name
    }
}

fn note(ui: &mut Ui, text: &str, color: egui::Color32) {
    ui.set_max_width(340.0);
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(color)).wrap());
}
