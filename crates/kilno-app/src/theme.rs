//! App.xaml as egui: the colour tokens, fonts, and the few custom widgets the chrome uses
//! (Quiet / Ghost / Accent buttons, status dot, chevron, switch, menu items). One dark theme.

use crate::fonts::{ICONS, SEMIBOLD};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Galley, Margin, Pos2, Rect, Response,
    Sense, Shadow, Stroke, StrokeKind, Ui, Vec2,
};
use std::sync::Arc;

pub const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const TERMINAL_BG: Color32 = hex(0x14171C);
pub const SURFACE_BG: Color32 = hex(0x1B1F26);
pub const RAISED_BG: Color32 = hex(0x222731);
pub const HAIRLINE: Color32 = hex(0x262C35);
pub const TEXT_PRIMARY: Color32 = hex(0xC8CDD6);
pub const TEXT_MUTED: Color32 = hex(0x7A828F);
pub const TEXT_FAINT: Color32 = hex(0x565E6B);
pub const MUTED_AA: Color32 = hex(0x8C94A1);
pub const WORKING: Color32 = hex(0xD4A24C);
pub const NEEDS_INPUT: Color32 = hex(0x5AA9E6);
pub const DONE: Color32 = hex(0x6BBF8A);
pub const FAILED: Color32 = hex(0xD2605F);
pub const ENDED: Color32 = hex(0x5A6472);
pub const DORMANT: Color32 = hex(0x39404B);
pub const CLAY: Color32 = hex(0xD27556);
pub const MENU_RIM: Color32 = hex(0x323A46);
pub const FOOTER_BAND: Color32 = hex(0x171B21);
pub const ROW_HOVER: Color32 = hex(0x1F242C);
pub const ROW_SELECTED: Color32 = hex(0x232935);
pub const POPUP_HOVER: Color32 = hex(0x2B313C);
pub const MENU_HOVER: Color32 = hex(0x2D343F);
pub const MENU_HOVER_FG: Color32 = hex(0xE6E9EE);
pub const MENU_GESTURE: Color32 = hex(0x959CA8);
pub const GHOST_RIM: Color32 = hex(0x2E353F);
pub const ACCENT_BG: Color32 = hex(0x2A303A);
pub const ACCENT_RIM: Color32 = hex(0x3A424E);
pub const SCROLL_THUMB: Color32 = hex(0x333A45);
pub const SCROLL_THUMB_HOVER: Color32 = hex(0x4A5462);

/// Status brush key (kilno_core::sidebar::status_key) to its colour.
pub fn status_color(key: &str) -> Color32 {
    match key {
        "Working" => WORKING,
        "NeedsInput" => NEEDS_INPUT,
        "Done" => DONE,
        "Failed" => FAILED,
        "Ended" => ENDED,
        _ => DORMANT,
    }
}

/// `#RRGGBB` (the sidebar's agent tag colours) to a colour.
pub fn parse_hex(s: &str) -> Color32 {
    u32::from_str_radix(s.trim_start_matches('#'), 16).map_or(TEXT_MUTED, hex)
}

