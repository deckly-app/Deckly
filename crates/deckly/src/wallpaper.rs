//! The Wallpaper window: choose a picture for the profile, page or folder
//! and place it across the keys by dragging and zooming
//! (docs/clean-room/2026-11-04-key-wallpaper.md), or several pictures
//! that follow each other as a slideshow
//! (docs/clean-room/2026-11-22-wallpaper-slideshow.md).

use crate::render::{self, Renderer};
use crate::store;
use crate::theme::*;
use deckly_core::{
    DeviceModel, Page, Slideshow, Transition, Wallpaper, WallpaperChoice, WallpaperLevel,
    resolve_wallpaper,
};
use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, Rect, RichText, Sense, Stroke, TextureHandle,
    TextureOptions, Vec2, pos2, vec2,
};
use egui_phosphor::regular as icon;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Width of the picture of the keys, in points.
const PREVIEW_WIDTH: f32 = 380.0;
/// Zoom change for one notch of the mouse wheel, in percent.
const WHEEL_ZOOM: f32 = 0.25;
const GAP_SHADE: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 150);

/// The levels a page's or folder's wallpaper comes from, as the window
/// edits them. The global wallpaper is only shown here; Preferences > Style
/// edits it.
pub struct Layers<'a> {
    pub global: Option<&'a Wallpaper>,
    pub profile: &'a mut Option<Wallpaper>,
    pub profile_disabled: &'a mut bool,
    pub page: &'a mut Page,
}

/// Where chosen pictures are kept.
enum Owner {
    /// In the profile's images folder.
    Profile(String),
    /// In Deckly's wallpapers folder, for the global wallpaper.
    Global,
}

pub struct WallpaperEditor {
    owner: Owner,
    pub profile: String,
    pub page: String,
    editing_profile: bool,
    job: Option<Receiver<Result<PathBuf, String>>>,
    /// The picture being read joins the slideshow instead of replacing the
    /// one selected.
    adding: bool,
    /// The slideshow picture shown and edited, 0 for the wallpaper's own.
    slide: usize,
    error: Option<String>,
    preview: Option<(u64, TextureHandle)>,
    /// Counts presses, so that each drag or slide is one undo step.
    gesture: u64,
}

impl WallpaperEditor {
    pub fn new(profile: &str, page: &str) -> Self {
        Self {
            owner: Owner::Profile(profile.to_owned()),
            profile: profile.to_owned(),
            page: page.to_owned(),
            editing_profile: false,
            job: None,
            adding: false,
            slide: 0,
            error: None,
            preview: None,
            gesture: 0,
        }
    }

    /// An editor for the global wallpaper of Preferences > Style.
    pub fn global() -> Self {
        Self {
            owner: Owner::Global,
            ..Self::new("", "")
        }
    }

