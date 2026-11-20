//! Where the Open action puts the window it starts: on which monitor, in
//! which state (maximized, minimized, fullscreen) and, on a tiling desktop,
//! floating or tiled.
//!
//! Each desktop can do part of this. [`support`] says which part, so the
//! inspector offers only that; a profile made on another system may still
//! ask for more, and [`open`] then opens the target with what it can and
//! says what it left out. The placement applies to the window the started
//! program opens: a program that is already running, or that hands the
//! request to another process (most browsers, single-instance apps), keeps
//! its own window where it is.

use serde_json::Value;

#[cfg(target_os = "linux")]
mod hyprland;
#[cfg(windows)]
mod windows;

/// Longest monitor id kept from a profile.
const MAX_MONITOR_ID: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowState {
    /// As the program decides.
    #[default]
    Default,
    /// Neither maximized nor minimized.
    Normal,
    Maximized,
    Minimized,
    Fullscreen,
}

impl WindowState {
    pub const ALL: [WindowState; 5] = [
        WindowState::Default,
        WindowState::Normal,
        WindowState::Maximized,
        WindowState::Minimized,
        WindowState::Fullscreen,
    ];

    pub fn id(self) -> &'static str {
        match self {
            WindowState::Default => "",
            WindowState::Normal => "normal",
            WindowState::Maximized => "maximized",
            WindowState::Minimized => "minimized",
            WindowState::Fullscreen => "fullscreen",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WindowState::Default => "As the app decides",
            WindowState::Normal => "Normal",
            WindowState::Maximized => "Maximized",
            WindowState::Minimized => "Minimized",
            WindowState::Fullscreen => "Fullscreen",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|s| s.id() == id)
            .unwrap_or_default()
    }
}

/// Floating or tiled, for tiling desktops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// As the desktop's own rules decide.
    #[default]
    Default,
    Floating,
    Tiled,
}

impl Layout {
    pub const ALL: [Layout; 3] = [Layout::Default, Layout::Floating, Layout::Tiled];

    pub fn id(self) -> &'static str {
        match self {
            Layout::Default => "",
            Layout::Floating => "floating",
            Layout::Tiled => "tiled",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Layout::Default => "As the desktop decides",
            Layout::Floating => "Floating",
            Layout::Tiled => "Tiled",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|l| l.id() == id)
            .unwrap_or_default()
    }
}

/// The Open action's window settings: `monitor`, `window` and `layout`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    /// A [`Monitor::id`], or empty for wherever the desktop puts it.
    pub monitor: String,
    /// The monitor's name when it was chosen, to name it while it is gone.
    pub monitor_name: String,
    pub state: WindowState,
    pub layout: Layout,
}

impl Placement {
    pub fn from_settings(settings: &Value) -> Self {
        let text = |field: &str| {
            settings
                .get(field)
                .and_then(Value::as_str)
                .filter(|s| s.len() <= MAX_MONITOR_ID)
                .unwrap_or("")
                .to_owned()
        };
        Self {
            monitor: text("monitor"),
            monitor_name: text("monitor_name"),
            state: WindowState::from_id(&text("window")),
            layout: Layout::from_id(&text("layout")),
        }
    }

    pub fn is_default(&self) -> bool {
        self.monitor.is_empty()
            && self.state == WindowState::Default
            && self.layout == Layout::Default
    }

    /// What this placement asks for that `support` cannot do, in words.
    pub fn unsupported(&self, support: &Support) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.monitor.is_empty() && !support.monitor {
            missing.push("choosing a monitor".to_owned());
        }
        if self.state != WindowState::Default && !support.states.contains(&self.state) {
            missing.push(format!("{} windows", self.state.label().to_lowercase()));
        }
        if self.layout != Layout::Default && !support.layout {
            missing.push(format!("{} windows", self.layout.label().to_lowercase()));
        }
        missing
    }
}

/// A connected monitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Monitor {
    /// What a profile stores. It names the same screen across restarts and,
    /// where the system allows, after it moves to another port.
    pub id: String,
    /// What the user sees, such as `DELL P2214H (DP-2)`.
    pub label: String,
    /// What the system calls it now: Hyprland's port name (`DP-2`), the
    /// Windows device name (`\\.\DISPLAY2`).
    pub port: String,
}

/// Which window settings this desktop can apply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Support {
    pub monitor: bool,
    /// Window states besides [`WindowState::Default`].
    pub states: Vec<WindowState>,
    pub layout: bool,
    /// Why some or all of it is missing, for the inspector.
    pub note: Option<String>,
}

impl Support {
    pub fn any(&self) -> bool {
        self.monitor || !self.states.is_empty() || self.layout
    }
}

