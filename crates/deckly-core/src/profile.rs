//! Profiles, pages and key assignments.
//!
//! A profile belongs to one device model and holds one or more pages; each
//! page maps key indices (row-major, top-left is 0) to a key configuration.
//! Profiles are stored as JSON in Deckly's own format. Loading never trusts
//! the file: out-of-range keys are dropped and limits are enforced.

use crate::device::{DeviceKind, DeviceModel};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_PAGES: usize = 100;
pub const MAX_FOLDERS: usize = 500;
/// Folders nest at most this deep; deeper folder keys open nothing.
pub const MAX_FOLDER_DEPTH: usize = 16;
pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_PAGE_NAME_CHARS: usize = 64;
pub const MAX_PROFILE_NAME_CHARS: usize = 64;
/// Applications one profile can follow; more are dropped on load.
pub const MAX_PROFILE_APPLICATIONS: usize = 64;
const MAX_APPLICATION_CHARS: usize = 256;
const MAX_APPLICATION_ICON_BYTES: usize = 1024;
/// The name of the profile made for a device that has none.
pub const DEFAULT_PROFILE_NAME: &str = "Default Profile";
/// Profile files larger than this are refused before parsing.
pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// A key that opens a folder; its `folder` setting holds the folder's id.
pub const FOLDER_ACTION: &str = "com.deckly.navigation.folder";
/// The key every folder gets to go back to where it was opened from.
pub const PARENT_FOLDER_ACTION: &str = "com.deckly.navigation.parent-folder";

/// Runs its actions in order.
pub const MULTI_ACTION: &str = "com.deckly.multi.action";
/// Two Multi Actions on one key; each press runs the next one.
pub const MULTI_ACTION_SWITCH: &str = "com.deckly.multi.switch";
/// Runs one of its actions, picked at random.
pub const RANDOM_ACTION: &str = "com.deckly.multi.random";
/// One action each for a single press, a double press and a hold.
pub const KEY_LOGIC: &str = "com.deckly.multi.key-logic";
/// A pause between the actions of a Multi Action.
pub const DELAY_ACTION: &str = "com.deckly.multi.delay";
/// The actions one list of a Multi Action can hold.
pub const MAX_GROUP_ACTIONS: usize = 100;

/// How many lists of actions a key with this action holds.
pub fn group_count(action: &str) -> usize {
    match action {
        MULTI_ACTION | RANDOM_ACTION => 1,
        MULTI_ACTION_SWITCH => 2,
        KEY_LOGIC => 3,
        _ => 0,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default = "format_version")]
    pub version: u32,
    pub id: String,
    pub name: String,
    pub device: DeviceKind,
    /// Default for pages and folders without a wallpaper override. `None`
    /// follows the global wallpaper in the settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<Wallpaper>,
    /// An explicit choice to leave this profile without the global
    /// wallpaper. Its pages and folders can still have their own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wallpaper_disabled: bool,
    pub pages: Vec<Page>,
    /// Folders, each one page of keys opened by a folder key. Only folders
    /// that a key leads to are kept.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<Page>,
    /// Pinned keys: each shows at its place on every page and folder of the
    /// profile where that place has no key of its own
    /// (docs/clean-room/2026-11-22-pinned-actions.md). `None` when nothing is
    /// pinned.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_pinned"
    )]
    pub pinned: Option<Page>,
    /// Applications that bring this profile up while they are in the
    /// foreground. Kept here so they leave with the profile when it is
    /// deleted and travel with it in exports.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applications: Vec<AppRule>,
}

/// An application that brings a profile up
/// (docs/clean-room/2026-11-02-active-window-profiles.md).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    /// As Preferences lists it.
    pub name: String,
    /// A program, bundle, package or window class. A path keeps only its last
    /// part: programs move between versioned folders, and a shared profile
    /// lands on a computer with other folders.
    pub id: String,
    /// Where the system finds its picture on this computer, as the picker or
    /// a chosen file gave it. Empty or gone leaves the plain window icon.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

impl AppRule {
    pub fn new(name: &str, id: &str) -> Self {
        let limit = |text: &str| text.trim().chars().take(MAX_APPLICATION_CHARS).collect();
        let id: String = limit(last_part(id));
        Self {
            name: if name.trim().is_empty() {
                id.clone()
            } else {
                limit(name)
            },
            id,
            icon: String::new(),
        }
    }

    /// A source too long to be real leaves the rule without a picture.
    pub fn with_icon(mut self, icon: &str) -> Self {
        if icon.len() <= MAX_APPLICATION_ICON_BYTES {
            self.icon = icon.to_owned();
        }
        self
    }

    /// Whether this is one of the names the system gives an application. Two
    /// applications with the same program file name both match.
    pub fn matches(&self, names: &[String]) -> bool {
        let id = self.id.to_lowercase();
        names.iter().any(|n| last_part(n).to_lowercase() == id)
    }
}

fn last_part(path: &str) -> &str {
    let path = path.trim_end_matches(['/', '\\']);
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    /// Empty shows the page's number, see [`Page::title`].
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Picture shown for the page instead of its number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
    /// One picture spread over all keys of the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<Wallpaper>,
    /// An explicit choice to leave this page or folder without wallpaper.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wallpaper_disabled: bool,
    #[serde(default)]
    pub keys: BTreeMap<u8, KeyConfig>,
}

/// A picture spread over all keys of a page, in the device's rows and
/// columns. The keys show it where their own picture leaves room: empty
/// keys and action icons show it whole, transparent parts of key pictures
/// let it through.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Wallpaper {
    pub image: PathBuf,
    /// In percent. 100 covers all keys with as much of the picture as fits
    /// their shape; more shows a smaller part of it.
    #[serde(default = "Wallpaper::default_zoom")]
    pub zoom: u16,
    /// The middle of the part shown, in thousandths of the picture's width
    /// and height. It moves only as far as the picture reaches.
    #[serde(default = "Wallpaper::middle")]
    pub x: u16,
    #[serde(default = "Wallpaper::middle")]
    pub y: u16,
    /// More pictures shown in turn after this one. Never set on the
    /// pictures inside a slideshow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slideshow: Option<Slideshow>,
}

/// Pictures a wallpaper changes to in turn, with an animated transition
/// between them. A Deckly feature (docs/clean-room/2026-11-22-wallpaper-slideshow.md).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Slideshow {
    /// The pictures after the wallpaper's own, in order; then it starts over.
    #[serde(default)]
    pub slides: Vec<Wallpaper>,
    /// How long each picture stays, in seconds.
    #[serde(default = "Slideshow::default_interval")]
    pub interval: u32,
    #[serde(default)]
    pub transition: Transition,
    /// How long a transition takes, in milliseconds.
    #[serde(default = "Slideshow::default_duration")]
    pub duration: u32,
}

/// How one picture of a slideshow gives way to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    /// Key by key, from the top left corner to the bottom right one.
    #[default]
    Diagonal,
    /// Key by key, from the middle outwards.
    Center,
    /// An edge moves across the keys and uncovers the next picture.
    Wipe,
    /// The next picture pushes the shown one out to the left.
    Push,
    /// The next picture fades in over the shown one.
    Fade,
}

impl Transition {
    pub const ALL: [Self; 5] = [
        Self::Diagonal,
        Self::Center,
        Self::Wipe,
        Self::Push,
        Self::Fade,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Diagonal => "Wave across",
            Self::Center => "Wave from the middle",
            Self::Wipe => "Wipe",
            Self::Push => "Push",
            Self::Fade => "Crossfade",
        }
    }
}

impl Slideshow {
    /// Pictures in one slideshow, the wallpaper's own included.
    pub const MAX_PICTURES: usize = 32;
    pub const MIN_INTERVAL: u32 = 2;
    /// One day.
    pub const MAX_INTERVAL: u32 = 24 * 60 * 60;
    pub const MIN_DURATION: u32 = 100;
    pub const MAX_DURATION: u32 = 5000;

    pub fn new() -> Self {
        Self {
            slides: Vec::new(),
            interval: Self::default_interval(),
            transition: Transition::default(),
            duration: Self::default_duration(),
        }
    }

    fn default_interval() -> u32 {
        30
    }

    fn default_duration() -> u32 {
        1000
    }

    fn sanitize(&mut self) {
        self.slides.retain(|w| !w.image.as_os_str().is_empty());
        self.slides.truncate(Self::MAX_PICTURES - 1);
        for slide in &mut self.slides {
            slide.slideshow = None;
            slide.sanitize();
        }
        self.interval = self.interval.clamp(Self::MIN_INTERVAL, Self::MAX_INTERVAL);
        self.duration = self.duration.clamp(Self::MIN_DURATION, Self::MAX_DURATION);
    }
}

impl Default for Slideshow {
    fn default() -> Self {
        Self::new()
    }
}

impl Wallpaper {
    pub const MIN_ZOOM: u16 = 100;
    pub const MAX_ZOOM: u16 = 400;
    /// The largest `x` and `y`: the right and bottom edges.
    pub const END: u16 = 1000;

    pub fn new(image: PathBuf) -> Self {
        Self {
            image,
            zoom: Self::default_zoom(),
            x: Self::middle(),
            y: Self::middle(),
            slideshow: None,
        }
    }

    /// How many pictures it shows in turn: 1 without a slideshow.
    pub fn slide_count(&self) -> usize {
        1 + self.slideshow.as_ref().map_or(0, |s| s.slides.len())
    }

    /// Picture `index` of the slideshow; 0 is the wallpaper itself.
    pub fn slide(&self, index: usize) -> Option<&Wallpaper> {
        match index {
            0 => Some(self),
            i => self.slideshow.as_ref()?.slides.get(i - 1),
        }
    }

    pub fn slide_mut(&mut self, index: usize) -> Option<&mut Wallpaper> {
        match index {
            0 => Some(self),
            i => self.slideshow.as_mut()?.slides.get_mut(i - 1),
        }
    }

    /// Whether two wallpapers show the same picture the same way, whatever
    /// their slideshows.
    pub fn same_picture(&self, other: &Wallpaper) -> bool {
        (&self.image, self.zoom, self.x, self.y) == (&other.image, other.zoom, other.x, other.y)
    }

    /// Takes picture `index` out of the slideshow. The next picture takes
    /// the place of the first one; the last one cannot go.
    pub fn remove_slide(&mut self, index: usize) {
        let Some(show) = self.slideshow.as_mut() else {
            return;
        };
        if index == 0 {
            if show.slides.is_empty() {
                return;
            }
            let next = show.slides.remove(0);
            (self.image, self.zoom, self.x, self.y) = (next.image, next.zoom, next.x, next.y);
        } else if index <= show.slides.len() {
            show.slides.remove(index - 1);
        }
        if show.slides.is_empty() {
            self.slideshow = None;
        }
    }

    /// Swaps pictures `a` and `b` of the slideshow.
    pub fn swap_slides(&mut self, a: usize, b: usize) {
        let count = self.slide_count();
        if a == b || a >= count || b >= count {
            return;
        }
        let (a, b) = (a.min(b), a.max(b));
        let Some(show) = self.slideshow.as_mut() else {
            return;
        };
        if a == 0 {
            let Some(other) = show.slides.get_mut(b - 1) else {
                return;
            };
            std::mem::swap(&mut self.image, &mut other.image);
            std::mem::swap(&mut self.zoom, &mut other.zoom);
            std::mem::swap(&mut self.x, &mut other.x);
            std::mem::swap(&mut self.y, &mut other.y);
        } else {
            show.slides.swap(a - 1, b - 1);
        }
    }

    /// Adds a picture at the end of the slideshow, unless it is full.
    pub fn push_slide(&mut self, slide: Wallpaper) -> bool {
        if self.slide_count() >= Slideshow::MAX_PICTURES {
            return false;
        }
        let show = self.slideshow.get_or_insert_with(Slideshow::new);
        show.slides.push(Wallpaper {
            slideshow: None,
            ..slide
        });
        true
    }

    fn default_zoom() -> u16 {
        Self::MIN_ZOOM
    }

    fn middle() -> u16 {
        Self::END / 2
    }

