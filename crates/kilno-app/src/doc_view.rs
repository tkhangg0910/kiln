//! The doc viewer (terminal.html's #md panel): a markdown file the session wrote, beside the
//! terminal (split, the terminal reflows) or over it (full, the terminal keeps its size).
//! Rendered natively with egui_commonmark; nothing in a doc runs or reaches the network
//! (kilno_core::doc). Links come back to the caller, which asks before opening them.

use crate::theme::*;
use kilno_core::doc;
use egui::{
    Align2, Color32, CursorIcon, Id, Margin, OutputCommand, Pos2, Rect, Sense, Stroke, StrokeKind, TextStyle, Ui,
    UiBuilder, Vec2,
};
use egui_commonmark::{CommonMarkCache, CommonMarkScrollOptions, CommonMarkViewer};
use std::path::{Path, PathBuf};

/// The split's share of the pane, as the grip allows it (TH:325-341).
const SHARE_MIN: f32 = 0.2;
const SHARE_MAX: f32 = 0.8;
/// The name bar: 6 px above and below a 20 px button, and its bottom border.
const BAR_H: f32 = 33.0;
/// Headings in the doc CSS (h1–h4).
const HEADING: Color32 = hex(0xECEEF1);
const QUOTE_RULE: Color32 = hex(0x333A45);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Split,
    Full,
}

pub struct DocViewer {
    pub path: PathBuf,
    pub mode: Mode,
    mtime: Option<chrono::DateTime<chrono::Utc>>,
    /// The prepared markdown (images made safe).
    text: String,
    /// The scroll area's id: the same doc keeps it (and its place), a different one starts over.
    id: Id,
    /// The pane's transcript read count when the file was last looked at.
    seen: u64,
}

/// What the viewer asks of the pane this frame.
#[derive(Default)]
pub struct ViewerOut {
    /// ✕: back to the terminal.
    pub close: bool,
    /// Links clicked in the doc, not yet opened.
    pub links: Vec<String>,
    /// A bar button was pressed: the terminal takes the focus back.
    pub focus_terminal: bool,
}

impl DocViewer {
    /// Opens in split mode (TH:255-269).
    pub fn open(path: &Path, root: &Path, seen: u64, seq: u64) -> Self {
        let mut v = Self {
            path: path.to_path_buf(),
            mode: Mode::Split,
            mtime: None,
            text: String::new(),
            id: Id::NULL.with(("doc", seq)),
            seen,
        };
        v.load(root);
        v
    }

    fn load(&mut self, root: &Path) {
        self.mtime = kilno_core::time::mtime(&self.path);
        let dir = self.path.parent().unwrap_or(root);
        self.text = doc::prepare(&doc::read(&self.path), dir, root);
    }

    /// On each transcript read, the session's newest doc again when its path or its file time
    /// changed (MainWindow:658-669). The same doc keeps its place; a different one starts at
    /// the top.
    pub fn follow(&mut self, newest: Option<&Path>, generation: u64, root: &Path, cache: &mut CommonMarkCache, seq: u64) {
        if self.seen == generation {
            return;
        }
        self.seen = generation;
        let path = newest.unwrap_or(&self.path).to_path_buf();
        let mtime = kilno_core::time::mtime(&path);
        if path == self.path && mtime == self.mtime {
            return;
        }
        if path != self.path {
            self.id = Id::NULL.with(("doc", seq));
            self.path = path;
        }
        cache.clear_viewer(&self.id);
        self.load(root);
    }

    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
}

/// Where the viewer goes in the pane's body: split takes `share` of it on the right, full all
/// of it. The terminal gets the rest (None in full: it is covered and keeps its size).
pub fn layout(body: Rect, mode: Mode, share: f32) -> (Option<Rect>, Rect) {
    match mode {
        Mode::Full => (None, body),
        Mode::Split => {
            let w = (body.width() * share.clamp(SHARE_MIN, SHARE_MAX)).round();
            let x = body.right() - w;
            (Some(Rect::from_min_max(body.min, Pos2::new(x, body.bottom()))), Rect::from_min_max(Pos2::new(x, body.top()), body.max))
        }
    }
}

