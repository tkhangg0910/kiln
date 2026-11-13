//! The image chip (MainWindow:1336-1468): the images in the active pane's prompt, pasted since
//! you last sent one. A thread looks every second while a pane is shown; the header draws the
//! chip, its thumbnails on hover, and the image full size over the pane on a click.

use crate::model::{lock, Shared};
use crate::pane::PaneShared;
use crate::theme::*;
use kiln_core::images;
use kiln_core::paths::OsEnv;
use egui::{Align2, Color32, ColorImage, CursorIcon, Id, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How often the shown pane's images are looked for (MainWindow:266).
const POLL: Duration = Duration::from_secs(1);
/// The thumbnails stay this long after the pointer leaves the chip and them (MainWindow:267).
const HIDE_AFTER: Duration = Duration::from_millis(300);
/// A new image shows its thumbnails this long (MainWindow:1385).
const FLASH: Duration = Duration::from_millis(1500);
/// Thumbnails are decoded at twice their 90 px height, for a sharp picture at 200 %.
const THUMB_DECODE: u32 = 180;
const THUMB_H: f32 = 90.0;
const THUMB_MAX_W: f32 = 200.0;
const WRAP_W: f32 = 460.0;

/// A pane's images in its prompt, as the poll last saw them.
#[derive(Default)]
pub struct Pasted {
    /// (file, its `[Image #n]` tag), in tag order.
    pub list: Vec<(PathBuf, u32)>,
    /// One was added since the UI last looked: the thumbnails flash.
    pub added: bool,
}

/// What the poll keeps between looks.
#[derive(Default)]
struct PollState {
    /// Codex clipboard files already given to a pane, so switching panes does not hand them on.
    claimed: HashSet<String>,
    /// Each Claude pane's paste cache folder, once it exists.
    dirs: HashMap<u64, PathBuf>,
}

/// Starts the poll: every second, the shown pane only (none while the window is hidden).
pub fn start_poll(model: Shared, ctx: egui::Context) {
    let _ = std::thread::Builder::new().name("kiln-images".into()).spawn(move || {
        let mut state = PollState::default();
        loop {
            std::thread::sleep(POLL);
            let pane = lock(&model).panes.iter().find(|p| p.visible.load(Ordering::Relaxed)).cloned();
            let Some(pane) = pane else { continue };
            if poll(&pane, &mut state) {
                ctx.request_repaint();
            }
        }
    });
}

/// PollImages for one pane. True when its list changed.
fn poll(pane: &PaneShared, state: &mut PollState) -> bool {
    let since = pane.images_since();
    let files: Vec<PathBuf> = if pane.codex {
        // A paste only reaches the pane you are in, so a new clipboard file is this one's.
        let mut mine = pane.codex_images.lock().unwrap_or_else(|e| e.into_inner());
        for file in images::since(&images::codex_dir(&OsEnv), since, images::CODEX_PATTERN) {
            if state.claimed.insert(file.to_string_lossy().to_lowercase()) {
                let at = kiln_core::time::mtime(&file).unwrap_or(since);
                mine.push((file, at));
            }
        }
        mine.retain(|(p, at)| *at > since && p.is_file());
        mine.iter().map(|(p, _)| p.clone()).collect()
    } else {
        let Some(id) = pane.session_id() else { return false };
        let dir = match state.dirs.entry(pane.id) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                let Some(d) = images::find_dir(&images::claude_root(&OsEnv), &id) else { return false };
                e.insert(d)
            }
        };
        images::since(dir, since, "*")
    };
    let tags = kiln_term::tags::image_tags(&pane.term.screen_lines_text());
    let list = images::tagged(&files, &tags);
    let mut p = pane.images.lock().unwrap_or_else(|e| e.into_inner());
    if list == p.list {
        return false;
    }
    p.added |= list.iter().any(|i| !p.list.contains(i));
    p.list = list;
    true
}

/// A picture decoded at a height, or None when it can't be.
type Decoded = ((PathBuf, u32), Option<ColorImage>);