    /// Shows the window over `layers.page`. Returns `None` once it is
    /// closed, else the undo group of this frame's changes.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        layers: Layers<'_>,
        scope: &str,
        model: &DeviceModel,
        renderer: &mut Renderer,
    ) -> Option<u64> {
        let Layers {
            global,
            profile: profile_wallpaper,
            profile_disabled,
            page,
        } = layers;
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.gesture = self.gesture.wrapping_add(1);
        }
        if let Some(path) = self.receive() {
            if self.editing_profile {
                self.take(profile_wallpaper, path);
                *profile_disabled = false;
            } else {
                self.take(&mut page.wallpaper, path);
                page.wallpaper_disabled = false;
            }
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new("Wallpaper")
            .id(egui::Id::new(("wallpaper", &self.page)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(PREVIEW_WIDTH);
                let busy = self.job.is_some();
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        let was = self.editing_profile;
                        ui.selectable_value(&mut self.editing_profile, false, scope);
                        ui.selectable_value(&mut self.editing_profile, true, "Profile");
                        if was != self.editing_profile {
                            self.slide = 0;
                        }
                    });
                });
                let profile_choice =
                    WallpaperChoice::of(profile_wallpaper.as_ref(), *profile_disabled);
                let (shown, wallpaper) = if self.editing_profile {
                    let shown = resolve_wallpaper(global, profile_choice, WallpaperChoice::Inherit);
                    (shown.map(|(w, l)| (w.clone(), l)), &mut *profile_wallpaper)
                } else {
                    let shown = resolve_wallpaper(global, profile_choice, page.wallpaper_choice());
                    (shown.map(|(w, l)| (w.clone(), l)), &mut page.wallpaper)
                };
                let level = shown.as_ref().map(|(_, level)| *level);
                let shown = shown.map(|(w, _)| w);
                self.edit(ui, ctx, shown.as_ref(), wallpaper, model, renderer, busy);
                let status = if self.editing_profile {
                    profile_status(*profile_disabled, level)
                } else {
                    local_status(page.wallpaper_disabled, level)
                };
                ui.label(RichText::new(status).color(TEXT_DIM));
                ui.horizontal(|ui| {
                    let (inherits, off, inherit_label) = if self.editing_profile {
                        (
                            !*profile_disabled && profile_wallpaper.is_none(),
                            *profile_disabled,
                            "Use Global",
                        )
                    } else {
                        (
                            !page.wallpaper_disabled && page.wallpaper.is_none(),
                            page.wallpaper_disabled,
                            "Use Profile",
                        )
                    };
                    let inherit = ui
                        .add_enabled(!busy && !inherits, egui::Button::new(inherit_label))
                        .clicked();
                    let disable = ui
                        .add_enabled(!busy && !off, egui::Button::new("No Wallpaper"))
                        .clicked();
                    if inherit || disable {
                        if self.editing_profile {
                            *profile_wallpaper = None;
                            *profile_disabled = disable;
                        } else {
                            page.wallpaper = None;
                            page.wallpaper_disabled = disable;
                        }
                        self.error = None;
                    }
                });
                self.error_row(ui);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    self.choose_button(ui, ctx, busy);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui.button("Done").clicked();
                    });
                });
            });
        if !open || close {
            return None;
        }
        Some(egui::Id::new(("wallpaper-edit", &self.page, self.gesture)).value())
    }

    /// Shows the global wallpaper's window, with the keys of `model`.
    /// Returns whether it is still open.
    pub fn show_global(
        &mut self,
        ctx: &egui::Context,
        wallpaper: &mut Option<Wallpaper>,
        model: &mut &'static DeviceModel,
        renderer: &mut Renderer,
    ) -> bool {
        if let Some(path) = self.receive() {
            self.take(wallpaper, path);
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new("Global Wallpaper")
            .id(egui::Id::new("global-wallpaper"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(PREVIEW_WIDTH);
                let busy = self.job.is_some();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Preview on").color(TEXT_DIM));
                    let layouts = DeviceModel::layouts();
                    let current = layouts
                        .iter()
                        .find(|g| g.iter().any(|m| m.same_layout(model)));
                    egui::ComboBox::from_id_salt("global-wallpaper-model")
                        .selected_text(
                            current.map_or_else(|| model.name.to_owned(), |g| layout_label(g)),
                        )
                        .show_ui(ui, |ui| {
                            for group in &layouts {
                                let Some(first) = group.first() else {
                                    continue;
                                };
                                let names: Vec<&str> = group.iter().map(|m| m.name).collect();
                                let chosen = first.same_layout(model);
                                if ui
                                    .selectable_label(chosen, layout_label(group))
                                    .on_hover_text(names.join("\n"))
                                    .clicked()
                                {
                                    *model = first;
                                }
                            }
                        });
                });
                ui.add_space(4.0);
                let shown = wallpaper.clone();
                self.edit(ui, ctx, shown.as_ref(), wallpaper, model, renderer, busy);
                let status = if wallpaper.is_some() {
                    "Behind the keys wherever a profile, page or folder does not use its own \
                     wallpaper or turn it off."
                } else {
                    "No global wallpaper. Profiles, pages and folders can still have their own."
                };
                ui.label(RichText::new(status).color(TEXT_DIM));
                self.error_row(ui);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    self.choose_button(ui, ctx, busy);
                    if ui
                        .add_enabled(
                            wallpaper.is_some() && !busy,
                            egui::Button::new(format!("{}  Remove", icon::TRASH)),
                        )
                        .clicked()
                    {
                        *wallpaper = None;
                        self.slide = 0;
                        self.error = None;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = ui.button("Done").clicked();
                    });
                });
            });
        open && !close
    }

    /// The preview of the selected picture, its zoom, and the slideshow.
    /// `shown` is what the keys show, `wallpaper` the level's own, if any.
    #[allow(clippy::too_many_arguments)]
    fn edit(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        shown: Option<&Wallpaper>,
        wallpaper: &mut Option<Wallpaper>,
        model: &DeviceModel,
        renderer: &mut Renderer,
        busy: bool,
    ) {
        let count = wallpaper.as_ref().map_or(1, Wallpaper::slide_count);
        self.slide = self.slide.min(count - 1);
        // An inherited wallpaper shows its first picture.
        let picture = match wallpaper {
            Some(own) => own.slide(self.slide).cloned(),
            None => shown.cloned(),
        };
        let editable = wallpaper.as_mut().and_then(|w| w.slide_mut(self.slide));
        self.preview(ui, picture.as_ref(), editable, model, renderer);
        ui.add_space(8.0);
        if let Some(wallpaper) = wallpaper {
            if let Some(slide) = wallpaper.slide_mut(self.slide) {
                zoom_row(ui, slide);
            }
            ui.add_space(4.0);
            self.slides_row(ui, ctx, wallpaper, busy);
            if let Some(show) = &mut wallpaper.slideshow {
                slideshow_rows(ui, show);
            }
            ui.add_space(4.0);
        }
    }

    /// The pictures of the slideshow: pick one to edit, move it, take it
    /// out, or add another.
    fn slides_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        wallpaper: &mut Wallpaper,
        busy: bool,
    ) {
        let count = wallpaper.slide_count();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Pictures").color(TEXT_DIM));
            for i in 0..count {
                let name = wallpaper
                    .slide(i)
                    .and_then(|w| w.image.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui
                    .selectable_label(self.slide == i, format!("{}", i + 1))
                    .on_hover_text(name)
                    .clicked()
                {
                    self.slide = i;
                }
            }
            ui.add_enabled_ui(!busy, |ui| {
                let at = self.slide;
                if ui
                    .add_enabled(at > 0, egui::Button::new(icon::CARET_LEFT))
                    .on_hover_text("Show earlier")
                    .clicked()
                {
                    wallpaper.swap_slides(at, at - 1);
                    self.slide = at - 1;
                }
                if ui
                    .add_enabled(at + 1 < count, egui::Button::new(icon::CARET_RIGHT))
                    .on_hover_text("Show later")
                    .clicked()
                {
                    wallpaper.swap_slides(at, at + 1);
                    self.slide = at + 1;
                }
                if ui
                    .add_enabled(count > 1, egui::Button::new(icon::MINUS))
                    .on_hover_text("Take this picture out of the slideshow")
                    .clicked()
                {
                    wallpaper.remove_slide(at);
                    self.slide = at.min(wallpaper.slide_count() - 1);
                }
                let full = count >= Slideshow::MAX_PICTURES;
                let add = ui
                    .add_enabled(!full, egui::Button::new(format!("{}  Add…", icon::PLUS)))
                    .on_hover_text("Add a picture: the wallpaper becomes a slideshow")
                    .on_disabled_hover_text(format!(
                        "A slideshow holds up to {} pictures",
                        Slideshow::MAX_PICTURES
                    ))
                    .clicked();
                if add {
                    self.choose(ctx, true);
                }
            });
        });
    }

    /// Puts a picture just read where the user asked for it: into the
    /// slideshow, in place of the selected picture, or as a new wallpaper.
    fn take(&mut self, wallpaper: &mut Option<Wallpaper>, path: PathBuf) {
        let chosen = Wallpaper::new(path);
        match wallpaper {
            Some(w) if self.adding => {
                if w.push_slide(chosen) {
                    self.slide = w.slide_count() - 1;
                }
            }
            Some(w) => {
                if let Some(slide) = w.slide_mut(self.slide) {
                    (slide.image, slide.zoom, slide.x, slide.y) =
                        (chosen.image, chosen.zoom, chosen.x, chosen.y);
                }
            }
            None => {
                *wallpaper = Some(chosen);
                self.slide = 0;
            }
        }
        self.adding = false;
    }

    fn error_row(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error {
            ui.label(RichText::new(format!("{}  {error}", icon::WARNING)).color(WARNING));
        }
    }

    fn choose_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, busy: bool) {
        let choose = ui
            .add_enabled(
                !busy,
                egui::Button::new(format!("{}  Choose Image…", icon::IMAGE)),
            )
            .clicked();
        if choose {
            self.choose(ctx, false);
        }
        if busy {
            ui.spinner();
        }
    }

    /// The keys as the device lays them out, with the wallpaper behind them.
    fn preview(
        &mut self,
        ui: &mut egui::Ui,
        shown: Option<&Wallpaper>,
        editable: Option<&mut Wallpaper>,
        model: &DeviceModel,
        renderer: &mut Renderer,
    ) {
        let (pw, ph) = model.panel_size();
        let height = PREVIEW_WIDTH * ph as f32 / pw.max(1) as f32;
        let (rect, resp) =
            ui.allocate_exact_size(vec2(PREVIEW_WIDTH, height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(4), KEY_EMPTY);
        let picture = shown.and_then(|w| Some((w, renderer.wallpaper(&w.image)?)));
        if let Some((wallpaper, src)) = &picture {
            let px = (PREVIEW_WIDTH * ui.ctx().pixels_per_point()).round() as u32;
            let mut h = DefaultHasher::new();
            (wallpaper, px, pw, ph).hash(&mut h);
            let hash = h.finish();
            let texture = match &self.preview {
                Some((shown, texture)) if *shown == hash => texture.id(),
                _ => {
                    let img = render::wallpaper_panel(src, wallpaper, model, px);
                    let size = [img.width() as usize, img.height() as usize];
                    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                    let texture = ui
                        .ctx()
                        .load_texture("wallpaper", color, TextureOptions::LINEAR);
                    let id = texture.id();
                    self.preview = Some((hash, texture));
                    id
                }
            };
            painter.image(
                texture,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else if shown.is_some() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "The picture cannot be read",
                egui::FontId::proportional(13.0),
                WARNING,
            );
        }
        // Shade what lies between the keys and outline the keys.
        let k = PREVIEW_WIDTH / pw.max(1) as f32;
        let keys: Vec<Rect> = model
            .key_rects()
            .iter()
            .map(|r| {
                Rect::from_min_size(
                    rect.min + vec2(r.x as f32, r.y as f32) * k,
                    Vec2::splat(r.size as f32 * k),
                )
            })
            .collect();
        if picture.is_some() {
            shade_outside(&painter, rect, &keys);
        }
        for key in &keys {
            painter.rect_stroke(
                *key,
                CornerRadius::same(3),
                Stroke::new(1.0, KEY_BORDER),
                egui::StrokeKind::Outside,
            );
        }
        let Some((_, src)) = picture else {
            return;
        };
        let Some(wallpaper) = editable else {
            return;
        };
        if resp.dragged() {
            let delta = resp.drag_delta();
            let [_, _, w, h] = render::wallpaper_view(src.dimensions(), wallpaper, (pw, ph));
            // Dragging moves the picture, so the view moves the other way:
            // `span` preview points show `shown` of `whole` picture pixels.
            let end = f32::from(Wallpaper::END);
            let shift = |at: u16, by: f32, shown: f32, span: f32, whole: u32| {
                let moved = -by / span.max(1.0) * shown / whole.max(1) as f32 * end;
                (f32::from(at) + moved).round().clamp(0.0, end) as u16
            };
            let (sw, sh) = src.dimensions();
            wallpaper.x = shift(wallpaper.x, delta.x, w, PREVIEW_WIDTH, sw);
            wallpaper.y = shift(wallpaper.y, delta.y, h, height, sh);
            clamp_to_view(wallpaper, src.dimensions(), (pw, ph));
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let zoom = f32::from(wallpaper.zoom) * (1.0 + WHEEL_ZOOM * scroll / 50.0);
                wallpaper.zoom = zoom.round().clamp(
                    f32::from(Wallpaper::MIN_ZOOM),
                    f32::from(Wallpaper::MAX_ZOOM),
                ) as u16;
                clamp_to_view(wallpaper, src.dimensions(), (pw, ph));
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    fn choose(&mut self, ctx: &egui::Context, adding: bool) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"],
            )
            .pick_file()
        else {
            return;
        };
        let (tx, rx) = mpsc::sync_channel(1);
        let owner = match &self.owner {
            Owner::Profile(id) => Some(id.clone()),
            Owner::Global => None,
        };
        let repaint = ctx.clone();
        // Large pictures take a moment; the window stays responsive.
        let spawned = std::thread::Builder::new()
            .name("deckly-wallpaper".into())
            .spawn(move || {
                let saved = render::decode_wallpaper(&path).and_then(|img| match &owner {
                    Some(profile) => store::save_wallpaper(profile, &img),
                    None => store::save_global_wallpaper(&img),
                });
                let _ = tx.send(saved);
                repaint.request_repaint();
            });
        match spawned {
            Ok(_) => {
                self.job = Some(rx);
                self.adding = adding;
                self.error = None;
            }
            Err(e) => self.error = Some(format!("Cannot read the picture: {e}")),
        }
    }

    fn receive(&mut self) -> Option<PathBuf> {
        let job = self.job.as_ref()?;
        let result = match job.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err("reading the picture stopped".into()),
        };
        self.job = None;
        match result {
            Ok(path) => {
                self.error = None;
                Some(path)
            }
            Err(e) => {
                self.error = Some(format!("Cannot use this picture: {e}"));
                None
            }
        }
    }
}