/// What this desktop can do. Cheap enough to call while drawing.
pub fn support() -> Support {
    #[cfg(target_os = "linux")]
    if hyprland::running() {
        return hyprland::support();
    }
    #[cfg(windows)]
    return windows::support();
    #[allow(unreachable_code)]
    Support {
        note: Some(unsupported_note().to_owned()),
        ..Support::default()
    }
}

#[allow(dead_code)]
fn unsupported_note() -> &'static str {
    if cfg!(target_os = "macos") {
        "Choosing a monitor or window state is not supported on macOS yet."
    } else if cfg!(target_os = "linux") {
        "Choosing a monitor or window state is supported on Hyprland only, \
         not on this desktop yet."
    } else {
        "Choosing a monitor or window state is not supported on this system."
    }
}

/// The monitors connected now.
pub fn monitors() -> Result<Vec<Monitor>, String> {
    #[cfg(target_os = "linux")]
    if hyprland::running() {
        return hyprland::monitors();
    }
    #[cfg(windows)]
    return windows::monitors();
    #[allow(unreachable_code)]
    Err(unsupported_note().to_owned())
}

/// The connected monitor a profile's id names.
pub fn find<'a>(monitors: &'a [Monitor], id: &str) -> Option<&'a Monitor> {
    monitors.iter().find(|m| m.id == id)
}

/// Opens `target` as [`super::open`] does, with its window placed. The
/// target opens even when part of the placement cannot be applied; the
/// error then says which part.
pub fn open(target: &str, placement: &Placement) -> Result<(), String> {
    if placement.is_default() {
        return super::open(target);
    }
    let support = support();
    let missing = placement.unsupported(&support);
    let mut notes = Vec::new();
    if !support.any() {
        super::open(target)?;
    } else {
        #[cfg(target_os = "linux")]
        notes.extend(hyprland::open(target, placement)?);
        #[cfg(windows)]
        notes.extend(windows::open(target, placement)?);
    }
    if !missing.is_empty() {
        notes.push(format!(
            "{} is not available on this desktop",
            missing.join(" and ")
        ));
    }
    if notes.is_empty() {
        Ok(())
    } else {
        Err(format!("Opened, but {}", notes.join("; ")))
    }
}

/// The note for a chosen monitor that is not connected.
#[allow(dead_code)]
fn missing_monitor(placement: &Placement) -> String {
    let name = if placement.monitor_name.is_empty() {
        &placement.monitor
    } else {
        &placement.monitor_name
    };
    format!("monitor {name} is not connected, so it opened where the desktop put it")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_settings_and_ignores_unknown_or_oversized_values() {
        let p = Placement::from_settings(&json!({
            "monitor": "desc:Dell P2214H",
            "monitor_name": "DELL P2214H (DP-2)",
            "window": "maximized",
            "layout": "floating",
        }));
        assert_eq!(p.monitor, "desc:Dell P2214H");
        assert_eq!(p.state, WindowState::Maximized);
        assert_eq!(p.layout, Layout::Floating);
        assert!(!p.is_default());

        let odd = Placement::from_settings(&json!({
            "monitor": "x".repeat(MAX_MONITOR_ID + 1),
            "window": "sideways",
            "layout": 3,
        }));
        assert!(odd.is_default());
        assert!(Placement::from_settings(&json!("not an object")).is_default());
        assert!(Placement::from_settings(&json!({"path": "spotify"})).is_default());
    }

    #[test]
    fn ids_round_trip() {
        for s in WindowState::ALL {
            assert_eq!(WindowState::from_id(s.id()), s);
        }
        for l in Layout::ALL {
            assert_eq!(Layout::from_id(l.id()), l);
        }
    }

    #[test]
    fn names_what_a_desktop_cannot_do() {
        let windows = Support {
            monitor: true,
            states: vec![
                WindowState::Normal,
                WindowState::Maximized,
                WindowState::Minimized,
            ],
            layout: false,
            note: None,
        };
        let from_hyprland = Placement {
            monitor: "DP-1".into(),
            state: WindowState::Fullscreen,
            layout: Layout::Tiled,
            ..Placement::default()
        };
        assert_eq!(
            from_hyprland.unsupported(&windows),
            ["fullscreen windows", "tiled windows"]
        );
        assert!(
            from_hyprland
                .unsupported(&Support {
                    monitor: true,
                    states: vec![WindowState::Fullscreen],
                    layout: true,
                    note: None,
                })
                .is_empty()
        );
        assert_eq!(
            from_hyprland.unsupported(&Support::default()),
            ["choosing a monitor", "fullscreen windows", "tiled windows"]
        );
    }
}