/// Decoded pictures, off the UI thread: the file is read whole and then decoded, so the
/// agent's file is never held open (MainWindow:1418-1437).
#[derive(Default)]
pub struct Pictures {
    textures: HashMap<(PathBuf, u32), Option<TextureHandle>>,
    ready: Arc<Mutex<Vec<Decoded>>>,
    pending: HashSet<(PathBuf, u32)>,
}

impl Pictures {
    /// The picture at `height` px (0 = its own size, at most 8192): None while it decodes,
    /// Some(None) when it can't be.
    fn get(&mut self, ctx: &egui::Context, path: &Path, height: u32) -> Option<Option<TextureHandle>> {
        for (key, img) in std::mem::take(&mut *self.ready.lock().unwrap_or_else(|e| e.into_inner())) {
            self.pending.remove(&key);
            let tex = img.map(|i| ctx.load_texture(format!("pasted {}", key.0.display()), i, egui::TextureOptions::LINEAR));
            self.textures.insert(key, tex);
        }
        let key = (path.to_path_buf(), height);
        if let Some(t) = self.textures.get(&key) {
            return Some(t.clone());
        }
        if self.pending.insert(key.clone()) {
            let ready = self.ready.clone();
            let ctx = ctx.clone();
            let _ = std::thread::Builder::new().name("decode image".into()).spawn(move || {
                let img = decode(&key.0, key.1);
                ready.lock().unwrap_or_else(|e| e.into_inner()).push((key, img));
                ctx.request_repaint();
            });
        }
        None
    }

    /// Drops the pictures of images no longer shown.
    pub fn keep(&mut self, shown: &[(PathBuf, u32)], big: Option<&Path>) {
        self.textures.retain(|(p, _), _| shown.iter().any(|s| &s.0 == p) || big == Some(p.as_path()));
    }
}