/// One entry of the preview's model list: a model's name, or the key grid
/// that several models share.
fn layout_label(group: &[&DeviceModel]) -> String {
    match group {
        [only] => only.name.to_owned(),
        [first, ..] => format!(
            "{} keys ({} × {})",
            first.key_count(),
            first.columns,
            first.rows
        ),
        [] => String::new(),
    }
}

fn zoom_row(ui: &mut egui::Ui, wallpaper: &mut Wallpaper) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Zoom").color(TEXT_DIM));
        ui.spacing_mut().slider_width = PREVIEW_WIDTH - 110.0;
        ui.add(
            egui::Slider::new(
                &mut wallpaper.zoom,
                Wallpaper::MIN_ZOOM..=Wallpaper::MAX_ZOOM,
            )
            .suffix("%"),
        );
    });
    ui.label(
        RichText::new(
            "Drag the picture to move it. Key pictures cover it; their \
             transparent parts let it through.",
        )
        .color(TEXT_FAINT),
    );
}

/// How a slideshow moves on: its transition, and how long pictures and
/// transitions last.
fn slideshow_rows(ui: &mut egui::Ui, show: &mut Slideshow) {
    egui::Grid::new("slideshow")
        .num_columns(2)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.label(RichText::new("Transition").color(TEXT_DIM));
            egui::ComboBox::from_id_salt("slideshow-transition")
                .selected_text(show.transition.label())
                .show_ui(ui, |ui| {
                    for t in Transition::ALL {
                        ui.selectable_value(&mut show.transition, t, t.label())
                            .on_hover_text(transition_hint(t));
                    }
                });
            ui.end_row();
            ui.label(RichText::new("Each picture").color(TEXT_DIM));
            ui.add(
                egui::DragValue::new(&mut show.interval)
                    .range(Slideshow::MIN_INTERVAL..=Slideshow::MAX_INTERVAL)
                    .speed(1.0)
                    .custom_formatter(|s, _| duration_text(s as u32))
                    .custom_parser(parse_seconds),
            );
            ui.end_row();
            ui.label(RichText::new("Transition time").color(TEXT_DIM));
            ui.add(
                egui::Slider::new(
                    &mut show.duration,
                    Slideshow::MIN_DURATION..=Slideshow::MAX_DURATION,
                )
                .step_by(100.0)
                .custom_formatter(|ms, _| format!("{:.1} s", ms / 1000.0))
                .custom_parser(|text| parse_seconds(text).map(|s| s * 1000.0)),
            );
            ui.end_row();
        });
    ui.label(RichText::new(transition_hint(show.transition)).color(TEXT_FAINT));
}