    pub fn sanitize(&mut self) {
        self.zoom = self.zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.x = self.x.min(Self::END);
        self.y = self.y.min(Self::END);
        if let Some(show) = &mut self.slideshow {
            show.sanitize();
            if show.slides.is_empty() {
                self.slideshow = None;
            }
        }
    }

    /// Drops slideshow pictures whose path is longer than `max` bytes.
    pub(crate) fn drop_long_slides(&mut self, max: usize) {
        if let Some(show) = &mut self.slideshow {
            show.slides.retain(|w| w.image.as_os_str().len() <= max);
        }
    }

    pub(crate) fn sanitize_optional(wallpaper: &mut Option<Self>) {
        if let Some(wall) = wallpaper {
            if wall.image.as_os_str().is_empty() {
                *wallpaper = None;
            } else {
                wall.sanitize();
            }
        }
    }

    /// Visits every picture of a wallpaper and its slideshow. A picture
    /// set to `None` leaves the slideshow; the next one takes the place of
    /// the first, and with none left the wallpaper goes.
    pub fn visit_picture(
        wallpaper: &mut Option<Self>,
        visit: &mut impl FnMut(&mut Option<PathBuf>),
    ) {
        let Some(wall) = wallpaper else {
            return;
        };
        if let Some(show) = &mut wall.slideshow {
            show.slides.retain_mut(|slide| {
                let mut image = Some(std::mem::take(&mut slide.image));
                visit(&mut image);
                match image {
                    Some(image) => {
                        slide.image = image;
                        true
                    }
                    None => false,
                }
            });
        }
        let mut image = Some(std::mem::take(&mut wall.image));
        visit(&mut image);
        match image {
            Some(image) => wall.image = image,
            // The next picture of the slideshow takes its place.
            None if wall.slide_count() > 1 => wall.remove_slide(0),
            None => *wallpaper = None,
        }
        if let Some(wall) = wallpaper
            && wall.slideshow.as_ref().is_some_and(|s| s.slides.is_empty())
        {
            wall.slideshow = None;
        }
    }
}

/// What one level of the wallpaper chain says: the profile, or a page or
/// folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperChoice<'a> {
    /// Use what the level above shows.
    Inherit,
    /// Use this picture.
    Own(&'a Wallpaper),
    /// Show no wallpaper, whatever the levels above have.
    Off,
}

impl<'a> WallpaperChoice<'a> {
    pub fn of(wallpaper: Option<&'a Wallpaper>, disabled: bool) -> Self {
        match (disabled, wallpaper) {
            (true, _) => Self::Off,
            (false, Some(wallpaper)) => Self::Own(wallpaper),
            (false, None) => Self::Inherit,
        }
    }
}

/// The level a shown wallpaper comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallpaperLevel {
    Global,
    Profile,
    /// The page or folder itself.
    Local,
}

/// The wallpaper a page or folder shows. The nearest level that says
/// something wins: the page or folder, then its profile, then the global
/// wallpaper. `Off` on a level hides the levels above it.
pub fn resolve_wallpaper<'a>(
    global: Option<&'a Wallpaper>,
    profile: WallpaperChoice<'a>,
    local: WallpaperChoice<'a>,
) -> Option<(&'a Wallpaper, WallpaperLevel)> {
    match (local, profile) {
        (WallpaperChoice::Own(w), _) => Some((w, WallpaperLevel::Local)),
        (WallpaperChoice::Off, _) | (WallpaperChoice::Inherit, WallpaperChoice::Off) => None,
        (WallpaperChoice::Inherit, WallpaperChoice::Own(w)) => Some((w, WallpaperLevel::Profile)),
        (WallpaperChoice::Inherit, WallpaperChoice::Inherit) => {
            global.map(|w| (w, WallpaperLevel::Global))
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyConfig {
    /// Action UUID, e.g. `com.deckly.system.hotkey`.
    pub action: String,
    #[serde(default)]
    pub title: String,
    /// `None` follows the global title style in the settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_style: Option<TitleStyle>,
    /// Custom key image (PNG or JPEG). `None` uses the action's icon.
    #[serde(default)]
    pub image: Option<PathBuf>,
    /// Action-specific settings.
    #[serde(default)]
    pub settings: serde_json::Value,
    /// Folder exit behavior after this key runs. `None` follows the folder's
    /// parent key; on that parent key it means automatic exit is disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_exit: Option<AutoExit>,
    /// The actions a Multi Action, Multi Action Switch, Random Action or
    /// Key Logic key holds, one list per [`group_count`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Vec<KeyConfig>>,
    /// Custom timing of an action inside a Multi Action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delays: Option<ActionDelays>,
    /// The state a many-state action inside a Multi Action asks for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desired_state: Option<u8>,
    /// Protection against accidental presses. `None` runs on a plain press.
    /// Only keys on a page or in a folder have it, never actions inside a
    /// key. A value Deckly cannot read loads as `None`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "Protection::lenient"
    )]
    pub protect: Option<Protection>,
}

/// How a protected key asks for a deliberate gesture before its action runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Protection {
    /// The action runs once the key was held down for `ms` milliseconds.
    Hold {
        #[serde(default = "Protection::default_hold")]
        ms: u32,
    },
    /// A first press arms the key; a second one within `ms` milliseconds
    /// runs the action.
    Twice {
        #[serde(default = "Protection::default_window")]
        ms: u32,
    },
    /// A press opens a confirmation screen on the device: Yes runs the
    /// action, No and Back close it. It closes by itself after `ms`
    /// milliseconds. `shuffle` places the answers anew each time.
    Confirm {
        #[serde(default = "Protection::default_timeout")]
        ms: u32,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        shuffle: bool,
    },
}

impl Protection {
    pub const HOLD_MIN_MS: u32 = 300;
    pub const HOLD_MAX_MS: u32 = 5_000;
    pub const HOLD_DEFAULT_MS: u32 = 1_000;
    pub const WINDOW_MIN_MS: u32 = 1_000;
    pub const WINDOW_MAX_MS: u32 = 10_000;
    pub const WINDOW_DEFAULT_MS: u32 = 3_000;
    pub const TIMEOUT_MIN_MS: u32 = 3_000;
    pub const TIMEOUT_MAX_MS: u32 = 30_000;
    pub const TIMEOUT_DEFAULT_MS: u32 = 10_000;

    fn default_hold() -> u32 {
        Self::HOLD_DEFAULT_MS
    }

    fn default_window() -> u32 {
        Self::WINDOW_DEFAULT_MS
    }

    fn default_timeout() -> u32 {
        Self::TIMEOUT_DEFAULT_MS
    }

    pub fn hold() -> Self {
        Self::Hold {
            ms: Self::HOLD_DEFAULT_MS,
        }
    }

    pub fn twice() -> Self {
        Self::Twice {
            ms: Self::WINDOW_DEFAULT_MS,
        }
    }

    pub fn confirm() -> Self {
        Self::Confirm {
            ms: Self::TIMEOUT_DEFAULT_MS,
            shuffle: false,
        }
    }

    /// The hold time or the window, in milliseconds.
    pub fn ms(self) -> u32 {
        match self {
            Self::Hold { ms } | Self::Twice { ms } | Self::Confirm { ms, .. } => ms,
        }
    }

    /// The bounds of [`Self::ms`] for this mode.
    pub fn range(self) -> (u32, u32) {
        match self {
            Self::Hold { .. } => (Self::HOLD_MIN_MS, Self::HOLD_MAX_MS),
            Self::Twice { .. } => (Self::WINDOW_MIN_MS, Self::WINDOW_MAX_MS),
            Self::Confirm { .. } => (Self::TIMEOUT_MIN_MS, Self::TIMEOUT_MAX_MS),
        }
    }

    /// This mode with another time, kept within its bounds.
    pub fn with_ms(self, ms: u32) -> Self {
        let (min, max) = self.range();
        let ms = ms.clamp(min, max);
        match self {
            Self::Hold { .. } => Self::Hold { ms },
            Self::Twice { .. } => Self::Twice { ms },
            Self::Confirm { shuffle, .. } => Self::Confirm { ms, shuffle },
        }
    }

    pub fn sanitize(&mut self) {
        *self = self.with_ms(self.ms());
    }

    /// Whether a key with `action` can be protected. Key Logic tells its
    /// own presses apart (single, double, hold), which a gesture in front
    /// of it would change.
    pub fn allowed(action: &str) -> bool {
        action != KEY_LOGIC
    }

    /// Reads a stored protection; anything unreadable is no protection,
    /// so one odd value never stops a whole profile from loading.
    fn lenient<'de, D>(deserializer: D) -> Result<Option<Self>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Option::<serde_json::Value>::deserialize(deserializer)?;
        Ok(value.and_then(|v| serde_json::from_value::<Self>(v).ok()))
    }
}

/// Timing of one action inside a Multi Action, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDelays {
    /// From the virtual key press to its release.
    pub press: u32,
    /// From the release to the next action.
    pub next: u32,
}

/// Automatic return to the enclosing folder after an action, in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoExit {
    pub enabled: bool,
    pub delay: u8,
}

impl Default for AutoExit {
    fn default() -> Self {
        Self {
            enabled: false,
            delay: 1,
        }
    }
}

impl AutoExit {
    pub const MAX_DELAY: u8 = 60;

    pub fn sanitize(&mut self) {
        self.delay = self.delay.min(Self::MAX_DELAY);
    }
}

impl Default for ActionDelays {
    fn default() -> Self {
        Self {
            press: Self::MAX,
            next: Self::MAX,
        }
    }
}

impl ActionDelays {
    pub const MIN: u32 = 1;
    pub const MAX: u32 = 100;