fn decode(path: &Path, height: u32) -> Option<ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let img = if height > 0 && img.height() > height {
        let w = (u64::from(img.width()) * u64::from(height) / u64::from(img.height())).max(1) as u32;
        img.thumbnail(w, height)
    } else if img.width().max(img.height()) > 8192 {
        img.thumbnail(8192, 8192)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

/// The chip's popup and the full-size view, for the active pane.
#[derive(Default)]
pub struct ImageChip {
    pub pictures: Pictures,
    /// The thumbnails are up until then (None: while hovered, or closed).
    open: bool,
    hide_at: Option<Instant>,
    flash_until: Option<Instant>,
    /// The image shown full size over the pane.
    pub big: Option<PathBuf>,
}

/// What the chip asks for this frame.
#[derive(Default)]
pub struct ChipOut {
    /// The full-size view closed: the terminal takes the focus back.
    pub focus_terminal: bool,
}

impl ImageChip {
    /// The chip (glyph and count) in the header's right-to-left row, and its thumbnails.
    /// `list` is the active pane's images; `added` that one came since the last frame; `front`
    /// that Kiln is the foreground window.
    pub fn chip(&mut self, ui: &mut Ui, list: &[(PathBuf, u32)], added: bool, front: bool) {
        let now = Instant::now();
        if list.is_empty() {
            self.open = false;
            return;
        }
        let resp = chip_button(ui, '\u{EB9F}', Some(&list.len().to_string()), None);
        if resp.clicked() {
            // The newest image, full size.
            self.big = list.last().map(|i| i.0.clone());
            self.open = false;
        }
        if resp.hovered() {
            self.open = true;
            self.hide_at = None;
        }
        // Only while Kiln is in front: a popup over other apps' windows would be rude.
        if added && front {
            self.open = true;
            self.flash_until = Some(now + FLASH);
        }
        if !self.open {
            return;
        }
        let area = egui::Area::new(Id::NULL.with("images-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(resp.rect.left_bottom() + Vec2::new(0.0, 6.0))
            .constrain(true)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, HAIRLINE))
                    .corner_radius(6)
                    .inner_margin(6)
                    .show(ui, |ui| {
                        ui.set_max_width(WRAP_W);
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                            for (path, tag) in list {
                                if self.thumb(ui, path, *tag).clicked() {
                                    self.big = Some(path.clone());
                                    self.open = false;
                                }
                            }
                        });
                    });
            });
        let hovered = resp.hovered() || area.response.contains_pointer();
        let flashing = self.flash_until.is_some_and(|t| now < t);
        if hovered || flashing {
            self.hide_at = None;
            if let Some(t) = self.flash_until.filter(|_| flashing && !hovered) {
                ui.ctx().request_repaint_after(t - now);
            }
        } else {
            self.flash_until = None;
            let at = *self.hide_at.get_or_insert(now + HIDE_AFTER);
            if now >= at {
                self.open = false;
                self.hide_at = None;
            } else {
                ui.ctx().request_repaint_after(at - now);
            }
        }
    }

    /// A thumbnail labelled with its tag, in a QuietButton (padding 4).
    fn thumb(&mut self, ui: &mut Ui, path: &Path, tag: u32) -> egui::Response {
        let tex = self.pictures.get(ui.ctx(), path, THUMB_DECODE);
        let size = match &tex {
            Some(Some(t)) => {
                let s = t.size_vec2();
                let w = (s.x * THUMB_H / s.y.max(1.0)).min(THUMB_MAX_W);
                Vec2::new(w, THUMB_H.min(w * s.y / s.x.max(1.0)))
            }
            // Undecodable: the tag alone.
            Some(None) => Vec2::new(40.0, 0.0),
            None => Vec2::new(THUMB_H, THUMB_H),
        };
        let label_h = 18.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(size.x.max(24.0) + 8.0, size.y + label_h + 8.0), Sense::click());
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 4, RAISED_BG);
        }
        p.rect_stroke(rect, 4, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
        let pic = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 4.0 + size.y / 2.0), size);
        if let Some(Some(t)) = &tex {
            p.image(t.id(), pic, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let c = if resp.hovered() { TEXT_PRIMARY } else { TEXT_MUTED };
        p.text(Pos2::new(rect.center().x, pic.bottom() + 3.0), Align2::CENTER_TOP, format!("#{tag}"), ui_font(12.0), c);
        resp.on_hover_cursor(CursorIcon::PointingHand)
    }

    /// The image full size over the pane (#E614171C, margin 40, only ever scaled down); Esc,
    /// a click or ✕ closes it.
    pub fn big_view(&mut self, ui: &mut Ui, rect: Rect) -> ChipOut {
        let mut out = ChipOut::default();
        let Some(path) = self.big.clone() else { return out };
        let id = Id::NULL.with("image-big");
        let cover = ui.interact(rect, id, Sense::click());
        // It holds the focus while up, so keys do not reach the terminal under it.
        cover.request_focus();
        let p = ui.painter();
        p.rect_filled(rect, 0, Color32::from_rgba_unmultiplied(0x14, 0x17, 0x1C, 0xE6));
        let room = rect.shrink(40.0);
        if let Some(Some(t)) = self.pictures.get(ui.ctx(), &path, 0) {
            let s = t.size_vec2();
            let scale = (room.width() / s.x).min(room.height() / s.y).min(1.0);
            let r = Rect::from_center_size(room.center(), s * scale);
            p.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let close_rect = Rect::from_min_size(Pos2::new(rect.right() - 12.0 - 40.0, rect.top() + 12.0), Vec2::new(40.0, 30.0));
        let close = ui
            .scope_builder(egui::UiBuilder::new().max_rect(close_rect).layout(egui::Layout::right_to_left(egui::Align::Min)), |ui| {
                quiet_button(ui, "✕", true, Vec2::new(9.0, 4.0), true).on_hover_text("Close (Esc)")
            })
            .inner;
        if cover.clicked() || close.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.big = None;
            out.focus_terminal = true;
        }
        out
    }
}
