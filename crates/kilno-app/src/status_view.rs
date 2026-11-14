//! StatusStripView.cs as egui: the pane header's status strip and the sidebar footer's
//! account limits, in the four themes (Numbers, Bars, Rings, Chips), with the design's sizes
//! and colours. What to show is `kilno_core::status::strip`; this only draws it.

use crate::theme::{elided, semibold, ui_font, MUTED_AA, TEXT_PRIMARY};
use kilno_core::settings::StatusTheme;
use kilno_core::status::strip::{effort_step, Band, Meter, Strip};
use egui::epaint::{CubicBezierShape, PathShape, PathStroke};
use egui::{Align2, Color32, CornerRadius, Galley, Id, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::sync::Arc;

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const BAND_GREEN: Color32 = hex(0x6BBF8A);
pub const BAND_YELLOW: Color32 = hex(0xD9BE5C);
pub const BAND_ORANGE: Color32 = hex(0xE58C4E);
pub const BAND_RED: Color32 = hex(0xEC726C);
const DIFF_ADDED: Color32 = hex(0x7CC596);
const DIFF_DELETED: Color32 = hex(0xE08480);
const STRIP_RULE: Color32 = hex(0x2E353F);
const LIMIT_RULE: Color32 = hex(0x262C35);
const METER_TRACK: Color32 = hex(0x2A303A);
const CHIP_BG: Color32 = hex(0x1F242C);
const STEP_ON: Color32 = hex(0xA9B0BB);
const STEP_OFF: Color32 = hex(0x39404B);

/// A limit past its reset is dimmed: last known, not current.
const STALE_OPACITY: f32 = 0.45;

pub fn band_color(b: Band) -> Color32 {
    match b {
        Band::Red => BAND_RED,
        Band::Orange => BAND_ORANGE,
        Band::Yellow => BAND_YELLOW,
        Band::Green => BAND_GREEN,
    }
}

/// The band's colour at a fraction of its strength, for a chip's rim and fill.
fn tint(b: Band, alpha: f32) -> Color32 {
    let c = band_color(b);
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha * 255.0).round() as u8)
}

fn long_name(m: &Meter) -> &'static str {
    if m.label == "5h" {
        "5-hour"
    } else {
        "7-day"
    }
}

// ---------------------------------------------------------------------------- parts

/// One piece of a field, measured before it is placed.
enum Part {
    Text(Arc<Galley>),
    Bar { w: f32, percent: i32, band: Band },
    Ring { size: f32, stroke: f32, percent: i32, band: Band },
    Steps(i32),
    BranchGlyph,
}

impl Part {
    fn size(&self) -> Vec2 {
        match self {
            Part::Text(g) => g.size(),
            Part::Bar { w, .. } => Vec2::new(*w, 4.0),
            Part::Ring { size, .. } => Vec2::splat(*size),
            Part::Steps(_) => Vec2::new(5.0 * 3.0 + 4.0 * 2.0, 12.0),
            Part::BranchGlyph => Vec2::splat(12.0),
        }
    }

    /// Drawn vertically centred on `cy`, from `x`.
    fn paint(&self, p: &Painter, x: f32, cy: f32) {
        let s = self.size();
        let r = Rect::from_min_size(Pos2::new(x, cy - s.y / 2.0), s);
        match self {
            Part::Text(g) => p.galley(r.min, g.clone(), TEXT_PRIMARY),
            Part::Bar { percent, band, .. } => bar(p, r, *percent, *band),
            Part::Ring { size, stroke, percent, band } => ring(p, r.center(), *size, *stroke, *percent, *band),
            Part::Steps(step) => steps(p, r, *step),
            Part::BranchGlyph => branch_glyph(p, r),
        }
    }
}

fn text(ui: &Ui, s: &str, size: f32, color: Color32) -> Part {
    Part::Text(ui.painter().layout_no_wrap(s.to_string(), ui_font(size), color))
}

fn percent(ui: &Ui, m: &Meter, size: f32) -> Part {
    Part::Text(ui.painter().layout_no_wrap(format!("{}%", m.percent), semibold(size), band_color(m.band())))
}