pub fn ui_font(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Install the chrome's visuals: dark graphite, thin scrollbars, clay-free popups.
pub fn install(ctx: &egui::Context) {
    // Dark always, like the C# window. egui keeps the native caption in sync with its theme
    // preference; left at "System" it would turn the caption light on a light-mode Windows.
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = TERMINAL_BG;
    v.window_fill = RAISED_BG;
    v.window_stroke = Stroke::new(1.0, MENU_RIM);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    v.popup_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(110) };
    v.window_shadow = v.popup_shadow;
    v.extreme_bg_color = TERMINAL_BG;
    v.text_edit_bg_color = Some(TERMINAL_BG);
    v.override_text_color = None;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0xD4, 0xA2, 0x4C, 70);
    v.selection.stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.text_cursor.stroke = Stroke::new(1.5, WORKING);
    v.hyperlink_color = NEEDS_INPUT;
    let w = &mut v.widgets;
    w.noninteractive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    w.noninteractive.bg_fill = SURFACE_BG;
    // bg_fill is the scrollbar thumb (ThinScrollBar #333A45, hover #4A5462).
    w.inactive.bg_fill = SCROLL_THUMB;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    w.inactive.corner_radius = CornerRadius::same(4);
    w.hovered.bg_fill = SCROLL_THUMB_HOVER;
    w.hovered.weak_bg_fill = MENU_HOVER;
    w.hovered.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.hovered.fg_stroke = Stroke::new(1.0, MENU_HOVER_FG);
    w.hovered.corner_radius = CornerRadius::same(5);
    w.hovered.expansion = 0.0;
    w.active.bg_fill = SCROLL_THUMB_HOVER;
    w.active.weak_bg_fill = MENU_HOVER;
    w.active.bg_stroke = Stroke::new(1.0, HAIRLINE);
    w.active.fg_stroke = Stroke::new(1.0, MENU_HOVER_FG);
    w.active.corner_radius = CornerRadius::same(5);
    w.active.expansion = 0.0;
    w.open = w.hovered;
    ctx.set_visuals(v);
    // No zoom (the C# WebView had it off): Ctrl+- and Ctrl+= belong to the terminal.
    ctx.options_mut(|o| o.zoom_with_keyboard = false);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(0.0, 0.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.menu_margin = Margin::same(4);
        s.spacing.menu_width = 176.0;
        let sc = &mut s.spacing.scroll;
        sc.floating = false;
        sc.bar_width = 4.0;
        sc.bar_inner_margin = 2.0;
        sc.bar_outer_margin = 0.0;
        sc.foreground_color = false;
        sc.dormant_background_opacity = 0.0;
        sc.active_background_opacity = 0.0;
        sc.interact_background_opacity = 0.0;
        sc.dormant_handle_opacity = 1.0;
        sc.active_handle_opacity = 1.0;
        sc.interact_handle_opacity = 1.0;
        s.interaction.tooltip_delay = 0.4;
    });
}

/// A popup surface: RaisedBg, rim, radius 8 (menus) or 5 (the folder picker), drop shadow.
pub fn popup_frame(radius: u8, rim: Color32) -> Frame {
    Frame::new()
        .fill(RAISED_BG)
        .stroke(Stroke::new(1.0, rim))
        .corner_radius(CornerRadius::same(radius))
        .inner_margin(Margin::same(4))
        .shadow(Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(110) })
}

/// Single-line text, cut with an ellipsis to `max_width`.
pub fn elided(ui: &Ui, text: &str, font: FontId, color: Color32, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = TextWrapping::truncate_at_width(max_width.max(1.0));
    ui.painter().layout_job(job)
}

/// The 7 px status light.
pub fn dot(painter: &egui::Painter, center: Pos2, color: Color32) {
    painter.circle_filled(center, 3.5, color);
}

/// The section chevron, pointing right, or down when open.
pub fn chevron(painter: &egui::Painter, center: Pos2, open: bool, color: Color32) {
    let s = Stroke::new(1.4, color);
    let pts: [Pos2; 3] = if open {
        [center + Vec2::new(-4.0, -2.0), center + Vec2::new(0.0, 2.0), center + Vec2::new(4.0, -2.0)]
    } else {
        [center + Vec2::new(-2.0, -4.0), center + Vec2::new(2.0, 0.0), center + Vec2::new(-2.0, 4.0)]
    };
    painter.line_segment([pts[0], pts[1]], s);
    painter.line_segment([pts[1], pts[2]], s);
}

/// FocusRing: a clay 1 px outline, radius 9, 3 px outside the control, for keyboard focus only.
/// egui gives a button focus by Tab alone (a click does not), so focus here is keyboard focus.
/// `around` is the control the ring goes round (a settings row's switch, say).
pub fn focus_ring(ui: &Ui, resp: &Response, around: Rect) {
    if resp.has_focus() {
        ui.ctx().layer_painter(resp.layer_id).rect_stroke(
            around.expand(3.0),
            9,
            Stroke::new(1.0, CLAY),
            StrokeKind::Inside,
        );
    }
}

