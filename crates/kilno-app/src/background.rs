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
    /// The picture's most striking colour, darkened, for a title bar that matches it.
    pub accent: Option<[u8; 3]>,
}

impl Background {
    /// Before anything else of the frame is drawn.
    pub fn paint(&mut self, ui: &Ui, rect: Rect, path: Option<&str>, strength: i32) {
        if self.path.as_deref() != path {
            self.path = path.map(str::to_string);
            // shortcut: decoded on the UI thread, once per pick; move to a thread if big files stall.
            let img = path.and_then(|p| crate::image_chip::decode(std::path::Path::new(p), 0));
            self.accent = img.as_ref().and_then(|i| accent(&i.pixels));
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

/// `c` as a big fill: see-through by the picture's strength while one is set. Each spot of the
/// window gets one veil only: a second on top darkens it past the strength the user chose.
pub fn veil(c: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), BACKGROUND_ALPHA.load(Ordering::Relaxed))
}

/// The fill of something drawn on a spot already veiled (a panel beside the terminal): nothing
/// while a picture shows, `c` otherwise.
pub fn over_veil(c: Color32) -> Color32 {
    if BACKGROUND_ALPHA.load(Ordering::Relaxed) == 255 { c } else { Color32::TRANSPARENT }
}

/// A small fill on a veiled spot (a hovered or chosen row, the sidebar's footer band, a button):
/// `c` without a picture; with one, a see-through white (`lift` > 0) or black (`lift` < 0) of
/// that strength, so the row lightens or darkens what is under it and the picture shows through.
pub fn shade(c: Color32, lift: f32) -> Color32 {
    if BACKGROUND_ALPHA.load(Ordering::Relaxed) == 255 {
        return c;
    }
    let a = (lift.abs().clamp(0.0, 1.0) * 255.0) as u8;
    if lift >= 0.0 { Color32::from_white_alpha(a) } else { Color32::from_black_alpha(a) }
}

/// The picture's most striking colour, as Android's palette or Spotify's covers pick one: the
/// hue (in 12 bands) with the most vivid area, averaged within that band, then darkened enough
/// for a title bar. A picture with no vivid colour gives its plain mean. Every 97th pixel is
/// enough, and quick on an 8K picture.
fn accent(px: &[Color32]) -> Option<[u8; 3]> {
    let mut bands = [(0.0f64, [0.0f64; 3]); 12];
    let (mut mean, mut n) = ([0.0f64; 3], 0.0);
    for c in px.iter().step_by(97) {
        let rgb = [c.r(), c.g(), c.b()].map(f64::from);
        for (m, v) in mean.iter_mut().zip(rgb) {
            *m += v;
        }
        n += 1.0;
        let (max, min) = (rgb.iter().copied().fold(0.0, f64::max), rgb.iter().copied().fold(255.0, f64::min));
        let (v, s) = (max / 255.0, if max > 0.0 { (max - min) / max } else { 0.0 });
        if s < 0.25 || v < 0.2 {
            continue; // greys, near-black: not a colour to pick
        }
        let w = s * v;
        let band = &mut bands[(hue(rgb, max, min) / 30.0) as usize % 12];
        band.0 += w;
        for (sum, x) in band.1.iter_mut().zip(rgb) {
            *sum += x * w;
        }
    }
    if n == 0.0 {
        return None;
    }
    let (w, sum) = bands.iter().copied().fold((0.0, [0.0; 3]), |best, b| if b.0 > best.0 { b } else { best });
    let rgb = if w > 0.0 { sum.map(|x| x / w) } else { mean.map(|x| x / n) };
    // A title bar, not a poster: brightness at most 45 %, the hue and saturation kept.
    let max = rgb.iter().copied().fold(1.0, f64::max);
    let scale = (0.45 * 255.0 / max).min(1.0);
    Some(rgb.map(|x| (x * scale).round() as u8))
}

/// Hue in degrees, 0..360.
fn hue([r, g, b]: [f64; 3], max: f64, min: f64) -> f64 {
    let d = max - min;
    if d == 0.0 {
        return 0.0;
    }
    let h = if max == r { (g - b) / d } else if max == g { 2.0 + (b - r) / d } else { 4.0 + (r - g) / d };
    (h * 60.0).rem_euclid(360.0)
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
    fn accent_is_the_vivid_colour_not_the_mud() {
        // Mostly grey and skin, a fair share of cyan: the cyan wins, darkened to 45 %.
        let mut px = vec![Color32::from_rgb(120, 120, 120); 97 * 60];
        px.extend(vec![Color32::from_rgb(40, 200, 210); 97 * 30]);
        px.extend(vec![Color32::from_rgb(230, 190, 170); 97 * 10]);
        let a = accent(&px).unwrap();
        assert!(a[2] > a[0] && a[1] > a[0], "cyan, not grey or skin: {a:?}");
        assert_eq!(a.iter().max(), Some(&115), "brightness capped at 45 %");
        // All grey: its mean, already dark enough.
        assert_eq!(accent(&[Color32::from_rgb(60, 60, 60); 200]), Some([60, 60, 60]));
        assert_eq!(accent(&[]), None);
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