/// A 4 px bar filled to the percentage in the band's colour.
fn bar(p: &Painter, r: Rect, percent: i32, band: Band) {
    p.rect_filled(r, CornerRadius::same(2), METER_TRACK);
    let f = percent.clamp(0, 100) as f32 / 100.0;
    if f > 0.0 {
        let fill = Rect::from_min_size(r.min, Vec2::new(r.width() * f, r.height()));
        p.rect_filled(fill, CornerRadius::same(2), band_color(band));
    }
}

/// A ring gauge: a dark track, and the percentage swept clockwise from the top with round ends.
fn ring(p: &Painter, c: Pos2, size: f32, stroke: f32, percent: i32, band: Band) {
    let r = (size - stroke) / 2.0;
    p.circle_stroke(c, r, Stroke::new(stroke, METER_TRACK));
    let pct = percent.clamp(0, 100) as f32;
    if pct <= 0.0 {
        return;
    }
    // Just short of a full turn at 100%: an arc cannot end where it starts.
    let a = (pct / 100.0 * std::f32::consts::TAU).min(std::f32::consts::TAU - 0.001);
    let n = ((a / std::f32::consts::TAU) * 64.0).ceil().max(2.0) as usize;
    let pts: Vec<Pos2> =
        (0..=n).map(|i| a * i as f32 / n as f32).map(|t| Pos2::new(c.x + r * t.sin(), c.y - r * t.cos())).collect();
    let color = band_color(band);
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    p.add(PathShape::line(pts, PathStroke::new(stroke, color)));
    // Round caps.
    p.circle_filled(first, stroke / 2.0, color);
    p.circle_filled(last, stroke / 2.0, color);
}

/// Effort as five rising bars, lit up to its step (0 is low, 4 is max).
fn steps(p: &Painter, r: Rect, step: i32) {
    for i in 0..5 {
        let h = 4.0 + 2.0 * i as f32;
        let x = r.left() + i as f32 * (3.0 + 2.0);
        let b = Rect::from_min_max(Pos2::new(x, r.bottom() - h), Pos2::new(x + 3.0, r.bottom()));
        p.rect_filled(b, 0, if i <= step { STEP_ON } else { STEP_OFF });
    }
}

/// The design's 16-unit branch glyph drawn at 12 px, its 1.3 stroke scaling with it.
fn branch_glyph(p: &Painter, r: Rect) {
    let k = r.width() / 16.0;
    let at = |x: f32, y: f32| Pos2::new(r.left() + x * k, r.top() + y * k);
    let s = Stroke::new(1.3 * k, MUTED_AA);
    for (x, y) in [(4.5, 3.5), (4.5, 12.5), (11.5, 5.0)] {
        p.circle_stroke(at(x, y), 1.6 * k, s);
    }
    p.line_segment([at(4.5, 5.1), at(4.5, 10.9)], s);
    p.add(CubicBezierShape::from_points_stroke(
        [at(11.5, 6.6), at(11.5, 9.6), at(4.5, 8.6), at(4.5, 10.9)],
        false,
        Color32::TRANSPARENT,
        s,
    ));
}

/// Parts side by side, `gap` apart.
fn row_width(parts: &[Part], gap: f32) -> f32 {
    parts.iter().map(|p| p.size().x).sum::<f32>() + gap * parts.len().saturating_sub(1) as f32
}

fn paint_row(p: &Painter, parts: &[Part], x: f32, cy: f32, gap: f32) {
    let mut x = x;
    for part in parts {
        part.paint(p, x, cy);
        x += part.size().x + gap;
    }
}

fn faded(p: &Painter, stale: bool) -> Painter {
    let mut p = p.clone();
    if stale {
        p.multiply_opacity(STALE_OPACITY);
    }
    p
}

// ---------------------------------------------------------------------------- header strip

/// One field of the strip: its parts, its tooltip, and for a meter its band (chip tint) and
/// whether it is stale.
struct Field {
    tip: String,
    parts: Vec<Part>,
    tint: Option<Band>,
    stale: bool,
}