fn transition_hint(transition: Transition) -> &'static str {
    match transition {
        Transition::Diagonal => {
            "Keys turn to the next picture one after another, corner to corner."
        }
        Transition::Center => "Keys turn to the next picture from the middle outwards.",
        Transition::Wipe => "An edge sweeps across the keys and uncovers the next picture.",
        Transition::Push => "The next picture slides in and pushes the shown one away.",
        Transition::Fade => "The next picture fades in over the shown one.",
    }
}

/// Seconds as the slideshow shows them: "45 s", "5 min", "1 h 30 min".
fn duration_text(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match (h, m, s) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, 0) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

/// Reads "90", "90 s", "5 min", "1.5 h" or "1 h 30 min" as seconds.
fn parse_seconds(text: &str) -> Option<f64> {
    let mut total = 0.0;
    let mut number: Option<f64> = None;
    let mut parts = text.split_whitespace().flat_map(|w| {
        // "5min" reads as "5 min".
        let split = w.find(|c: char| c.is_alphabetic()).unwrap_or(w.len());
        [&w[..split], &w[split..]]
    });
    for part in parts.by_ref().filter(|p| !p.is_empty()) {
        if let Ok(n) = part.replace(',', ".").parse::<f64>() {
            if number.replace(n).is_some() {
                return None;
            }
            continue;
        }
        let unit = match part.to_lowercase().as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "h" | "hr" | "hour" | "hours" => 3600.0,
            _ => return None,
        };
        total += number.take()? * unit;
    }
    let total = total + number.unwrap_or(0.0);
    (total.is_finite() && total >= 0.0).then_some(total)
}

