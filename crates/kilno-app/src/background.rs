//! The window's background picture (Settings › General). Drawn faintly over everything, as
//! VS Code's background extensions do, so panes and panels need no transparency of their own.

use egui::{Color32, Context, Id, LayerId, Order, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

#[derive(Default)]
pub struct Background {
    /// The path the texture was made from; reloaded when Settings names another.
    path: Option<String>,
    tex: Option<TextureHandle>,
}

impl Background {
    pub fn paint(&mut self, ctx: &Context, rect: Rect, path: Option<&str>, opacity: i32) {
        if self.path.as_deref() != path {
            self.path = path.map(str::to_string);
            // shortcut: decoded on the UI thread, once per pick; move to a thread if big files stall.
            self.tex = path
                .and_then(|p| crate::image_chip::decode(std::path::Path::new(p), 0))
                .map(|img| ctx.load_texture("kilno-background", img, TextureOptions::LINEAR));
        }
        let Some(tex) = &self.tex else { return };
        let alpha = (opacity.clamp(0, 100) * 255 / 100) as u8;
        // Above the window's own background layer, below dialogs and popups.
        ctx.layer_painter(LayerId::new(Order::Background, Id::NULL.with("kilno-background")))
            .image(tex.id(), rect, cover_uv(tex.size_vec2(), rect.size()), Color32::from_white_alpha(alpha));
    }
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