/// The strip beside a session's title, from `x` to `right` (clipped, never squeezed), centred
/// on `cy`. Nothing to show draws nothing.
pub fn draw_strip(ui: &mut Ui, strip: Option<&Strip>, theme: StatusTheme, x: f32, right: f32, cy: f32) {
    let Some(strip) = strip else { return };
    if right <= x {
        return;
    }
    let chips = theme == StatusTheme::Chips;
    // Chip text is a size smaller.
    let ts = if chips { 11.5 } else { 12.0 };
    let mut fields: Vec<Field> = Vec::new();
    if let Some(model) = &strip.model {
        fields.push(Field { tip: "Model".into(), parts: vec![text(ui, model, ts, TEXT_PRIMARY)], tint: None, stale: false });
    }
    if let Some(git) = &strip.git {
        let mut parts = vec![
            Part::BranchGlyph,
            Part::Text(elided(ui, &git.branch, ui_font(ts), TEXT_PRIMARY, 190.0)),
        ];
        // Nothing uncommitted shows no counts, as in the user's own status line.
        if git.added + git.deleted > 0 {
            parts.push(text(ui, &format!("+{}", git.added), ts, DIFF_ADDED));
            parts.push(text(ui, &format!("\u{2212}{}", git.deleted), ts, DIFF_DELETED));
        }
        fields.push(Field { tip: "Branch and uncommitted diff".into(), parts, tint: None, stale: false });
    }
    let meter = |ui: &Ui, m: &Meter| -> Field {
        // Bars and Rings spell context out; the limits are short either way.
        let label = if m.label == "ctx" { "Context" } else { m.label.as_str() };
        let band = m.band();
        let mut parts = match theme {
            StatusTheme::Bars => vec![
                text(ui, label, ts, MUTED_AA),
                Part::Bar { w: 52.0, percent: m.percent, band },
                percent(ui, m, ts),
            ],
            StatusTheme::Rings => vec![
                Part::Ring { size: 16.0, stroke: 2.5, percent: m.percent, band },
                text(ui, label, ts, MUTED_AA),
                percent(ui, m, ts),
            ],
            _ => vec![text(ui, &m.label, ts, MUTED_AA), percent(ui, m, ts)],
        };
        // Numbers puts "used/size" or "↻ 1h20m" before the percentage; Chips the bare reset after it.
        if theme == StatusTheme::Numbers {
            if let Some(d) = &m.detail {
                parts.insert(1, text(ui, d, ts, MUTED_AA));
            }
        }
        if chips {
            if let Some(r) = &m.reset {
                parts.push(text(ui, r, ts, MUTED_AA));
            }
        }
        Field { tip: m.tip.clone(), parts, tint: Some(band), stale: m.stale }
    };
    if let Some(m) = &strip.context {
        fields.push(meter(ui, m));
    }
    if let Some(effort) = &strip.effort {
        let value = text(ui, effort, ts, TEXT_PRIMARY);
        let step = effort_step(effort);
        let parts = match theme {
            StatusTheme::Chips => vec![text(ui, "effort", ts, MUTED_AA), value],
            StatusTheme::Bars | StatusTheme::Rings if step >= 0 => vec![Part::Steps(step), value],
            _ => vec![value],
        };
        fields.push(Field { tip: format!("Effort: {effort}"), parts, tint: None, stale: false });
    }
    if let Some(m) = &strip.five_hour {
        fields.push(meter(ui, m));
    }
    if let Some(m) = &strip.seven_day {
        fields.push(meter(ui, m));
    }

    let clip = Rect::from_min_max(Pos2::new(x, cy - 16.0), Pos2::new(right, cy + 16.0));
    let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
    // The rule that sets the strip off from the title, in every theme.
    let mut x = x + 2.0;
    painter.vline(x + 0.5, (cy - 7.0)..=(cy + 7.0), Stroke::new(1.0, STRIP_RULE));
    x += 1.0 + 12.0;
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            match theme {
                StatusTheme::Numbers => {
                    x += 14.0;
                    painter.vline(x + 0.5, (cy - 6.0)..=(cy + 6.0), Stroke::new(1.0, STRIP_RULE));
                    x += 1.0 + 14.0;
                }
                StatusTheme::Chips => x += 6.0,
                _ => x += 14.0,
            }
        }
        if x >= right {
            break;
        }
        let p = faded(&painter, f.stale);
        let w = row_width(&f.parts, 6.0);
        let field = if chips {
            // A 22 px pill, neutral or tinted by the meter's band.
            let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(w + 18.0, 22.0));
            let (rim, fill) = match f.tint {
                Some(b) => (tint(b, 0.34), tint(b, 0.10)),
                None => (STRIP_RULE, CHIP_BG),
            };
            p.rect(r, CornerRadius::same(11), fill, Stroke::new(1.0, rim), StrokeKind::Inside);
            paint_row(&p, &f.parts, x + 9.0, cy, 6.0);
            r
        } else {
            paint_row(&p, &f.parts, x, cy, 6.0);
            Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(w, 22.0))
        };
        let shown = field.intersect(clip);
        if shown.width() > 0.0 {
            let r = ui.interact(shown, Id::NULL.with(("strip-field", i)), Sense::hover());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &f.tip));
            r.on_hover_text(&f.tip);
        }
        x = field.right();
    }
}