/// What the Profile tab says about the profile's wallpaper.
fn profile_status(disabled: bool, shown: Option<WallpaperLevel>) -> &'static str {
    match (disabled, shown) {
        (true, _) => {
            "Wallpaper is off for this profile. Pages and folders can still have their own."
        }
        (false, Some(WallpaperLevel::Global)) => {
            "This profile uses the global wallpaper from Preferences > Style."
        }
        (false, Some(_)) => "Applies to pages and folders that use the profile wallpaper.",
        (false, None) => {
            "This profile follows the global wallpaper, which is not set. \
             Choose a picture here or in Preferences > Style."
        }
    }
}

/// What the page or folder tab says about the wallpaper on the canvas.
fn local_status(disabled: bool, shown: Option<WallpaperLevel>) -> &'static str {
    match (disabled, shown) {
        (true, _) => "Wallpaper is off here.",
        (false, Some(WallpaperLevel::Local)) => "This layout uses its own wallpaper.",
        (false, Some(WallpaperLevel::Profile)) => "This layout uses the profile wallpaper.",
        (false, Some(WallpaperLevel::Global)) => {
            "This layout uses the global wallpaper through its profile."
        }
        (false, None) => "This layout follows the profile, which shows no wallpaper.",
    }
}

/// Moves the view back inside the picture, as the renderer shows it, so a
/// later drag starts from what is on screen.
fn clamp_to_view(wallpaper: &mut Wallpaper, src: (u32, u32), panel: (u32, u32)) {
    let [left, top, w, h] = render::wallpaper_view(src, wallpaper, panel);
    let end = f32::from(Wallpaper::END);
    let middle = |start: f32, shown: f32, whole: u32| {
        ((start + shown / 2.0) / whole.max(1) as f32 * end)
            .round()
            .clamp(0.0, end) as u16
    };
    wallpaper.x = middle(left, w, src.0);
    wallpaper.y = middle(top, h, src.1);
}