/// QuietButton: transparent, hairline border, radius 4, padding 10,5, 12 px; hover RaisedBg and
/// TextPrimary; disabled TextFaint with no border.
pub fn quiet_button(ui: &mut Ui, text: &str, enabled: bool, padding: Vec2, border: bool) -> Response {
    let font = ui_font(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, TEXT_MUTED);
    let size = galley.size() + padding * 2.0;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, 4, RAISED_BG);
        }
        if border && enabled {
            ui.painter().rect_stroke(rect, 4, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
        }
        let color = if !enabled {
            TEXT_FAINT
        } else if hovered {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };
        ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    }
    if enabled {
        resp.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// GhostButton (or AccentButton with `accent`): 30 high, radius 6, label at .75 that comes to
/// full on hover under a wash of the label colour; an optional Segoe glyph before the label.
pub fn ghost_button(ui: &mut Ui, glyph: Option<char>, label: &str, accent: bool, width: Option<f32>) -> Response {
    let fg = if accent { MENU_HOVER_FG } else { TEXT_PRIMARY };
    let label_g = (!label.is_empty()).then(|| ui.painter().layout_no_wrap(label.to_string(), ui_font(12.0), fg));
    let glyph_size = if label.is_empty() { 15.0 } else { 12.0 };
    let glyph_g = glyph.map(|c| ui.painter().layout_no_wrap(c.to_string(), icon_font(glyph_size), fg));
    let gap = if glyph_g.is_some() && label_g.is_some() { 7.0 } else { 0.0 };
    let content =
        label_g.as_ref().map_or(0.0, |g| g.size().x) + glyph_g.as_ref().map_or(0.0, |g| g.size().x) + gap;
    let w = width.unwrap_or(content + 24.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.16);
        let p = ui.painter();
        let (bg, rim) = if accent { (ACCENT_BG, ACCENT_RIM) } else { (Color32::TRANSPARENT, GHOST_RIM) };
        p.rect_filled(rect, 6, bg);
        p.rect_stroke(rect, 6, Stroke::new(1.0, rim), StrokeKind::Inside);
        let pressed = resp.is_pointer_button_down_on();
        let wash = if pressed { 0.03 } else { 0.08 * hover };
        if wash > 0.0 {
            p.rect_filled(rect, 6, fg.gamma_multiply(wash));
        }
        let face = if pressed { 0.62 } else { 0.75 + 0.25 * hover };
        let mut x = rect.center().x - content / 2.0;
        if let Some(g) = glyph_g {
            let y = rect.center().y - g.size().y / 2.0;
            let gw = g.size().x;
            p.galley(Pos2::new(x, y), g, fg.gamma_multiply(face));
            x += gw + gap;
        }
        if let Some(g) = label_g {
            let y = rect.center().y - g.size().y / 2.0;
            p.galley(Pos2::new(x, y), g, fg.gamma_multiply(face));
        }
    }
    focus_ring(ui, &resp, rect);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// One context-menu item: radius 5, hover #2D343F / #E6E9EE, gesture text on the right, a clay
/// tick when `checked` is Some(true). A disabled item is drawn faint and does not respond.
pub fn menu_item(ui: &mut Ui, label: &str, gesture: Option<&str>, checked: Option<bool>, enabled: bool) -> Response {
    let w = ui.available_width().max(176.0 - 8.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 28.0), sense);
    if ui.is_rect_visible(rect) {
        let hovered = enabled && resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, 5, MENU_HOVER);
        }
        let fg = if !enabled {
            TEXT_FAINT
        } else if hovered {
            MENU_HOVER_FG
        } else {
            TEXT_PRIMARY
        };
        let p = ui.painter();
        if checked == Some(true) {
            let c = Pos2::new(rect.left() + 14.0, rect.center().y);
            let s = Stroke::new(1.6, if enabled { CLAY } else { TEXT_FAINT });
            p.line_segment([c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.0, 3.0)], s);
            p.line_segment([c + Vec2::new(-1.0, 3.0), c + Vec2::new(5.0, -4.0)], s);
        }
        p.text(Pos2::new(rect.left() + 30.0, rect.center().y), Align2::LEFT_CENTER, label, ui_font(12.0), fg);
        if let Some(g) = gesture {
            let c = if enabled { MENU_GESTURE } else { TEXT_FAINT };
            p.text(Pos2::new(rect.right() - 12.0, rect.center().y), Align2::RIGHT_CENTER, g, ui_font(11.5), c);
        }
    }
    resp
}

/// Menu separator: 1 px MenuRim, margin 10,4.
pub fn menu_separator(ui: &mut Ui) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 9.0), Sense::hover());
    ui.painter().hline(rect.x_range().shrink(10.0), rect.center().y, Stroke::new(1.0, MENU_RIM));
}

