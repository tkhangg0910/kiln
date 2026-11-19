//! The window's background picture (Settings › General). Drawn first, under everything; the
//! big fills (sidebar, header, terminal) then let it show through by its strength, so text and
//! controls are drawn over it at full contrast, as in Windows Terminal.

use egui::{Color32, Pos2, Rect, TextureHandle, TextureOptions, Ui, Vec2};
use kilno_term::palette::BACKGROUND_ALPHA;
use std::sync::atomic::Ordering;

#[derive(Default)]
pub struct Background {
    /// The path the texture was made from; reloaded when Settings names another.
    path: Option<String>,
    tex: Option<TextureHandle>,
    /// The picture's average colour, for a title bar that matches it.
    pub average: Option<[u8; 3]>,
}

impl Background {
    /// Before anything else of the frame is drawn.
    pub fn paint(&mut self, ui: &Ui, rect: Rect, path: Option<&str>, strength: i32) {
        if self.path.as_deref() != path {
            self.path = path.map(str::to_string);
            // shortcut: decoded on the UI thread, once per pick; move to a thread if big files stall.
            let img = path.and_then(|p| crate::image_chip::decode(std::path::Path::new(p), 0));
            self.average = img.as_ref().and_then(|i| average(&i.pixels));
            self.tex = img.map(|img| ui.ctx().load_texture("kilno-background", img, TextureOptions::LINEAR));
        }
        let Some(tex) = &self.tex else {
            BACKGROUND_ALPHA.store(255, Ordering::Relaxed);
            return;
        };
        ui.painter().image(tex.id(), rect, cover_uv(tex.size_vec2(), rect.size()), Color32::WHITE);
        BACKGROUND_ALPHA.store(255 - (strength.clamp(0, 100) * 255 / 100) as u8, Ordering::Relaxed);
    }
}

/// `c` as a big fill: see-through by the picture's strength while one is set.
pub fn veil(c: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), BACKGROUND_ALPHA.load(Ordering::Relaxed))
}

/// The mean of every 97th pixel: enough for a colour, quick on an 8K picture.
fn average(px: &[Color32]) -> Option<[u8; 3]> {
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for c in px.iter().step_by(97) {
        for (s, v) in sum.iter_mut().zip([c.r(), c.g(), c.b()]) {
            *s += u64::from(v);
        }
        n += 1;
    }
    (n > 0).then(|| sum.map(|s| (s / n) as u8))
}

/// The part of the picture that fills `area` without stretching: centred, the overflow cropped.
fn cover_uv(img: Vec2, area: Vec2) -> Rect {
    if img.x <= 0.0 || img.y <= 0.0 || area.x <= 0.0 || area.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let (ia, aa) = (img.x / img.y, area.x / area.y);
    let (w, h) = if ia > aa { (aa / ia, 1.0) } else { (1.0, ia / aa) };
    Rect::from_center_size(Pos2::new(0.5, 0.5), Vec2::new(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_of_two_colours() {
        assert_eq!(average(&[Color32::from_rgb(0, 100, 200), Color32::from_rgb(0, 100, 200)]), Some([0, 100, 200]));
        assert_eq!(average(&[]), None);
    }

    #[test]
    fn cover_crops_the_long_side_and_keeps_the_middle() {
        // A wide picture in a square: the middle half of its width.
        let r = cover_uv(Vec2::new(200.0, 100.0), Vec2::new(50.0, 50.0));
        assert_eq!((r.min, r.max), (Pos2::new(0.25, 0.0), Pos2::new(0.75, 1.0)));
        // A tall picture in a wide window: the middle of its height.
        let r = cover_uv(Vec2::new(100.0, 400.0), Vec2::new(200.0, 100.0));
        assert_eq!((r.min, r.max), (Pos2::new(0.0, 0.4375), Pos2::new(1.0, 0.5625)));
        assert_eq!(cover_uv(Vec2::ZERO, Vec2::new(1.0, 1.0)).max, Pos2::new(1.0, 1.0));
    }
}