/// Darkens the panel outside the keys, where the device's frame is.
fn shade_outside(painter: &egui::Painter, panel: Rect, keys: &[Rect]) {
    let mut rows: Vec<(f32, f32)> = keys.iter().map(|k| (k.top(), k.bottom())).collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows.dedup_by(|a, b| (a.0 - b.0).abs() < 0.5);
    let mut y = panel.top();
    for &(top, bottom) in &rows {
        if top > y {
            painter.rect_filled(
                Rect::from_x_y_ranges(panel.x_range(), y..=top),
                0,
                GAP_SHADE,
            );
        }
        let mut x = panel.left();
        let mut row: Vec<&Rect> = keys
            .iter()
            .filter(|k| (k.top() - top).abs() < 0.5)
            .collect();
        row.sort_by(|a, b| a.left().total_cmp(&b.left()));
        for key in row {
            if key.left() > x {
                painter.rect_filled(
                    Rect::from_x_y_ranges(x..=key.left(), top..=bottom),
                    0,
                    GAP_SHADE,
                );
            }
            x = key.right();
        }
        if panel.right() > x {
            painter.rect_filled(
                Rect::from_x_y_ranges(x..=panel.right(), top..=bottom),
                0,
                GAP_SHADE,
            );
        }
        y = bottom;
    }
    if panel.bottom() > y {
        painter.rect_filled(
            Rect::from_x_y_ranges(panel.x_range(), y..=panel.bottom()),
            0,
            GAP_SHADE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slideshow_times_read_and_print_in_plain_units() {
        assert_eq!(duration_text(45), "45 s");
        assert_eq!(duration_text(300), "5 min");
        assert_eq!(duration_text(90), "1 min 30 s");
        assert_eq!(duration_text(5400), "1 h 30 min");
        assert_eq!(duration_text(86400), "24 h");
        assert_eq!(parse_seconds("90"), Some(90.0));
        assert_eq!(parse_seconds("1 min 30 s"), Some(90.0));
        assert_eq!(parse_seconds("5min"), Some(300.0));
        assert_eq!(parse_seconds("1,5 h"), Some(5400.0));
        assert_eq!(parse_seconds("1.5"), Some(1.5));
        assert_eq!(parse_seconds("soon"), None);
        assert_eq!(parse_seconds("1 2"), None);
        assert_eq!(parse_seconds("min"), None);
        assert_eq!(parse_seconds("-5"), None);
    }

    #[test]
    fn a_picture_read_joins_or_replaces_as_asked() {
        let mut editor = WallpaperEditor::global();
        let mut wall = None;
        editor.take(&mut wall, "a.png".into());
        editor.adding = true;
        editor.take(&mut wall, "b.png".into());
        let w = wall.as_ref().unwrap();
        assert_eq!((w.slide_count(), editor.slide), (2, 1));
        // Choosing a picture again replaces the selected one only.
        wall.as_mut().unwrap().slide_mut(1).unwrap().zoom = 300;
        editor.take(&mut wall, "c.png".into());
        let w = wall.unwrap();
        assert_eq!(w.image, PathBuf::from("a.png"));
        let second = w.slide(1).unwrap();
        assert_eq!(
            (second.image.as_path(), second.zoom),
            (std::path::Path::new("c.png"), 100)
        );
        assert!(w.slideshow.is_some());
    }

    #[test]
    fn the_view_is_brought_back_inside_the_picture() {
        // A square picture on a wide panel fills its width at 100 %, so it
        // cannot move sideways, only up and down.
        let mut wall = Wallpaper {
            x: 0,
            y: 0,
            ..Wallpaper::new("w.png".into())
        };
        clamp_to_view(&mut wall, (1000, 1000), (480, 272));
        assert_eq!(wall.x, Wallpaper::END / 2);
        let [_, top, _, h] = render::wallpaper_view((1000, 1000), &wall, (480, 272));
        assert_eq!(top, 0.0);
        assert_eq!(wall.y, ((h / 2.0) / 1000.0 * 1000.0).round() as u16);
        // Clamping again changes nothing.
        let before = wall.clone();
        clamp_to_view(&mut wall, (1000, 1000), (480, 272));
        assert_eq!(wall, before);
    }
}