    pub fn sanitize(&mut self) {
        self.press = self.press.clamp(Self::MIN, Self::MAX);
        self.next = self.next.clamp(Self::MIN, Self::MAX);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct TitleStyle {
    pub show: bool,
    pub color: [u8; 3],
    /// Font size in points at the 72 px key size; scaled for larger keys.
    pub size: u8,
    pub align: TitleAlign,
    /// Font family as the system names it. Empty is Deckly's default font;
    /// a family that is not installed is drawn with it too.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub font: String,
    /// Titles were always bold before the font options existed, so bold is
    /// the default and old profiles look the same.
    #[serde(skip_serializing_if = "is_true")]
    pub bold: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub underline: bool,
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl TitleStyle {
    pub const MIN_SIZE: u8 = 6;
    pub const MAX_SIZE: u8 = 40;
    pub const MAX_FONT_CHARS: usize = 64;

    pub fn sanitize(&mut self) {
        self.size = self.size.clamp(Self::MIN_SIZE, Self::MAX_SIZE);
        let font = self.font.trim();
        if font.len() != self.font.len()
            || font.chars().count() > Self::MAX_FONT_CHARS
            || font.chars().any(char::is_control)
        {
            self.font = font
                .chars()
                .filter(|c| !c.is_control())
                .take(Self::MAX_FONT_CHARS)
                .collect();
        }
    }
}

impl Default for TitleStyle {
    fn default() -> Self {
        Self {
            show: true,
            color: [0xFF, 0xFF, 0xFF],
            size: 12,
            align: TitleAlign::Bottom,
            font: String::new(),
            bold: true,
            italic: false,
            underline: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleAlign {
    Top,
    Middle,
    #[default]
    Bottom,
}

impl TitleAlign {
    pub const ALL: [TitleAlign; 3] = [TitleAlign::Top, TitleAlign::Middle, TitleAlign::Bottom];

    pub fn label(self) -> &'static str {
        match self {
            TitleAlign::Top => "Top",
            TitleAlign::Middle => "Middle",
            TitleAlign::Bottom => "Bottom",
        }
    }
}

/// Reads pinned keys so that damage to them costs only the damaged keys,
/// never the profile.
fn lenient_pinned<'de, D>(deserializer: D) -> Result<Option<Page>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(pinned_from_value))
}

fn pinned_from_value(value: serde_json::Value) -> Option<Page> {
    if let Ok(page) = serde_json::from_value::<Page>(value.clone()) {
        return Some(page);
    }
    let object = value.as_object()?;
    let id = object
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned();
    let keys = object
        .get("keys")
        .and_then(|v| v.as_object())
        .into_iter()
        .flatten()
        .filter_map(|(index, key)| {
            Some((
                index.parse().ok()?,
                serde_json::from_value(key.clone()).ok()?,
            ))
        })
        .collect();
    Some(Page {
        id,
        keys,
        ..Page::default()
    })
}

/// What is wrong with the pinned keys in a profile file, if anything. The
/// profile still loads; the damaged pinned keys are left out.
pub fn pinned_damage(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let pinned = value.get("pinned").filter(|v| !v.is_null())?;
    serde_json::from_value::<Page>(pinned.clone())
        .err()
        .map(|e| format!("some pinned actions could not be read and were left out ({e})"))
}

fn format_version() -> u32 {
    FORMAT_VERSION
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Page {
    pub fn new() -> Self {
        Self {
            id: new_id(),
            ..Self::default()
        }
    }

    /// What this page or folder says about its wallpaper.
    pub fn wallpaper_choice(&self) -> WallpaperChoice<'_> {
        WallpaperChoice::of(self.wallpaper.as_ref(), self.wallpaper_disabled)
    }

    /// The name people see for the page at `index`.
    pub fn title(&self, index: usize) -> String {
        if self.name.is_empty() {
            format!("Page {}", index + 1)
        } else {
            self.name.clone()
        }
    }

    /// The short form for lists and keys: its name or its number.
    pub fn label(&self, index: usize) -> String {
        if self.name.is_empty() {
            (index + 1).to_string()
        } else {
            self.name.clone()
        }
    }
}

impl KeyConfig {
    pub fn new(action: impl Into<String>, settings: serde_json::Value) -> Self {
        let action: String = action.into();
        let groups = vec![Vec::new(); group_count(&action)];
        Self {
            action,
            title: String::new(),
            title_style: None,
            image: None,
            settings,
            auto_exit: None,
            groups,
            delays: None,
            desired_state: None,
            protect: None,
        }
    }

    /// Whether `member` can go in this key's lists of actions. Lists hold
    /// single actions only; Delay is for sequences, not Key Logic.
    pub fn can_hold(&self, member: &str) -> bool {
        group_count(&self.action) > 0
            && group_count(member) == 0
            && !matches!(member, FOLDER_ACTION | PARENT_FOLDER_ACTION)
            && (member != DELAY_ACTION || self.action != KEY_LOGIC)
    }

    /// Brings the action lists of a key from an untrusted source within
    /// limits: the right number of lists, and only actions that can be in
    /// them. Key Logic has one action per list.
    fn sanitize_groups(&mut self) {
        let lists = group_count(&self.action);
        let mut groups = std::mem::take(&mut self.groups);
        groups.resize(lists, Vec::new());
        let size = if self.action == KEY_LOGIC {
            1
        } else {
            MAX_GROUP_ACTIONS
        };
        for list in &mut groups {
            list.retain(|k| self.can_hold(&k.action));
            list.truncate(size);
            for member in list {
                member.groups.clear();
                member.protect = None;
                if let Some(delays) = &mut member.delays {
                    delays.sanitize();
                }
                sanitize_title(member);
            }
        }
        self.groups = groups;
    }

    /// The folder a folder key opens.
    pub fn folder(&self) -> Option<&str> {
        if self.action != FOLDER_ACTION {
            return None;
        }
        self.settings.get("folder").and_then(|v| v.as_str())
    }

    fn set_folder(&mut self, id: &str) {
        if !self.settings.is_object() {
            self.settings = serde_json::json!({});
        }
        if let Some(map) = self.settings.as_object_mut() {
            map.insert("folder".to_owned(), id.into());
        }
    }
}

impl Profile {
    pub fn new(name: impl Into<String>, device: DeviceKind) -> Self {
        Self {
            version: FORMAT_VERSION,
            id: new_id(),
            name: name.into(),
            device,
            wallpaper: None,
            wallpaper_disabled: false,
            pages: vec![Page::new()],
            folders: Vec::new(),
            pinned: None,
            applications: Vec::new(),
        }
    }

    /// An independent copy, including fresh ids and links to its own folders.
    /// Its applications stay with the original, so two profiles do not claim one.
    pub fn duplicate(&self, name: String) -> Self {
        let mut copy = self.clone();
        copy.id = new_id();
        copy.name = name;
        copy.applications.clear();
        let mut ids = HashMap::new();
        for page in copy.all_pages_mut() {
            let id = new_id();
            ids.insert(page.id.clone(), id.clone());
            page.id = id;
        }
        copy.for_each_key(|key| {
            if key.action == FOLDER_ACTION
                && let Some(target) = key.settings.get_mut("folder")
                && let Some(id) = target.as_str().and_then(|id| ids.get(id))
            {
                *target = id.clone().into();
            }
        });
        copy
    }

    /// A free name among profiles for the same model. Existing names are
    /// compared without case; copy suffixes never accumulate on repeated copies.
    pub fn available_name(&self, profiles: &[Profile]) -> String {
        let taken: HashSet<String> = profiles
            .iter()
            .filter(|p| p.device == self.device && p.id != self.id)
            .map(|p| p.name.trim().to_lowercase())
            .collect();
        let name: String = self
            .name
            .trim()
            .chars()
            .take(MAX_PROFILE_NAME_CHARS)
            .collect();
        if !taken.contains(&name.to_lowercase()) {
            return name;
        }
        let mut base = name.as_str();
        // Strip only whole trailing words, preserving names such as "Photocopy".
        loop {
            let without_number = base
                .rsplit_once(' ')
                .filter(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                .map_or(base, |(head, _)| head);
            let stripped = without_number
                .rsplit_once(' ')
                .filter(|(_, word)| word.eq_ignore_ascii_case("copy"));
            match stripped {
                Some((head, _)) => base = head.trim_end(),
                None => break,
            }
        }
        if !taken.contains(&base.to_lowercase()) {
            return base.to_owned();
        }
        for n in 0..=taken.len() {
            let suffix = if n == 0 {
                " copy".to_owned()
            } else {
                format!(" copy {n}")
            };
            let prefix: String = base
                .chars()
                .take(MAX_PROFILE_NAME_CHARS.saturating_sub(suffix.len()))
                .collect();
            let candidate = format!("{}{suffix}", prefix.trim_end());
            if !taken.contains(&candidate.to_lowercase()) {
                return candidate;
            }
        }
        // There are more candidates than occupied names, so the loop always returns.
        name
    }

    pub fn folder(&self, id: &str) -> Option<&Page> {
        self.folders.iter().find(|f| f.id == id)
    }

    /// Appends an empty page unless the profile has reached the page limit.
    pub fn add_page(&mut self) -> Option<String> {
        if self.pages.len() >= MAX_PAGES {
            return None;
        }
        let page = Page::new();
        let id = page.id.clone();
        self.pages.push(page);
        Some(id)
    }

    /// Deletes a regular page and its unreachable folders, keeping at least one page.
    /// Returns the removed page's former index.
    pub fn remove_page(&mut self, id: &str) -> Option<usize> {
        if self.pages.len() <= 1 {
            return None;
        }
        let index = self.pages.iter().position(|page| page.id == id)?;
        self.pages.remove(index);
        self.normalize_folders();
        Some(index)
    }

    pub fn folder_mut(&mut self, id: &str) -> Option<&mut Page> {
        self.folders.iter_mut().find(|f| f.id == id)
    }

    /// What the profile says about the wallpaper of its pages and folders.
    pub fn wallpaper_choice(&self) -> WallpaperChoice<'_> {
        WallpaperChoice::of(self.wallpaper.as_ref(), self.wallpaper_disabled)
    }

    /// The wallpaper `page`, one of this profile's pages or folders, shows
    /// under `global`, and where it comes from.
    pub fn wallpaper_of<'a>(
        &'a self,
        page: &'a Page,
        global: Option<&'a Wallpaper>,
    ) -> Option<(&'a Wallpaper, WallpaperLevel)> {
        resolve_wallpaper(global, self.wallpaper_choice(), page.wallpaper_choice())
    }

    /// Visits every key on pages and in folders, and every action a key holds.
    pub fn for_each_key(&mut self, mut visit: impl FnMut(&mut KeyConfig)) {
        for page in self.all_pages_mut() {
            for key in page.keys.values_mut() {
                visit(key);
                key.groups.iter_mut().flatten().for_each(&mut visit);
            }
        }
    }

    /// Visits every picture the profile points at, including wallpapers.
    pub fn for_each_picture(&mut self, mut visit: impl FnMut(&mut Option<PathBuf>)) {
        Wallpaper::visit_picture(&mut self.wallpaper, &mut visit);
        for page in self.all_pages_mut() {
            visit(&mut page.icon);
            Wallpaper::visit_picture(&mut page.wallpaper, &mut visit);
        }
        self.for_each_key(|key| visit(&mut key.image));
    }

    /// Whether this is still the empty profile made for a device that had
    /// none. Nothing is lost when one goes.
    pub fn is_placeholder(&self) -> bool {
        self.name == DEFAULT_PROFILE_NAME
            && self.folders.is_empty()
            && self.pinned.is_none()
            && self.applications.is_empty()
            && self.wallpaper.is_none()
            && !self.wallpaper_disabled
            && matches!(self.pages.as_slice(), [page]
                if page.keys.is_empty()
                    && page.name.is_empty()
                    && page.icon.is_none()
                    && page.wallpaper.is_none()
                    && !page.wallpaper_disabled)
    }

    /// Gives every folder key a folder of its own and drops folders no key
    /// leads to. A folder key whose folder is missing gets a new empty one;
    /// a folder that a second key leads to (a pasted key) is copied for that
    /// key, with the folders inside it; a key leading into one of its own
    /// enclosing folders gets an empty one. Every folder keeps a parent
    /// folder key. Past the folder limits a folder key opens nothing.
    pub fn normalize_folders(&mut self) {
        let key_count = self.model().key_count();
        let mut walk = FolderWalk::default();
        let mut order = Vec::new();
        for folder in std::mem::take(&mut self.folders) {
            if !walk.original.contains_key(&folder.id) {
                order.push(folder.id.clone());
                walk.original.insert(folder.id.clone(), folder);
            }
        }
        for page in self.pages.iter_mut().chain(&mut self.pinned) {
            walk.resolve(page, &[], key_count);
        }
        // Folders keep their place in the file; new ones go last.
        walk.done.sort_by_key(|f| {
            order
                .iter()
                .position(|id| *id == f.id)
                .unwrap_or(usize::MAX)
        });
        self.folders = walk.done;
    }

    /// Every page of keys: the pages, the pinned keys, the folders.
    pub fn all_pages(&self) -> impl Iterator<Item = &Page> {
        self.pages.iter().chain(&self.pinned).chain(&self.folders)
    }

    pub fn all_pages_mut(&mut self) -> impl Iterator<Item = &mut Page> {
        self.pages
            .iter_mut()
            .chain(&mut self.pinned)
            .chain(&mut self.folders)
    }

    /// A page, the pinned keys or a folder, by id.
    pub fn any_page(&self, id: &str) -> Option<&Page> {
        self.all_pages().find(|p| p.id == id)
    }

    pub fn any_page_mut(&mut self, id: &str) -> Option<&mut Page> {
        self.all_pages_mut().find(|p| p.id == id)
    }

    /// Whether `id` is the page of pinned keys.
    pub fn is_pinned_page(&self, id: &str) -> bool {
        self.pinned.as_ref().is_some_and(|p| p.id == id)
    }

    /// The key at `index` on `page`, one of this profile's pages, folders or
    /// its pinned keys, as the device and the editor show it: the page's own
    /// key, else the key pinned there. Everything that shows or runs a key
    /// goes through here, so they never disagree.
    pub fn resolve<'a>(&'a self, page: &'a Page, index: u8) -> Option<ShownKey<'a>> {
        if let Some(key) = page.keys.get(&index) {
            return Some(ShownKey {
                page,
                key,
                pinned: self.is_pinned_page(&page.id),
            });
        }
        let pinned = self.pinned.as_ref().filter(|p| p.id != page.id)?;
        Some(ShownKey {
            page: pinned,
            key: pinned.keys.get(&index)?,
            pinned: true,
        })
    }

    /// Every key `page` shows, by index; see [`Profile::resolve`].
    pub fn shown_keys<'a>(&'a self, page: &'a Page) -> BTreeMap<u8, ShownKey<'a>> {
        let pinned = self.pinned.iter().flat_map(|p| p.keys.keys());
        page.keys
            .keys()
            .chain(pinned)
            .filter_map(|&i| Some((i, self.resolve(page, i)?)))
            .collect()
    }

    /// Moves the key at `index` of a page or folder to the pinned keys, where
    /// it shows on every page and folder that has no key of its own there.
    pub fn pin(&mut self, page_id: &str, index: u8) -> Result<(), PinError> {
        if self.is_pinned_page(page_id) {
            return Err(PinError::AlreadyPinned);
        }
        let page = self
            .pages
            .iter()
            .chain(&self.folders)
            .find(|p| p.id == page_id)
            .ok_or(PinError::Gone)?;
        let key = page.keys.get(&index).ok_or(PinError::Empty)?;
        if key.action == PARENT_FOLDER_ACTION {
            return Err(PinError::ParentFolder);
        }
        if self.pinned_key(index).is_some() {
            return Err(PinError::Taken);
        }
        let key = self
            .any_page_mut(page_id)
            .and_then(|p| p.keys.remove(&index))
            .ok_or(PinError::Empty)?;
        self.pinned
            .get_or_insert_with(Page::new)
            .keys
            .insert(index, key);
        Ok(())
    }

    /// Moves the key pinned at `index` onto the page or folder `page_id`,
    /// which must have no key of its own there. The other pages and folders
    /// no longer show it.
    pub fn unpin(&mut self, index: u8, page_id: &str) -> Result<(), PinError> {
        if self.is_pinned_page(page_id) {
            return Err(PinError::Gone);
        }
        let target = self
            .pages
            .iter()
            .chain(&self.folders)
            .find(|p| p.id == page_id)
            .ok_or(PinError::Gone)?;
        if target.keys.contains_key(&index) {
            return Err(PinError::Taken);
        }
        let key = self
            .pinned
            .as_mut()
            .and_then(|p| p.keys.remove(&index))
            .ok_or(PinError::Empty)?;
        if let Some(target) = self.any_page_mut(page_id) {
            target.keys.insert(index, key);
        }
        if self.pinned.as_ref().is_some_and(|p| p.keys.is_empty()) {
            self.pinned = None;
        }
        Ok(())
    }

    /// The key pinned at `index`.
    pub fn pinned_key(&self, index: u8) -> Option<&KeyConfig> {
        self.pinned.as_ref()?.keys.get(&index)
    }

    /// Pinned keys are a set of keys, not a page: no name, picture or
    /// wallpaper, no parent folder key, and an id of their own.
    fn sanitize_pinned(&mut self) {
        let taken: HashSet<String> = self
            .pages
            .iter()
            .chain(&self.folders)
            .map(|p| p.id.clone())
            .collect();
        let Some(pinned) = &mut self.pinned else {
            return;
        };
        if pinned.id.is_empty() || taken.contains(&pinned.id) {
            pinned.id = new_id();
        }
        pinned.name.clear();
        pinned.icon = None;
        pinned.wallpaper = None;
        pinned.wallpaper_disabled = false;
        pinned.keys.retain(|_, k| k.action != PARENT_FOLDER_ACTION);
        if pinned.keys.is_empty() {
            self.pinned = None;
        }
    }

    pub fn model(&self) -> &'static DeviceModel {
        DeviceModel::of(self.device)
    }

    /// Brings a profile from an untrusted source within limits so the rest
    /// of the app can rely on them.
    pub fn sanitize(&mut self) {
        let key_count = self.model().key_count();
        if self.wallpaper_disabled {
            self.wallpaper = None;
        }
        Wallpaper::sanitize_optional(&mut self.wallpaper);
        self.pages.truncate(MAX_PAGES);
        if self.pages.is_empty() {
            self.pages.push(Page::new());
        }
        self.folders.truncate(MAX_FOLDERS);
        self.sanitize_pinned();
        for page in self.all_pages_mut() {
            if page.id.is_empty() {
                page.id = new_id();
            }
            page.name = page.name.trim().chars().take(MAX_PAGE_NAME_CHARS).collect();
            page.keys.retain(|&index, _| index < key_count);
            if page.wallpaper_disabled {
                page.wallpaper = None;
            }
            Wallpaper::sanitize_optional(&mut page.wallpaper);
            for key in page.keys.values_mut() {
                sanitize_title(key);
                key.sanitize_groups();
            }
        }
        if self.pinned.as_ref().is_some_and(|p| p.keys.is_empty()) {
            self.pinned = None;
        }
        self.normalize_folders();
        if self.id.is_empty() {
            self.id = new_id();
        }
        self.applications = std::mem::take(&mut self.applications)
            .iter()
            .map(|rule| AppRule::new(&rule.name, &rule.id).with_icon(&rule.icon))
            .filter(|rule| !rule.id.is_empty())
            .take(MAX_PROFILE_APPLICATIONS)
            .collect();
    }

    pub fn from_json(text: &str) -> Result<Self, ProfileError> {
        let mut profile: Profile = serde_json::from_str(text).map_err(ProfileError::Parse)?;
        if profile.version > FORMAT_VERSION {
            return Err(ProfileError::NewerFormat(profile.version));
        }
        profile.sanitize();
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<String, ProfileError> {
        serde_json::to_string_pretty(self).map_err(ProfileError::Parse)
    }

    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        Self::load_checked(path).map(|(profile, _)| profile)
    }

    /// Loads a profile and says what was wrong with its pinned keys.
    pub fn load_checked(path: &Path) -> Result<(Self, Option<String>), ProfileError> {
        let len = std::fs::metadata(path).map_err(ProfileError::Io)?.len();
        if len > MAX_FILE_BYTES {
            return Err(ProfileError::TooLarge(len));
        }
        let text = std::fs::read_to_string(path).map_err(ProfileError::Io)?;
        let profile = Self::from_json(&text)?;
        Ok((profile, pinned_damage(&text)))
    }

    /// Writes through a temporary file so a crash never leaves a half-written
    /// profile behind.
    pub fn save(&self, path: &Path) -> Result<(), ProfileError> {
        let text = self.to_json()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(ProfileError::Io)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(ProfileError::Io)?;
        std::fs::rename(&tmp, path).map_err(ProfileError::Io)
    }
}

fn sanitize_title(key: &mut KeyConfig) {
    if let Some(auto_exit) = &mut key.auto_exit {
        auto_exit.sanitize();
    }
    if !Protection::allowed(&key.action) {
        key.protect = None;
    }
    if let Some(protect) = &mut key.protect {
        protect.sanitize();
    }
    if key.title.chars().count() > MAX_TITLE_CHARS {
        key.title = key.title.chars().take(MAX_TITLE_CHARS).collect();
    }
    if let Some(style) = &mut key.title_style {
        style.sanitize();
    }
    // Before the global title style existed every key stored one; an
    // untouched default style now follows the global one.
    if key.title_style == Some(TitleStyle::default()) {
        key.title_style = None;
    }
}

#[derive(Default)]
struct FolderWalk {
    /// The folders as they were before the walk, by id.
    original: HashMap<String, Page>,
    /// Original folders already given to a key.
    used: HashSet<String>,
    done: Vec<Page>,
}

impl FolderWalk {
    /// Resolves the folder keys on a page inside `ancestors`, the original
    /// ids of the folders around it, outermost first.
    fn resolve(&mut self, page: &mut Page, ancestors: &[String], key_count: u8) {
        for key in page.keys.values_mut() {
            if key.action != FOLDER_ACTION {
                continue;
            }
            if ancestors.len() >= MAX_FOLDER_DEPTH || self.done.len() >= MAX_FOLDERS {
                key.set_folder("");
                continue;
            }
            let target = key.folder().unwrap_or_default().to_owned();
            let source = self
                .original
                .get(&target)
                .filter(|_| !ancestors.contains(&target))
                .cloned();
            let mut folder = match source {
                Some(f) if self.used.insert(target.clone()) => f,
                Some(mut copy) => {
                    copy.id = new_id();
                    copy
                }
                None => Page::new(),
            };
            key.set_folder(&folder.id);
            let mut inner = ancestors.to_vec();
            inner.push(target);
            self.resolve(&mut folder, &inner, key_count);
            ensure_parent_key(&mut folder, key_count);
            self.done.push(folder);
        }
    }
}

/// Puts a parent folder key in a folder that has none: top left when that
/// key is free, else on the first free key, else over the top-left key.
fn ensure_parent_key(folder: &mut Page, key_count: u8) {
    if folder
        .keys
        .values()
        .any(|k| k.action == PARENT_FOLDER_ACTION)
    {
        return;
    }
    let slot = (0..key_count)
        .find(|i| !folder.keys.contains_key(i))
        .unwrap_or(0);
    folder.keys.insert(
        slot,
        KeyConfig::new(PARENT_FOLDER_ACTION, serde_json::json!({})),
    );
}

/// A key as a page or folder shows it; see [`Profile::resolve`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShownKey<'a> {
    /// Where the key is kept: the page itself, or the pinned keys. Plugins
    /// know the key by this page's id, so a pinned key stays one instance
    /// on every page that shows it.
    pub page: &'a Page,
    pub key: &'a KeyConfig,
    /// The key is pinned.
    pub pinned: bool,
}

/// Why a key cannot be pinned or unpinned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinError {
    /// The page or folder is no longer there.
    Gone,
    /// There is no key at that place.
    Empty,
    /// The key is pinned already.
    AlreadyPinned,
    /// A parent folder key belongs to its folder.
    ParentFolder,
    /// Another key is at that place already.
    Taken,
}

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Gone => "This page is no longer there",
            Self::Empty => "There is no action at this place",
            Self::AlreadyPinned => "This action is pinned already",
            Self::ParentFolder => "The Parent Folder key stays in its folder",
            Self::Taken => "Another action is already at this place",
        })
    }
}