/// Draws the viewer in `rect` (from [`layout`]); `body` is the whole pane, for the grip.
pub fn show(ui: &mut Ui, rect: Rect, body: Rect, v: &mut DocViewer, share: &mut f32, cache: &mut CommonMarkCache) -> ViewerOut {
    let mut out = ViewerOut::default();
    let p = ui.painter().clone();
    // Beside the terminal the pane's veil is enough; Full lies on the terminal's text.
    let fill = if v.mode == Mode::Full { crate::background::veil(TERMINAL_BG) } else { crate::background::over_veil(TERMINAL_BG) };
    p.rect_filled(rect, 0, fill);
    // Full covers the terminal: nothing under it may take a click.
    if v.mode == Mode::Full {
        ui.interact(rect, Id::NULL.with("doc-cover"), Sense::click_and_drag());
    } else {
        p.vline(rect.left() + 0.5, rect.y_range(), Stroke::new(1.0, HAIRLINE));
    }

    // The name bar: the name, then ◧ Split, ⛶ Full, ✕.
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), BAR_H));
    p.hline(bar.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, HAIRLINE));
    let inner = Rect::from_min_max(bar.min + Vec2::new(14.0, 6.0), bar.max - Vec2::new(8.0, 7.0));
    let mut name_right = inner.right();
    ui.scope_builder(UiBuilder::new().max_rect(inner).layout(egui::Layout::right_to_left(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if bar_button(ui, "✕", false).on_hover_text("Back to the terminal").clicked() {
            out.close = true;
        }
        if bar_button(ui, "⛶ Full", v.mode == Mode::Full).on_hover_text("Fill the pane").clicked() {
            v.mode = Mode::Full;
            out.focus_terminal = true;
        }
        if bar_button(ui, "◧ Split", v.mode == Mode::Split).on_hover_text("Half and half").clicked() {
            v.mode = Mode::Split;
            out.focus_terminal = true;
        }
        name_right = ui.min_rect().left() - 4.0;
    });
    let g = elided(ui, &v.name(), ui_font(12.0), TEXT_MUTED, name_right - inner.left());
    p.galley(Pos2::new(inner.left(), inner.center().y - g.size().y / 2.0), g, TEXT_MUTED);

    let content = Rect::from_min_max(Pos2::new(rect.left() + 1.0, bar.bottom()), rect.max);
    out.links = markdown(ui, content, v.id, &v.text, cache);

    if v.mode == Mode::Split {
        grip(ui, rect, body, share);
    }
    out
}

/// The split's grip: 8 px over `rect`'s left border; amber while hovered or dragged (TH:31-36).
pub fn grip(ui: &mut Ui, rect: Rect, body: Rect, share: &mut f32) {
    let grip = Rect::from_min_max(Pos2::new(rect.left() - 4.0, rect.top()), Pos2::new(rect.left() + 4.0, rect.bottom()));
    let resp = ui.interact(grip, Id::NULL.with("doc-grip"), Sense::drag()).on_hover_cursor(CursorIcon::ResizeHorizontal);
    let resp = if resp.dragged() { resp } else { resp.on_hover_text("Drag to resize") };
    if resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        if let Some(x) = ui.ctx().pointer_interact_pos().map(|p| p.x) {
            *share = ((body.right() - x) / body.width().max(1.0)).clamp(SHARE_MIN, SHARE_MAX);
        }
    }
    if resp.hovered() || resp.dragged() {
        ui.painter().vline(rect.left(), rect.y_range(), Stroke::new(2.0, WORKING));
    }
}