// ---------------------------------------------------------------------------- footer limits

fn row_h(ui: &Ui, size: f32) -> f32 {
    ui.fonts_mut(|f| f.row_height(&ui_font(size)))
}

/// How tall the footer's limits are in this theme: 0 when there are none (the row and its gap
/// go).
pub fn limits_height(ui: &Ui, meters: &[Meter], theme: StatusTheme) -> f32 {
    if meters.is_empty() {
        return 0.0;
    }
    match theme {
        StatusTheme::Numbers => row_h(ui, 12.0),
        StatusTheme::Bars => meters.len() as f32 * row_h(ui, 12.0) + (meters.len() - 1) as f32 * 8.0,
        StatusTheme::Rings => 34.0,
        StatusTheme::Chips => {
            let reset = meters.iter().any(|m| m.reset.is_some());
            2.0 + 14.0 + row_h(ui, 15.0) + if reset { 1.0 + row_h(ui, 11.0) } else { 0.0 }
        }
    }
}

/// LimitsView: Numbers in two ruled columns, Bars as a row each, Rings and Chips as two cards
/// side by side. `rect` is as tall as [`limits_height`].
pub fn draw_limits(ui: &mut Ui, rect: Rect, meters: &[Meter], theme: StatusTheme) {
    if meters.is_empty() {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect.expand(1.0).intersect(ui.clip_rect()));
    let col_w = rect.width() / 2.0;
    for (i, m) in meters.iter().enumerate() {
        let p = faded(&painter, m.stale);
        let cell = match theme {
            StatusTheme::Bars => {
                let h = row_h(ui, 12.0);
                let r = Rect::from_min_size(Pos2::new(rect.left(), rect.top() + i as f32 * (h + 8.0)), Vec2::new(rect.width(), h));
                bar_row(ui, &p, r, m);
                r
            }
            _ => {
                // The second column: Numbers rules it off 12 px either side; the cards keep a gap.
                let mut r = Rect::from_min_size(Pos2::new(rect.left() + col_w * i as f32, rect.top()), Vec2::new(col_w, rect.height()));
                if i > 0 {
                    let gap = match theme {
                        StatusTheme::Rings => 10.0,
                        StatusTheme::Chips => 8.0,
                        _ => 12.0,
                    };
                    r.min.x += gap;
                    if theme == StatusTheme::Numbers {
                        p.vline(r.left() + 0.5, r.y_range(), Stroke::new(1.0, LIMIT_RULE));
                        r.min.x += 1.0 + 12.0;
                    }
                }
                match theme {
                    StatusTheme::Rings => ring_cell(ui, &p, r, m),
                    StatusTheme::Chips => chip_cell(ui, &p, r, m),
                    _ => numbers_cell(ui, &p, r, m),
                }
                r
            }
        };
        // Tooltip and screen reader name, as Draw.Describe.
        let r = ui.interact(cell, Id::NULL.with(("limit-cell", i)), Sense::hover());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &m.tip));
        r.on_hover_text(&m.tip);
    }
}

/// "5h 62% ↻ 1h20m".
fn numbers_cell(ui: &Ui, p: &Painter, r: Rect, m: &Meter) {
    let mut parts = vec![text(ui, &m.label, 12.0, MUTED_AA), percent(ui, m, 12.0)];
    if let Some(d) = &m.detail {
        parts.push(text(ui, d, 11.5, MUTED_AA));
    }
    paint_row(&p.with_clip_rect(r.intersect(p.clip_rect())), &parts, r.left(), r.center().y, 6.0);
}