impl std::error::Error for PinError {}

#[derive(Debug)]
pub enum ProfileError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    TooLarge(u64),
    NewerFormat(u32),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Io(e) => write!(f, "{e}"),
            ProfileError::Parse(e) => write!(f, "invalid profile: {e}"),
            ProfileError::TooLarge(n) => write!(f, "profile file is too large ({n} bytes)"),
            ProfileError::NewerFormat(v) => {
                write!(f, "profile was saved by a newer Deckly (format {v})")
            }
        }
    }
}

impl std::error::Error for ProfileError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn protection_round_trips_and_old_keys_load_unprotected() {
        let legacy: KeyConfig = serde_json::from_value(json!({
            "action": "com.example.action", "settings": {}
        }))
        .unwrap();
        assert_eq!(legacy.protect, None);
        // An unprotected key writes nothing new.
        assert!(
            serde_json::to_value(&legacy)
                .unwrap()
                .get("protect")
                .is_none()
        );
        let mut profile = Profile::new("Safe", DeviceKind::Mk2);
        for (index, protect) in [
            None,
            Some(Protection::hold()),
            Some(Protection::Hold { ms: 2_500 }),
            Some(Protection::twice()),
            Some(Protection::confirm()),
            Some(Protection::Confirm {
                ms: 4_000,
                shuffle: true,
            }),
        ]
        .into_iter()
        .enumerate()
        {
            let mut key = legacy.clone();
            key.protect = protect;
            profile.pages[0].keys.insert(index as u8, key);
        }
        let json = profile.to_json().unwrap();
        assert!(json.contains(r#""mode": "hold""#));
        assert!(json.contains(r#""mode": "twice""#));
        assert!(json.contains(r#""mode": "confirm""#));
        // Shuffle off is the default and is not written.
        assert_eq!(json.matches(r#""shuffle""#).count(), 1);
        assert_eq!(Profile::from_json(&json).unwrap(), profile);
    }

    #[test]
    fn stored_protection_is_clamped_and_never_breaks_a_profile() {
        let protect = |value: serde_json::Value| {
            let text = json!({
                "id": "p", "name": "P", "device": "mk2",
                "pages": [{"id": "a", "keys": {"0": {
                    "action": "com.example.action", "protect": value
                }}}]
            })
            .to_string();
            Profile::from_json(&text).unwrap().pages[0].keys[&0].protect
        };
        assert_eq!(
            protect(json!({"mode": "hold", "ms": 10})),
            Some(Protection::Hold {
                ms: Protection::HOLD_MIN_MS
            })
        );
        assert_eq!(
            protect(json!({"mode": "hold", "ms": 999_999_999})),
            Some(Protection::Hold {
                ms: Protection::HOLD_MAX_MS
            })
        );
        assert_eq!(
            protect(json!({"mode": "twice", "ms": 0})),
            Some(Protection::Twice {
                ms: Protection::WINDOW_MIN_MS
            })
        );
        assert_eq!(protect(json!({"mode": "hold"})), Some(Protection::hold()));
        assert_eq!(protect(json!({"mode": "twice"})), Some(Protection::twice()));
        assert_eq!(
            protect(json!({"mode": "confirm"})),
            Some(Protection::confirm())
        );
        assert_eq!(
            protect(json!({"mode": "confirm", "ms": 1, "shuffle": true})),
            Some(Protection::Confirm {
                ms: Protection::TIMEOUT_MIN_MS,
                shuffle: true
            })
        );
        assert_eq!(protect(json!({"mode": "confirm", "shuffle": "yes"})), None);
        // Unknown or malformed values load as no protection.
        assert_eq!(protect(json!({"mode": "triple"})), None);
        assert_eq!(protect(json!({"mode": "hold", "ms": -5})), None);
        assert_eq!(protect(json!("hold")), None);
        assert_eq!(protect(json!(null)), None);
    }

    #[test]
    fn only_keys_that_allow_it_keep_protection() {
        let mut profile = Profile::new("Safe", DeviceKind::Mk2);
        let mut logic = KeyConfig::new(KEY_LOGIC, json!({}));
        logic.protect = Some(Protection::hold());
        let mut multi = KeyConfig::new(MULTI_ACTION, json!({}));
        multi.protect = Some(Protection::twice());
        let mut member = KeyConfig::new("com.example.action", json!({}));
        member.protect = Some(Protection::hold());
        multi.groups[0].push(member);
        profile.pages[0].keys.insert(0, logic);
        profile.pages[0].keys.insert(1, multi);
        profile.sanitize();
        let keys = &profile.pages[0].keys;
        assert_eq!(keys[&0].protect, None);
        assert_eq!(keys[&1].protect, Some(Protection::twice()));
        assert_eq!(keys[&1].groups[0][0].protect, None);
    }

    #[test]
    fn protection_times_stay_in_their_mode_bounds() {
        assert_eq!(Protection::hold().with_ms(0).ms(), Protection::HOLD_MIN_MS);
        assert_eq!(
            Protection::twice().with_ms(u32::MAX).ms(),
            Protection::WINDOW_MAX_MS
        );
        assert_eq!(Protection::hold().with_ms(1_500).ms(), 1_500);
        assert!(Protection::allowed(MULTI_ACTION));
        assert!(!Protection::allowed(KEY_LOGIC));
    }

    #[test]
    fn auto_exit_defaults_to_inheritance_and_preserves_explicit_settings() {
        let legacy: KeyConfig = serde_json::from_value(json!({
            "action": "com.example.action", "settings": {}
        }))
        .unwrap();
        assert_eq!(legacy.auto_exit, None);
        assert!(
            serde_json::to_value(&legacy)
                .unwrap()
                .get("auto_exit")
                .is_none()
        );
        assert_eq!(
            AutoExit::default(),
            AutoExit {
                enabled: false,
                delay: 1
            }
        );

        let mut profile = Profile::new("Scenes", DeviceKind::Mk2);
        for (index, auto_exit) in [
            None,
            Some(AutoExit {
                enabled: false,
                delay: 9,
            }),
            Some(AutoExit {
                enabled: true,
                delay: 0,
            }),
            Some(AutoExit {
                enabled: true,
                delay: 60,
            }),
        ]
        .into_iter()
        .enumerate()
        {
            let mut key = legacy.clone();
            key.auto_exit = auto_exit;
            profile.pages[0].keys.insert(index as u8, key);
        }
        assert_eq!(
            Profile::from_json(&profile.to_json().unwrap()).unwrap(),
            profile
        );
    }

    #[test]
    fn auto_exit_delays_are_bounded_on_keys_and_group_members() {
        let mut profile = Profile::new("Scenes", DeviceKind::Mk2);
        let mut key = KeyConfig::new(MULTI_ACTION, json!({}));
        key.auto_exit = Some(AutoExit {
            enabled: true,
            delay: 255,
        });
        let mut member = KeyConfig::new("com.example.action", json!({}));
        member.auto_exit = Some(AutoExit {
            enabled: false,
            delay: 101,
        });
        key.groups[0].push(member);
        profile.pages[0].keys.insert(1, key);

        let loaded = Profile::from_json(&profile.to_json().unwrap()).unwrap();
        let key = &loaded.pages[0].keys[&1];
        assert_eq!(
            key.auto_exit,
            Some(AutoExit {
                enabled: true,
                delay: 60
            })
        );
        assert_eq!(
            key.groups[0][0].auto_exit,
            Some(AutoExit {
                enabled: false,
                delay: 60
            })
        );
    }

    #[test]
    fn copied_and_normalized_folders_keep_auto_exit_preferences() {
        let mut profile = Profile::new("Scenes", DeviceKind::Mk2);
        profile.pages[0]
            .keys
            .insert(1, KeyConfig::new(FOLDER_ACTION, json!({})));
        profile.normalize_folders();
        let folder = &mut profile.folders[0];
        folder.keys.get_mut(&0).unwrap().auto_exit = Some(AutoExit {
            enabled: true,
            delay: 3,
        });
        let mut member = KeyConfig::new("com.example.action", json!({}));
        member.auto_exit = Some(AutoExit {
            enabled: false,
            delay: 12,
        });
        folder.keys.insert(1, member);
        let expected = folder.keys.clone();

        let copy = profile.duplicate("Copy".into());
        assert_eq!(copy.folders[0].keys, expected);
        // Pasting the folder key makes an independent folder with the same settings.
        let pasted = profile.pages[0].keys[&1].clone();
        profile.pages[0].keys.insert(2, pasted);
        profile.normalize_folders();
        assert_eq!(profile.folders.len(), 2);
        assert!(profile.folders.iter().all(|folder| folder.keys == expected));
    }

    #[test]
    fn copies_keep_pages_actions_and_their_own_folder_links() {
        let mut source = Profile::new("Scenes", DeviceKind::Mk2);
        source.add_page();
        source.pages[0].name = "Main".into();
        source.pages[0].icon = Some("main.svg".into());
        source.pages[0]
            .keys
            .insert(0, KeyConfig::new(FOLDER_ACTION, serde_json::json!({})));
        source.normalize_folders();
        source.folders[0].keys.insert(
            1,
            KeyConfig::new("com.example.action", serde_json::json!({"value": 42})),
        );
        let copy = source.duplicate("Scenes copy".into());
        assert_ne!(copy.id, source.id);
        assert_eq!(copy.name, "Scenes copy");
        assert_eq!(copy.pages.len(), 2);
        for (a, b) in copy.pages.iter().zip(&source.pages) {
            assert_ne!(a.id, b.id);
            assert_eq!((&a.name, &a.icon), (&b.name, &b.icon));
        }
        assert_ne!(copy.folders[0].id, source.folders[0].id);
        assert_eq!(
            copy.pages[0].keys[&0].folder(),
            Some(copy.folders[0].id.as_str())
        );
        assert_eq!(copy.folders[0].keys, source.folders[0].keys);
        assert_eq!(Profile::from_json(&copy.to_json().unwrap()).unwrap(), copy);
    }

    #[test]
    fn free_names_use_copy_suffixes_and_respect_model_case_and_length() {
        let mut profile = Profile::new("Scenes", DeviceKind::Mk2);
        let mut existing = vec![Profile::new("Scenes", DeviceKind::Mini)];
        assert_eq!(profile.available_name(&existing), "Scenes");
        existing.push(Profile::new("SCENES", DeviceKind::Mk2));
        assert_eq!(profile.available_name(&existing), "Scenes copy");
        existing.push(Profile::new("Scenes copy", DeviceKind::Mk2));
        existing.push(Profile::new("Scenes copy 2", DeviceKind::Mk2));
        profile.name = "Scenes copy 2".into();
        assert_eq!(profile.available_name(&existing), "Scenes copy 1");
        let same_id = profile.clone();
        assert_eq!(profile.available_name(&[same_id]), "Scenes copy 2");
        assert_eq!(
            profile.available_name(&[Profile::new("Scenes copy 2", DeviceKind::Mk2)]),
            "Scenes"
        );
        profile.name = "Ж".repeat(MAX_PROFILE_NAME_CHARS);
        existing.push(Profile::new(profile.name.clone(), DeviceKind::Mk2));
        let name = profile.available_name(&existing);
        assert_eq!(name.chars().count(), MAX_PROFILE_NAME_CHARS);
        assert!(name.ends_with(" copy"));
        profile.name = "Photocopy".into();
        existing.push(Profile::new("Photocopy", DeviceKind::Mk2));
        assert_eq!(profile.available_name(&existing), "Photocopy copy");
    }

    #[test]
    fn adding_pages_preserves_existing_pages_and_stops_at_the_limit() {
        let mut profile = Profile::new("Test", DeviceKind::Mk2);
        profile.pages[0]
            .keys
            .insert(1, KeyConfig::new("com.example.a", serde_json::json!({})));
        let original = profile.pages[0].clone();
        let id = profile.add_page().unwrap();
        assert_eq!(profile.pages[0], original);
        assert_eq!(profile.pages[1].id, id);
        assert!(profile.pages[1].keys.is_empty());
        while profile.add_page().is_some() {}
        assert_eq!(profile.pages.len(), MAX_PAGES);
        assert!(profile.add_page().is_none());
    }

    #[test]
    fn deleting_pages_removes_their_folders_but_preserves_the_last_page() {
        let mut profile = Profile::new("Test", DeviceKind::Mk2);
        let first = profile.pages[0].id.clone();
        let second = profile.add_page().unwrap();
        for page in &mut profile.pages {
            page.keys
                .insert(1, KeyConfig::new(FOLDER_ACTION, json!({})));
        }
        profile.normalize_folders();
        let remaining_folder = profile.pages[1].keys[&1].folder().unwrap().to_owned();
        assert_eq!(profile.remove_page("missing"), None);
        assert_eq!(profile.remove_page(&first), Some(0));
        assert_eq!(profile.pages[0].id, second);
        assert_eq!(profile.folders.len(), 1);
        assert_eq!(profile.folders[0].id, remaining_folder);
        let before = profile.clone();
        assert_eq!(profile.remove_page(&second), None);
        assert_eq!(profile, before);
        assert_eq!(
            Profile::from_json(&profile.to_json().unwrap()).unwrap(),
            profile
        );
    }
    #[test]
    fn round_trip() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        let mut key = KeyConfig::new(
            "com.deckly.system.website",
            json!({"url": "https://example.com"}),
        );
        key.title = "Site".into();
        p.pages[0].keys.insert(3, key);
        p.applications.push(AppRule::new("Spotify", "Spotify.exe"));
        p.applications
            .push(AppRule::new("Game", "game.exe").with_icon(r"C:\Games\game.exe"));
        let text = p.to_json().unwrap();
        assert_eq!(Profile::from_json(&text).unwrap(), p);
        assert_eq!(text.matches("\"icon\"").count(), 1);
    }

    #[test]
    fn older_profile_files_list_no_applications_and_new_empty_ones_stay_small() {
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p"}]}"#;
        let p = Profile::from_json(text).unwrap();
        assert!(p.applications.is_empty());
        assert!(!p.to_json().unwrap().contains("applications"));
    }

    #[test]
    fn a_rule_matches_by_the_last_part_of_a_name_in_any_case() {
        let rule = AppRule::new("Discord", r"C:\Users\me\AppData\Local\Discord\Discord.exe");
        assert_eq!(rule.id, "Discord.exe");
        let running = r"C:\Users\me\AppData\Local\Discord\app-1.0.9\discord.EXE";
        assert!(rule.matches(&["other".into(), running.into()]));
        assert!(!rule.matches(&[r"C:\Discord.exe.bak".into()]));
        assert!(!rule.matches(&[]));

        let package = AppRule::new("Calculator", "Microsoft.Calc_8wek!App");
        assert!(package.matches(&["microsoft.calc_8wek!app".into()]));
        let bundle = AppRule::new("Music", "/Applications/Music.app/");
        assert_eq!(bundle.id, "Music.app");
        assert!(bundle.matches(&["/System/Applications/Music.app".into()]));
        assert!(!bundle.matches(&["com.apple.Music".into()]));
    }

    #[test]
    fn a_rule_without_a_name_shows_its_id() {
        assert_eq!(AppRule::new("  ", "/usr/bin/gimp").name, "gimp");
    }

    #[test]
    fn sanitize_bounds_applications_and_drops_empty_ones() {
        let long = "x".repeat(1000);
        let huge = "x".repeat(MAX_APPLICATION_ICON_BYTES + 1);
        let rules: Vec<String> = (0..=MAX_PROFILE_APPLICATIONS)
            .map(|i| format!(r#"{{"name":"{long}","id":"/a/b/{i}.exe","icon":"{huge}"}}"#))
            .collect();
        let text = format!(
            r#"{{"id":"a","name":"x","device":"mk2","pages":[],
            "applications":[{{"name":"none","id":" / "}},{}]}}"#,
            rules.join(",")
        );
        let p = Profile::from_json(&text).unwrap();
        assert_eq!(p.applications.len(), MAX_PROFILE_APPLICATIONS);
        assert_eq!(p.applications[0].id, "0.exe");
        assert!(p.applications[0].icon.is_empty());
        assert_eq!(
            p.applications[0].name.chars().count(),
            MAX_APPLICATION_CHARS
        );
    }

    #[test]
    fn a_copy_does_not_claim_the_applications_of_its_original() {
        let mut p = Profile::new("Game", DeviceKind::Mk2);
        p.applications.push(AppRule::new("Game", "game.exe"));
        assert!(p.duplicate("Copy".into()).applications.is_empty());
    }

    #[test]
    fn sanitize_drops_out_of_range_keys_and_adds_page() {
        let text = r#"{"id":"","name":"x","device":"mk2","pages":[]}"#;
        let p = Profile::from_json(text).unwrap();
        assert_eq!(p.pages.len(), 1);
        assert!(!p.id.is_empty());

        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{
            "14":{"action":"a"},"15":{"action":"b"},"200":{"action":"c"}}}]}"#;
        let p = Profile::from_json(text).unwrap();
        assert_eq!(
            p.pages[0].keys.keys().copied().collect::<Vec<_>>(),
            vec![14]
        );
    }

    #[test]
    fn wallpapers_round_trip_and_old_pages_have_none() {
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{}}]}"#;
        let mut p = Profile::from_json(text).unwrap();
        assert_eq!(p.pages[0].wallpaper, None);
        assert!(!p.to_json().unwrap().contains("wallpaper"));
        p.pages[0].wallpaper = Some(Wallpaper {
            zoom: 250,
            x: 100,
            y: 900,
            ..Wallpaper::new("images/a/wall.png".into())
        });
        let back = Profile::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn slideshows_round_trip_and_old_wallpapers_have_none() {
        let text = r#"{"id":"a","name":"x","device":"mk2",
            "wallpaper":{"image":"w.png","zoom":150},"pages":[{"id":"p","keys":{}}]}"#;
        let mut p = Profile::from_json(text).unwrap();
        let wall = p.wallpaper.as_mut().unwrap();
        assert_eq!(wall.slideshow, None);
        assert_eq!(wall.slide_count(), 1);
        assert!(!p.to_json().unwrap().contains("slideshow"));
        let wall = p.wallpaper.as_mut().unwrap();
        assert!(wall.push_slide(Wallpaper {
            zoom: 300,
            ..Wallpaper::new("second.png".into())
        }));
        let show = wall.slideshow.as_mut().unwrap();
        show.interval = 90;
        show.transition = Transition::Push;
        show.duration = 1500;
        let json = p.to_json().unwrap();
        assert!(json.contains(r#""transition": "push""#));
        let back = Profile::from_json(&json).unwrap();
        assert_eq!(back, p);
        let wall = back.wallpaper.unwrap();
        assert_eq!(wall.slide_count(), 2);
        assert_eq!(wall.slide(1).unwrap().zoom, 300);
        // Missing fields take their defaults.
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{},
            "wallpaper":{"image":"w.png","slideshow":{"slides":[{"image":"b.png"}]}}}]}"#;
        let p = Profile::from_json(text).unwrap();
        let show = p.pages[0]
            .wallpaper
            .as_ref()
            .unwrap()
            .slideshow
            .clone()
            .unwrap();
        assert_eq!(
            show,
            Slideshow {
                slides: show.slides.clone(),
                ..Slideshow::new()
            }
        );
    }

    #[test]
    fn slideshows_are_brought_within_limits() {
        let slides: Vec<String> = (0..100)
            .map(|i| format!(r#"{{"image":"s{i}.png","zoom":9999,"slideshow":{{"slides":[{{"image":"x.png"}}]}}}}"#))
            .chain([r#"{"image":""}"#.to_owned()])
            .collect();
        let text = format!(
            r#"{{"id":"a","name":"x","device":"mk2","pages":[{{"id":"p","keys":{{}}}}],
            "wallpaper":{{"image":"w.png","slideshow":{{"slides":[{}],"interval":0,"duration":99999}}}}}}"#,
            slides.join(",")
        );
        let p = Profile::from_json(&text).unwrap();
        let wall = p.wallpaper.unwrap();
        assert_eq!(wall.slide_count(), Slideshow::MAX_PICTURES);
        let show = wall.slideshow.as_ref().unwrap();
        assert_eq!(show.interval, Slideshow::MIN_INTERVAL);
        assert_eq!(show.duration, Slideshow::MAX_DURATION);
        assert!(
            show.slides
                .iter()
                .all(|s| s.slideshow.is_none() && s.zoom == Wallpaper::MAX_ZOOM)
        );
        // A slideshow with no pictures of its own is no slideshow.
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{}}],
            "wallpaper":{"image":"w.png","slideshow":{"slides":[{"image":""}],"interval":99999999}}}"#;
        let p = Profile::from_json(text).unwrap();
        assert_eq!(p.wallpaper.unwrap().slideshow, None);
    }

    #[test]
    fn slides_move_and_go_while_the_first_picture_stays_the_wallpaper() {
        let names = |w: &Wallpaper| -> Vec<String> {
            (0..w.slide_count())
                .map(|i| w.slide(i).unwrap().image.display().to_string())
                .collect()
        };
        let mut w = Wallpaper::new("a".into());
        w.push_slide(Wallpaper::new("b".into()));
        w.push_slide(Wallpaper {
            zoom: 200,
            ..Wallpaper::new("c".into())
        });
        w.swap_slides(0, 2);
        assert_eq!(names(&w), ["c", "b", "a"]);
        assert_eq!(w.zoom, 200);
        w.swap_slides(1, 2);
        assert_eq!(names(&w), ["c", "a", "b"]);
        w.swap_slides(0, 9);
        assert_eq!(names(&w), ["c", "a", "b"]);
        w.remove_slide(0);
        assert_eq!(names(&w), ["a", "b"]);
        assert_eq!(w.zoom, Wallpaper::MIN_ZOOM);
        w.remove_slide(1);
        assert_eq!(names(&w), ["a"]);
        assert_eq!(w.slideshow, None);
        w.remove_slide(0);
        assert_eq!(names(&w), ["a"]);
        for i in 1..Slideshow::MAX_PICTURES {
            assert!(w.push_slide(Wallpaper::new(format!("{i}").into())));
        }
        assert!(!w.push_slide(Wallpaper::new("full".into())));
    }

    #[test]
    fn a_missing_first_slide_gives_way_to_the_next() {
        let mut p = Profile::new("x", DeviceKind::Mk2);
        let mut w = Wallpaper::new("gone.png".into());
        w.push_slide(Wallpaper::new("kept.png".into()));
        w.push_slide(Wallpaper::new("gone.png".into()));
        p.wallpaper = Some(w);
        p.for_each_picture(|pic| {
            if pic.as_deref() == Some(Path::new("gone.png")) {
                *pic = None;
            }
        });
        let w = p.wallpaper.unwrap();
        assert_eq!(w.image, Path::new("kept.png"));
        assert_eq!(w.slideshow, None);
    }

    #[test]
    fn profile_wallpaper_respects_page_and_folder_overrides_after_saving() {
        let mut p = Profile::new(DEFAULT_PROFILE_NAME, DeviceKind::Mk2);
        let default = Wallpaper::new("default.png".into());
        let local = Wallpaper::new("folder.png".into());
        p.wallpaper = Some(default.clone());
        assert!(!p.is_placeholder());
        p.add_page();
        p.pages[1].wallpaper_disabled = true;
        p.pages[0]
            .keys
            .insert(0, KeyConfig::new(FOLDER_ACTION, serde_json::json!({})));
        p.normalize_folders();
        let shown = |p: &Profile, page: &Page| p.wallpaper_of(page, None).map(|(w, _)| w.clone());
        assert_eq!(shown(&p, &p.folders[0]), Some(default.clone()));
        p.folders[0].wallpaper = Some(local.clone());
        let p = Profile::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(shown(&p, &p.pages[0]), Some(default));
        assert_eq!(shown(&p, &p.pages[1]), None);
        assert_eq!(shown(&p, &p.folders[0]), Some(local));
        let mut disabled = p.folders[0].clone();
        disabled.wallpaper_disabled = true;
        assert_eq!(shown(&p, &disabled), None);
    }

    #[test]
    fn the_nearest_wallpaper_setting_wins() {
        use WallpaperChoice::{Inherit, Off, Own};
        use WallpaperLevel::{Global, Local, Profile as FromProfile};
        let g = Wallpaper::new("global.png".into());
        let p = Wallpaper::new("profile.png".into());
        let l = Wallpaper::new("local.png".into());
        let profiles = [Inherit, Own(&p), Off];
        let locals = [Inherit, Own(&l), Off];
        for global in [None, Some(&g)] {
            for profile in profiles {
                for local in locals {
                    let expected = match (local, profile) {
                        (Own(w), _) => Some((w, Local)),
                        (Off, _) | (Inherit, Off) => None,
                        (Inherit, Own(w)) => Some((w, FromProfile)),
                        (Inherit, Inherit) => global.map(|w| (w, Global)),
                    };
                    assert_eq!(
                        resolve_wallpaper(global, profile, local),
                        expected,
                        "global {global:?}, profile {profile:?}, local {local:?}"
                    );
                }
            }
        }
        // Spelled out: inheriting all the way up reaches the global picture.
        assert_eq!(
            resolve_wallpaper(Some(&g), Inherit, Inherit),
            Some((&g, Global))
        );
        // A profile that opts out hides the global picture, but its pages
        // can still have their own.
        assert_eq!(resolve_wallpaper(Some(&g), Off, Inherit), None);
        assert_eq!(resolve_wallpaper(Some(&g), Off, Own(&l)), Some((&l, Local)));
        // A page that opts out hides both levels above it.
        assert_eq!(resolve_wallpaper(Some(&g), Own(&p), Off), None);
        // Without a global picture, inheriting shows nothing.
        assert_eq!(resolve_wallpaper(None, Inherit, Inherit), None);
    }

    #[test]
    fn profiles_follow_the_global_wallpaper_unless_they_opt_out() {
        let global = Wallpaper::new("global.png".into());
        let mut p = Profile::new("x", DeviceKind::Mk2);
        p.add_page();
        p.pages[1].wallpaper = Some(Wallpaper::new("own.png".into()));
        let level = |p: &Profile, i: usize| p.wallpaper_of(&p.pages[i], Some(&global)).map(|w| w.1);
        assert_eq!(level(&p, 0), Some(WallpaperLevel::Global));
        assert_eq!(level(&p, 1), Some(WallpaperLevel::Local));
        p.wallpaper_disabled = true;
        // Saved and loaded back, the opt-out holds.
        let text = p.to_json().unwrap();
        assert!(text.contains(r#""wallpaper_disabled": true"#));
        let p = Profile::from_json(&text).unwrap();
        assert!(p.wallpaper_disabled);
        assert_eq!(level(&p, 0), None);
        assert_eq!(level(&p, 1), Some(WallpaperLevel::Local));
        assert!(!p.is_placeholder());
    }

    #[test]
    fn older_profiles_follow_the_global_wallpaper() {
        let text = r#"{"id":"a","name":"x","device":"mk2",
            "wallpaper":{"image":"w.png"},"pages":[{"id":"p","keys":{}}]}"#;
        let p = Profile::from_json(text).unwrap();
        assert!(!p.wallpaper_disabled);
        assert_eq!(
            p.wallpaper_choice(),
            WallpaperChoice::Own(&Wallpaper::new("w.png".into()))
        );
        // An opt-out leaves no picture behind on load.
        let text = r#"{"id":"a","name":"x","device":"mk2","wallpaper_disabled":true,
            "wallpaper":{"image":"w.png"},"pages":[{"id":"p","keys":{}}]}"#;
        let p = Profile::from_json(text).unwrap();
        assert_eq!(p.wallpaper, None);
        assert_eq!(p.wallpaper_choice(), WallpaperChoice::Off);
        let fresh = Profile::new("x", DeviceKind::Mk2);
        assert!(!fresh.to_json().unwrap().contains("wallpaper"));
    }

    #[test]
    fn wallpapers_are_brought_within_limits() {
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[
            {"id":"p","wallpaper":{"image":"w.png","zoom":9999,"x":5000,"y":7}},
            {"id":"q","wallpaper":{"image":"w.png"}},
            {"id":"r","wallpaper":{"image":"","zoom":200}}]}"#;
        let p = Profile::from_json(text).unwrap();
        let w = p.pages[0].wallpaper.as_ref().unwrap();
        assert_eq!((w.zoom, w.x, w.y), (Wallpaper::MAX_ZOOM, Wallpaper::END, 7));
        assert_eq!(p.pages[1].wallpaper, Some(Wallpaper::new("w.png".into())));
        assert_eq!(p.pages[2].wallpaper, None);
        let zero = r#"{"id":"a","name":"x","device":"mk2","pages":[
            {"id":"p","wallpaper":{"image":"w.png","zoom":0}}]}"#;
        let p = Profile::from_json(zero).unwrap();
        assert_eq!(
            p.pages[0].wallpaper.as_ref().unwrap().zoom,
            Wallpaper::MIN_ZOOM
        );
    }

    #[test]
    fn wallpapers_are_pictures_of_the_profile() {
        let mut p = Profile::new("Walls", DeviceKind::Mk2);
        p.pages[0].wallpaper = Some(Wallpaper::new("a.png".into()));
        p.add_page();
        p.pages[1].wallpaper = Some(Wallpaper::new("b.png".into()));
        let mut seen = Vec::new();
        p.for_each_picture(|picture| {
            if let Some(path) = picture.take() {
                seen.push(path.clone());
                // Pictures that go away take their wallpaper with them.
                *picture = (path != Path::new("b.png")).then(|| path.with_extension("jpg"));
            }
        });
        assert_eq!(seen, [PathBuf::from("a.png"), PathBuf::from("b.png")]);
        assert_eq!(
            p.pages[0].wallpaper.as_ref().map(|w| w.image.clone()),
            Some(PathBuf::from("a.jpg"))
        );
        assert_eq!(p.pages[1].wallpaper, None);
    }

    #[test]
    fn default_title_styles_follow_the_global_style() {
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{
            "0":{"action":"a","title_style":{"show":true,"color":[255,255,255],"size":12,"align":"bottom"}},
            "1":{"action":"b","title_style":{"size":99}},
            "2":{"action":"c"}}}]}"#;
        let p = Profile::from_json(text).unwrap();
        let keys = &p.pages[0].keys;
        assert_eq!(keys[&0].title_style, None);
        assert_eq!(
            keys[&1].title_style.as_ref().map(|s| s.size),
            Some(TitleStyle::MAX_SIZE)
        );
        assert_eq!(keys[&2].title_style, None);
        assert!(!p.to_json().unwrap().contains("\"title_style\": null"));
    }

    #[test]
    fn title_styles_without_font_options_keep_their_look() {
        let text = r#"{"show":false,"color":[1,2,3],"size":20,"align":"top"}"#;
        let style: TitleStyle = serde_json::from_str(text).unwrap();
        assert_eq!(style.font, "");
        assert!(style.bold);
        assert!(!style.italic && !style.underline);
        // Untouched font options are not written, so the file is unchanged.
        let back: serde_json::Value = serde_json::to_value(&style).unwrap();
        assert_eq!(
            back,
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );
    }

    #[test]
    fn font_options_round_trip() {
        let mut p = Profile::new("Fonts", DeviceKind::Mk2);
        let mut key = KeyConfig::new("a", serde_json::json!({}));
        key.title_style = Some(TitleStyle {
            font: "DejaVu Serif".into(),
            bold: false,
            italic: true,
            underline: true,
            ..TitleStyle::default()
        });
        p.pages[0].keys.insert(0, key);
        let back = Profile::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn font_names_are_trimmed_and_limited() {
        let long = "F".repeat(TitleStyle::MAX_FONT_CHARS + 10);
        let mut style = TitleStyle {
            font: format!("  {long}\n"),
            ..TitleStyle::default()
        };
        style.sanitize();
        assert_eq!(style.font, "F".repeat(TitleStyle::MAX_FONT_CHARS));
        style.font = " Noto\u{7}Sans ".into();
        style.sanitize();
        assert_eq!(style.font, "NotoSans");
        style.font = "Noto Sans".into();
        style.sanitize();
        assert_eq!(style.font, "Noto Sans");
    }

    fn folder_key(id: &str) -> KeyConfig {
        KeyConfig::new(FOLDER_ACTION, json!({ "folder": id }))
    }

    #[test]
    fn new_folder_keys_get_a_folder_with_a_parent_key() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].keys.insert(4, folder_key(""));
        p.normalize_folders();
        let id = p.pages[0].keys[&4].folder().unwrap().to_owned();
        let folder = p.folder(&id).unwrap();
        assert_eq!(folder.keys[&0].action, PARENT_FOLDER_ACTION);
        // Normalizing again changes nothing.
        let before = p.clone();
        p.normalize_folders();
        assert_eq!(p, before);
        // The folder survives a save and load.
        assert_eq!(Profile::from_json(&p.to_json().unwrap()).unwrap(), p);
    }

    #[test]
    fn unreachable_folders_are_dropped() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].keys.insert(0, folder_key(""));
        p.normalize_folders();
        assert_eq!(p.folders.len(), 1);
        p.pages[0].keys.clear();
        p.normalize_folders();
        assert!(p.folders.is_empty());
        assert!(!p.to_json().unwrap().contains("folders"));
    }

    #[test]
    fn a_pasted_folder_key_gets_a_deep_copy() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].keys.insert(0, folder_key(""));
        p.normalize_folders();
        let outer = p.pages[0].keys[&0].folder().unwrap().to_owned();
        p.folder_mut(&outer).unwrap().keys.insert(5, folder_key(""));
        p.normalize_folders();
        assert_eq!(p.folders.len(), 2);
        // Paste the outer folder key on another key.
        let copy = p.pages[0].keys[&0].clone();
        p.pages[0].keys.insert(1, copy);
        p.normalize_folders();
        assert_eq!(p.folders.len(), 4);
        let a = p.pages[0].keys[&0].folder().unwrap();
        let b = p.pages[0].keys[&1].folder().unwrap();
        assert_ne!(a, b);
        let inner = |id: &str| p.folder(id).unwrap().keys[&5].folder().unwrap().to_owned();
        assert_ne!(inner(a), inner(b));
    }

    #[test]
    fn folder_cycles_and_missing_folders_are_repaired() {
        // Folder "a" holds a key leading back to itself, and "b" is missing.
        let text = r#"{"id":"x","name":"x","device":"mk2",
            "pages":[{"id":"p","keys":{"0":{"action":"com.deckly.navigation.folder","settings":{"folder":"a"}},
                                      "1":{"action":"com.deckly.navigation.folder","settings":{"folder":"b"}}}}],
            "folders":[{"id":"a","keys":{"3":{"action":"com.deckly.navigation.folder","settings":{"folder":"a"}},
                                         "99":{"action":"junk"}}}]}"#;
        let p = Profile::from_json(text).unwrap();
        assert_eq!(p.folders.len(), 3);
        let a = p.folder("a").unwrap();
        assert_eq!(a.keys[&0].action, PARENT_FOLDER_ACTION);
        assert!(!a.keys.contains_key(&99));
        let inner = a.keys[&3].folder().unwrap();
        assert_ne!(inner, "a");
        assert!(p.folder(inner).is_some());
        assert!(p.folder(p.pages[0].keys[&1].folder().unwrap()).is_some());
    }

    #[test]
    fn folder_nesting_is_bounded() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].keys.insert(0, folder_key(""));
        // Nest one level deeper on each pass.
        for _ in 0..MAX_FOLDER_DEPTH + 5 {
            p.normalize_folders();
            for folder in &mut p.folders {
                folder.keys.entry(1).or_insert_with(|| folder_key(""));
            }
        }
        p.normalize_folders();
        assert_eq!(p.folders.len(), MAX_FOLDER_DEPTH);
    }

    #[test]
    fn group_keys_round_trip_with_their_actions() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        let mut key = KeyConfig::new(MULTI_ACTION_SWITCH, json!({}));
        assert_eq!(key.groups.len(), 2);
        let mut member = KeyConfig::new("com.example.a", json!({"x": 1}));
        member.delays = Some(ActionDelays { press: 1, next: 50 });
        member.desired_state = Some(1);
        key.groups[1].push(member);
        p.pages[0].keys.insert(0, key);
        p.pages[0]
            .keys
            .insert(1, KeyConfig::new("com.example.b", json!({})));
        let text = p.to_json().unwrap();
        assert_eq!(Profile::from_json(&text).unwrap(), p);
        // Plain keys stay as small as before.
        assert_eq!(text.matches("\"groups\"").count(), 1);
        assert!(!text.contains("delays\": null"));
    }

    #[test]
    fn group_lists_are_repaired() {
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{
            "0":{"action":"com.deckly.multi.action","groups":[
                [{"action":"com.deckly.multi.action"},
                 {"action":"com.deckly.navigation.folder"},
                 {"action":"com.deckly.navigation.parent-folder"},
                 {"action":"com.deckly.multi.delay","groups":[[{"action":"x"}]],
                  "delays":{"press":0,"next":99999}}],
                [{"action":"extra"}]]},
            "1":{"action":"com.deckly.multi.key-logic","groups":[
                [{"action":"a"},{"action":"b"}],
                [{"action":"com.deckly.multi.delay"}]]},
            "2":{"action":"com.deckly.multi.switch"},
            "3":{"action":"plain","groups":[[{"action":"x"}]]}}}]}"#;
        let p = Profile::from_json(text).unwrap();
        let keys = &p.pages[0].keys;
        assert_eq!(keys[&0].groups.len(), 1);
        let only = &keys[&0].groups[0];
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].action, DELAY_ACTION);
        assert!(only[0].groups.is_empty());
        assert_eq!(
            only[0].delays,
            Some(ActionDelays {
                press: ActionDelays::MIN,
                next: ActionDelays::MAX
            })
        );
        // Key Logic has one action per press and no pauses.
        assert_eq!(keys[&1].groups.len(), 3);
        assert_eq!(keys[&1].groups[0].len(), 1);
        assert!(keys[&1].groups[1].is_empty());
        assert_eq!(keys[&2].groups.len(), 2);
        assert!(keys[&3].groups.is_empty());
    }

    #[test]
    fn group_lists_are_bounded() {
        let mut key = KeyConfig::new(MULTI_ACTION, json!({}));
        key.groups[0] = vec![KeyConfig::new("a", json!({})); MAX_GROUP_ACTIONS + 5];
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].keys.insert(0, key);
        p.sanitize();
        assert_eq!(p.pages[0].keys[&0].groups[0].len(), MAX_GROUP_ACTIONS);
    }

    #[test]
    fn pages_keep_their_name_and_icon() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].name = "Scenes".into();
        p.pages[0].icon = Some(PathBuf::from("icon.svg"));
        p.pages.push(Page::new());
        let back = Profile::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.pages[0].title(0), "Scenes");
        assert_eq!(back.pages[1].title(1), "Page 2");
        assert_eq!(back.pages[0].label(0), "Scenes");
        assert_eq!(back.pages[1].label(1), "2");

        p.pages[0].name = format!("  {}  ", "x".repeat(MAX_PAGE_NAME_CHARS + 5));
        p.sanitize();
        assert_eq!(p.pages[0].name.chars().count(), MAX_PAGE_NAME_CHARS);
    }

    #[test]
    fn only_an_untouched_default_profile_is_a_placeholder() {
        let new = || Profile::new(DEFAULT_PROFILE_NAME, DeviceKind::Mk2);
        assert!(new().is_placeholder());
        assert!(!Profile::new("Streaming", DeviceKind::Mk2).is_placeholder());
        let mut with_key = new();
        with_key.pages[0]
            .keys
            .insert(0, KeyConfig::new("a", json!({})));
        assert!(!with_key.is_placeholder());
        let mut with_page = new();
        with_page.pages.push(Page::new());
        assert!(!with_page.is_placeholder());
        let mut named = new();
        named.pages[0].name = "Scenes".into();
        assert!(!named.is_placeholder());
        let mut following = new();
        following
            .applications
            .push(AppRule::new("Game", "game.exe"));
        assert!(!following.is_placeholder());
    }

    #[test]
    fn pictures_of_pages_and_keys_are_visited() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        p.pages[0].icon = Some(PathBuf::from("page.svg"));
        let mut key = KeyConfig::new("a", json!({}));
        key.image = Some(PathBuf::from("key.png"));
        p.pages[0].keys.insert(0, key);
        p.for_each_picture(|picture| {
            *picture = picture.take().map(|path| Path::new("new").join(path));
        });
        assert_eq!(p.pages[0].icon, Some(Path::new("new").join("page.svg")));
        assert_eq!(
            p.pages[0].keys[&0].image,
            Some(Path::new("new").join("key.png"))
        );
    }

    #[test]
    fn every_key_is_visited() {
        let mut p = Profile::new("Test", DeviceKind::Mk2);
        let mut multi = KeyConfig::new(MULTI_ACTION, json!({}));
        multi.groups[0].push(KeyConfig::new("a", json!({})));
        p.pages[0].keys.insert(0, multi);
        p.pages[0].keys.insert(1, folder_key(""));
        p.normalize_folders();
        let mut seen = 0;
        p.for_each_key(|_| seen += 1);
        // The Multi Action, its action, the folder key and the parent key.
        assert_eq!(seen, 4);
    }

    #[test]
    fn rejects_newer_format() {
        let text = r#"{"version":99,"id":"a","name":"x","device":"mk2","pages":[]}"#;
        assert!(matches!(
            Profile::from_json(text),
            Err(ProfileError::NewerFormat(99))
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(Profile::from_json("not json").is_err());
        assert!(
            Profile::from_json(r#"{"id":"a","name":"x","device":"toaster","pages":[]}"#).is_err()
        );
    }
    fn hotkey(title: &str) -> KeyConfig {
        let mut key = KeyConfig::new("com.deckly.system.hotkey", json!({}));
        key.title = title.into();
        key
    }

    /// A profile with two pages and a folder on the first one.
    fn pinning_profile() -> Profile {
        let mut p = Profile::new("P", DeviceKind::Mk2);
        p.add_page();
        p.pages[0]
            .keys
            .insert(4, KeyConfig::new(FOLDER_ACTION, json!({})));
        p.pages[0].keys.insert(2, hotkey("mute"));
        p.normalize_folders();
        p
    }

    fn shown_title(p: &Profile, page: &Page, index: u8) -> Option<String> {
        p.resolve(page, index).map(|k| k.key.title.clone())
    }

    #[test]
    fn a_pinned_key_shows_on_every_page_and_folder_without_a_key_there() {
        let mut p = pinning_profile();
        let page = p.pages[0].id.clone();
        p.pin(&page, 2).unwrap();
        assert!(!p.pages[0].keys.contains_key(&2));
        let pinned = p.pinned.clone().unwrap();
        for page in p.pages.iter().chain(&p.folders) {
            let shown = p.resolve(page, 2).unwrap();
            assert!(shown.pinned);
            assert_eq!(shown.page.id, pinned.id, "one instance everywhere");
            assert_eq!(shown.key.title, "mute");
        }
        assert_eq!(p.resolve(&pinned, 2).map(|k| k.pinned), Some(true));
        assert_eq!(p.resolve(&p.pages[1], 3), None);
    }

    #[test]
    fn a_key_of_its_own_wins_over_the_pin_on_that_page_only() {
        let mut p = pinning_profile();
        let page = p.pages[0].id.clone();
        p.pin(&page, 2).unwrap();
        p.pages[1].keys.insert(2, hotkey("local"));
        assert_eq!(shown_title(&p, &p.pages[1], 2).as_deref(), Some("local"));
        assert!(!p.resolve(&p.pages[1], 2).unwrap().pinned);
        assert_eq!(shown_title(&p, &p.pages[0], 2).as_deref(), Some("mute"));
        let folder = p.folders[0].clone();
        assert_eq!(shown_title(&p, &folder, 2).as_deref(), Some("mute"));
        // Clearing the local key shows the pin again.
        p.pages[1].keys.remove(&2);
        assert_eq!(shown_title(&p, &p.pages[1], 2).as_deref(), Some("mute"));
    }

    #[test]
    fn the_parent_folder_key_wins_and_cannot_be_pinned() {
        let mut p = pinning_profile();
        let folder = p.folders[0].id.clone();
        let parent = *p.folders[0]
            .keys
            .iter()
            .find(|(_, k)| k.action == PARENT_FOLDER_ACTION)
            .unwrap()
            .0;
        assert_eq!(p.pin(&folder, parent), Err(PinError::ParentFolder));
        p.pages[1].keys.insert(parent, hotkey("pin"));
        let page = p.pages[1].id.clone();
        p.pin(&page, parent).unwrap();
        let shown = p.resolve(&p.folders[0], parent).unwrap();
        assert_eq!(shown.key.action, PARENT_FOLDER_ACTION);
    }

    #[test]
    fn pinning_refuses_a_taken_place_and_unpin_lands_where_asked() {
        let mut p = pinning_profile();
        let (first, second) = (p.pages[0].id.clone(), p.pages[1].id.clone());
        p.pin(&first, 2).unwrap();
        p.pages[1].keys.insert(2, hotkey("other"));
        assert_eq!(p.pin(&second, 2), Err(PinError::Taken));
        assert_eq!(p.unpin(2, &second), Err(PinError::Taken));
        assert_eq!(p.pin(&first, 9), Err(PinError::Empty));
        let pinned = p.pinned.as_ref().unwrap().id.clone();
        assert_eq!(p.pin(&pinned, 2), Err(PinError::AlreadyPinned));
        let folder = p.folders[0].id.clone();
        p.unpin(2, &folder).unwrap();
        assert_eq!(p.pinned, None, "the last unpin leaves nothing pinned");
        assert_eq!(p.folders[0].keys[&2].title, "mute");
        assert_eq!(shown_title(&p, &p.pages[0], 2), None);
        assert_eq!(p.unpin(2, &folder), Err(PinError::Taken));
        assert_eq!(p.unpin(3, &first), Err(PinError::Empty));
    }

    #[test]
    fn a_pinned_folder_keeps_its_folder() {
        let mut p = pinning_profile();
        let page = p.pages[0].id.clone();
        let folder = p.pages[0].keys[&4].folder().unwrap().to_owned();
        p.pin(&page, 4).unwrap();
        p.normalize_folders();
        assert_eq!(p.pinned_key(4).and_then(|k| k.folder()), Some(&*folder));
        assert!(p.folder(&folder).is_some());
        let copy = p.duplicate("Copy".into());
        let pinned = copy.pinned.as_ref().unwrap();
        assert_ne!(pinned.id, p.pinned.as_ref().unwrap().id);
        let opened = pinned.keys[&4].folder().unwrap();
        assert!(copy.folder(opened).is_some());
        assert_ne!(opened, folder);
    }

    #[test]
    fn pins_belong_to_their_profile() {
        let mut a = pinning_profile();
        let page = a.pages[0].id.clone();
        a.pin(&page, 2).unwrap();
        let b = Profile::new("B", DeviceKind::Mk2);
        assert_eq!(b.resolve(&b.pages[0], 2), None);
        // Another model keeps only the places it has.
        let mut json: serde_json::Value = serde_json::from_str(&a.to_json().unwrap()).unwrap();
        json["device"] = json!("mini");
        json["pinned"]["keys"]["14"] = serde_json::to_value(hotkey("far")).unwrap();
        let mini = Profile::from_json(&json.to_string()).unwrap();
        let pinned = mini.pinned.unwrap();
        assert!(pinned.keys.keys().all(|&k| k < 6));
        assert!(pinned.keys.contains_key(&2));
    }

    #[test]
    fn pinned_keys_round_trip_and_old_files_load_unchanged() {
        let mut p = pinning_profile();
        let page = p.pages[0].id.clone();
        p.pin(&page, 2).unwrap();
        let back = Profile::from_json(&p.to_json().unwrap()).unwrap();
        assert_eq!(back, p);
        let old = r#"{"version":1,"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{"0":{"action":"com.deckly.system.hotkey"}}}]}"#;
        let old = Profile::from_json(old).unwrap();
        assert_eq!(old.pinned, None);
        assert!(!old.to_json().unwrap().contains("pinned"));
    }

    #[test]
    fn damaged_pinned_keys_cost_only_themselves() {
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p","keys":{}}],
            "pinned":{"id":"pin","name":5,"keys":{"1":{"action":"com.deckly.system.hotkey"},"2":{"title":3},"x":{},"0":{"action":"com.deckly.navigation.parent-folder"}}}}"#;
        let p = Profile::from_json(text).unwrap();
        let pinned = p.pinned.as_ref().unwrap();
        assert_eq!(pinned.keys.keys().copied().collect::<Vec<_>>(), [1]);
        assert!(pinned_damage(text).is_some());
        for broken in ["5", "\"x\"", "[]", "{}", r#"{"keys":7}"#] {
            let text =
                format!(r#"{{"id":"a","name":"x","device":"mk2","pages":[],"pinned":{broken}}}"#);
            let p = Profile::from_json(&text).unwrap();
            assert_eq!(p.pinned, None, "{broken}");
            assert_eq!(p.pages.len(), 1);
        }
        assert_eq!(pinned_damage(r#"{"pinned":null}"#), None);
        // Pinned keys never share an id with a page.
        let text = r#"{"id":"a","name":"x","device":"mk2","pages":[{"id":"p"}],
            "pinned":{"id":"p","keys":{"1":{"action":"com.deckly.system.hotkey"}}}}"#;
        let p = Profile::from_json(text).unwrap();
        assert_ne!(p.pinned.unwrap().id, "p");
    }

    #[test]
    fn shown_keys_merge_own_and_pinned_keys() {
        let mut p = pinning_profile();
        let page = p.pages[0].id.clone();
        p.pin(&page, 2).unwrap();
        p.pages[1].keys.insert(7, hotkey("own"));
        let shown = p.shown_keys(&p.pages[1]);
        assert_eq!(shown.keys().copied().collect::<Vec<_>>(), [2, 7]);
        assert!(shown[&2].pinned && !shown[&7].pinned);
    }
}