/// Switch: a 40x20 outlined track that fills with clay when on, its knob sliding 20 px and
/// growing a little when `hover`; drawn at 0.4 when disabled. Only paints: the caller owns the
/// click (a settings row toggles from anywhere on it).
pub fn paint_switch(ui: &Ui, rect: Rect, id: egui::Id, on: bool, enabled: bool, hover: bool) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let t = ui.ctx().animate_bool_with_time(id, on, 0.18);
    let a = if enabled { 1.0 } else { 0.4 };
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 10, CLAY.gamma_multiply(a));
    } else {
        p.rect_stroke(rect, 10, Stroke::new(1.0, MUTED_AA.gamma_multiply(a)), StrokeKind::Inside);
    }
    let grow = if hover && enabled { 1.12 } else { 1.0 };
    let x = rect.left() + 10.0 + 20.0 * t;
    let knob = if on { hex(0x1A1210) } else { hex(0xA9B0BB) };
    p.circle_filled(Pos2::new(x, rect.center().y), 6.0 * grow, knob.gamma_multiply(a));
}

/// The 16 px tick box (the Tick style): clay-washed with a clay mark when on; faint when disabled.
pub fn tick_box(ui: &Ui, rect: Rect, on: bool, enabled: bool) {
    let p = ui.painter();
    let (bg, rim, mark) = match (on, enabled) {
        (_, false) => (Color32::TRANSPARENT, hex(0x3A414C), hex(0x59616D)),
        (true, true) => (Color32::from_rgba_unmultiplied(0xD2, 0x75, 0x56, 0x24), CLAY, CLAY),
        (false, true) => (Color32::TRANSPARENT, MUTED_AA, CLAY),
    };
    p.rect_filled(rect, 4, bg);
    p.rect_stroke(rect, 4, Stroke::new(1.0, rim), StrokeKind::Inside);
    if on {
        let o = rect.min;
        let s = Stroke::new(1.6, mark);
        p.line_segment([o + Vec2::new(2.6, 6.85), o + Vec2::new(5.6, 9.8)], s);
        p.line_segment([o + Vec2::new(5.6, 9.8), o + Vec2::new(11.4, 4.0)], s);
    }
}

/// A pane header chip (MainWindow.xaml:399-428): QuietButton with no border, padding 7,5, a
/// 14 px Segoe glyph and maybe a 12 px count 5 px after it; `tint` colours it (a flash).
pub fn chip_button(ui: &mut Ui, glyph: char, count: Option<&str>, tint: Option<Color32>) -> Response {
    let g = ui.painter().layout_no_wrap(glyph.to_string(), icon_font(14.0), TEXT_MUTED);
    let c = count.map(|c| ui.painter().layout_no_wrap(c.to_string(), ui_font(12.0), TEXT_MUTED));
    let w = g.size().x + c.as_ref().map_or(0.0, |c| 5.0 + c.size().x);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w + 14.0, 26.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = resp.hovered();
        if hovered {
            ui.painter().rect_filled(rect, 4, RAISED_BG);
        }
        let color = tint.unwrap_or(if hovered { TEXT_PRIMARY } else { TEXT_MUTED });
        let x = rect.left() + 7.0;
        let gw = g.size().x;
        ui.painter().galley(Pos2::new(x, rect.center().y - g.size().y / 2.0), g, color);
        if let Some(c) = c {
            ui.painter().galley(Pos2::new(x + gw + 5.0, rect.center().y - c.size().y / 2.0), c, color);
        }
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// A rectangle's band of height `h` at its top.
pub fn top_band(r: Rect, h: f32) -> Rect {
    Rect::from_min_size(r.min, Vec2::new(r.width(), h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_keys_map_to_the_xaml_brushes() {
        assert_eq!(status_color("Working"), Color32::from_rgb(0xD4, 0xA2, 0x4C));
        assert_eq!(status_color("NeedsInput"), Color32::from_rgb(0x5A, 0xA9, 0xE6));
        assert_eq!(status_color("Done"), Color32::from_rgb(0x6B, 0xBF, 0x8A));
        assert_eq!(status_color("Failed"), Color32::from_rgb(0xD2, 0x60, 0x5F));
        assert_eq!(status_color("Ended"), Color32::from_rgb(0x5A, 0x64, 0x72));
        assert_eq!(status_color("Dormant"), DORMANT);
        assert_eq!(parse_hex("#7DBBEB"), Color32::from_rgb(0x7D, 0xBB, 0xEB));
    }
}