/// "5-hour [bar] 62% 1h20m", in 44 | rest | 44 | 58 columns, 10 px apart.
fn bar_row(ui: &Ui, p: &Painter, r: Rect, m: &Meter) {
    let cy = r.center().y;
    text(ui, long_name(m), 12.0, MUTED_AA).paint(p, r.left(), cy);
    let bar_l = r.left() + 44.0 + 10.0;
    let bar_r = r.right() - 58.0 - 44.0;
    if bar_r > bar_l {
        bar(p, Rect::from_min_max(Pos2::new(bar_l, cy - 2.0), Pos2::new(bar_r, cy + 2.0)), m.percent, m.band());
    }
    let pct = percent(ui, m, 12.0);
    pct.paint(p, r.right() - 58.0 - pct.size().x, cy);
    let reset = text(ui, m.reset.as_deref().unwrap_or(""), 11.5, MUTED_AA);
    reset.paint(p, r.right() - reset.size().x, cy);
}

/// A 34 px ring with the number in it, then "5-hour limit" over "resets in 1h20m".
fn ring_cell(ui: &Ui, p: &Painter, r: Rect, m: &Meter) {
    let c = Pos2::new(r.left() + 17.0, r.top() + 17.0);
    ring(p, c, 34.0, 3.0, m.percent, m.band());
    p.text(c, Align2::CENTER_CENTER, m.percent.to_string(), semibold(10.5), band_color(m.band()));
    // The words get what is left of the column and trim there.
    let x = r.left() + 34.0 + 9.0;
    let w = (r.right() - x).max(1.0);
    let head = elided(ui, &format!("{} limit", long_name(m)), ui_font(12.0), TEXT_PRIMARY, w);
    let sub = m.reset.as_ref().map(|reset| elided(ui, &format!("resets in {reset}"), ui_font(11.0), MUTED_AA, w));
    let h = head.size().y + sub.as_ref().map_or(0.0, |s| 2.0 + s.size().y);
    let mut y = r.top() + (34.0 - h) / 2.0;
    let hh = head.size().y;
    p.galley(Pos2::new(x, y), head, TEXT_PRIMARY);
    y += hh + 2.0;
    if let Some(s) = sub {
        p.galley(Pos2::new(x, y), s, MUTED_AA);
    }
}

/// A card tinted by the band: "5-hour" and the percentage, then "resets in 1h20m".
fn chip_cell(ui: &Ui, p: &Painter, r: Rect, m: &Meter) {
    let band = m.band();
    p.rect(r, CornerRadius::same(8), tint(band, 0.08), Stroke::new(1.0, tint(band, 0.30)), StrokeKind::Inside);
    let inner = r.shrink2(Vec2::new(10.0 + 1.0, 7.0 + 1.0));
    let pct = percent(ui, m, 15.0);
    let top_h = pct.size().y;
    let cy = inner.top() + top_h / 2.0;
    pct.paint(p, inner.right() - pct.size().x, cy);
    let name_w = (inner.width() - pct.size().x - 4.0).max(1.0);
    Part::Text(elided(ui, long_name(m), ui_font(11.5), MUTED_AA, name_w)).paint(p, inner.left(), cy);
    if let Some(reset) = &m.reset {
        let g = elided(ui, &format!("resets in {reset}"), ui_font(11.0), MUTED_AA, inner.width());
        p.galley(Pos2::new(inner.left(), inner.top() + top_h + 1.0), g, MUTED_AA);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_colours_and_tints() {
        assert_eq!(band_color(Band::Yellow), Color32::from_rgb(0xD9, 0xBE, 0x5C));
        let t = tint(Band::Red, 0.34);
        assert_eq!(t.a(), 87);
        assert_eq!(tint(Band::Green, 0.10).a(), 26);
    }

    #[test]
    fn effort_steps_measure_like_the_design() {
        assert_eq!(Part::Steps(2).size(), Vec2::new(23.0, 12.0));
        assert_eq!(Part::BranchGlyph.size(), Vec2::splat(12.0));
    }
}