/// Prepared markdown in `content`, styled as the C# doc CSS (TH:243-248), scrolled under `id`.
/// Links clicked come back, to be asked about, not opened (TH:274-280).
pub fn markdown(ui: &mut Ui, content: Rect, id: Id, text: &str, cache: &mut CommonMarkCache) -> Vec<String> {
    let before = ui.ctx().output(|o| o.commands.len());
    ui.scope_builder(UiBuilder::new().max_rect(content), |ui| {
        doc_style(ui);
        let scroll = CommonMarkScrollOptions::default().viewport_cache(true).content_margin(Margin {
            left: 24,
            right: 24,
            top: 18,
            bottom: 40,
        });
        CommonMarkViewer::new().show_alt_text_on_hover(false).show_scrollable(id, ui, cache, &scroll, text);
    });
    let mut links = Vec::new();
    ui.ctx().output_mut(|o| {
        let mut i = 0;
        o.commands.retain(|c| {
            i += 1;
            match c {
                OutputCommand::OpenUrl(u) if i > before => {
                    links.push(u.url.clone());
                    false
                }
                _ => true,
            }
        });
    });
    links
}

/// The bar's buttons: 12 px, hairline border, radius 4, padding 2,8; hover RaisedBg and
/// TextPrimary; the active mode amber.
pub fn bar_button(ui: &mut Ui, text: &str, on: bool) -> egui::Response {
    let g = ui.painter().layout_no_wrap(text.to_string(), ui_font(12.0), TEXT_MUTED);
    let size = Vec2::new(g.size().x + 16.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 4, RAISED_BG);
    }
    ui.painter().rect_stroke(rect, 4, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
    let c = if on {
        WORKING
    } else if hovered {
        TEXT_PRIMARY
    } else {
        TEXT_MUTED
    };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, ui_font(12.0), c);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// body 14 px Segoe UI #c8cdd6; headings #eceef1; links #d4a24c; code 12.5 px Cascadia on
/// #1b1f26; blockquote text #7a828f; rules and table lines #262c35.
fn doc_style(ui: &mut Ui) {
    let s = ui.style_mut();
    s.text_styles.insert(TextStyle::Body, ui_font(14.0));
    s.text_styles.insert(TextStyle::Button, ui_font(14.0));
    s.text_styles.insert(TextStyle::Heading, semibold(24.0));
    s.text_styles.insert(TextStyle::Monospace, mono(12.5));
    s.text_styles.insert(TextStyle::Small, ui_font(11.0));
    s.override_text_style = None;
    let v = &mut s.visuals;
    v.override_text_color = None;
    v.widgets.noninteractive.fg_stroke.color = TEXT_PRIMARY;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    v.widgets.active.fg_stroke.color = HEADING;
    v.weak_text_color = Some(TEXT_MUTED);
    v.hyperlink_color = WORKING;
    v.code_bg_color = SURFACE_BG;
    v.extreme_bg_color = SURFACE_BG;
    v.faint_bg_color = hex(0x181B21);
    v.window_stroke = Stroke::new(1.0, QUOTE_RULE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_takes_its_share_on_the_right() {
        let body = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 500.0));
        let (term, doc) = layout(body, Mode::Split, 0.5);
        assert_eq!(term.unwrap().right(), 500.0);
        assert_eq!(doc.left(), 500.0);
        let (term, _) = layout(body, Mode::Split, 0.95);
        assert_eq!(term.unwrap().right(), 200.0);
        let (term, doc) = layout(body, Mode::Full, 0.5);
        assert!(term.is_none());
        assert_eq!(doc, body);
    }

    #[test]
    fn follows_edits_and_the_newest_doc() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        std::fs::write(&a, "# A").unwrap();
        std::fs::write(&b, "# B").unwrap();
        let mut cache = CommonMarkCache::default();
        let mut v = DocViewer::open(&a, dir.path(), 1, 1);
        assert_eq!(v.text, "# A");
        let id = v.id;
        // The same read count: nothing is looked at.
        std::fs::write(&a, "# A2").unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&a).unwrap().set_modified(later).unwrap();
        v.follow(Some(&a), 1, dir.path(), &mut cache, 2);
        assert_eq!(v.text, "# A");
        // A new read: the edit shows, in the same place.
        v.follow(Some(&a), 2, dir.path(), &mut cache, 2);
        assert_eq!((v.text.as_str(), v.id), ("# A2", id));
        // A different doc starts over.
        v.follow(Some(&b), 3, dir.path(), &mut cache, 3);
        assert_eq!(v.text, "# B");
        assert_ne!(v.id, id);
    }
}
