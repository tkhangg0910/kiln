//! Text for the pill, since tiny-skia draws none: ab_glyph over system fonts, memory-mapped so
//! their bytes stay out of the private working set, with a per-glyph fallback chain
//! Segoe UI (Variable Text when present) → Segoe UI Symbol → Segoe UI Emoji → Microsoft YaHei UI.

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, VariableFont};
use std::path::{Path, PathBuf};
use tiny_skia::{Color, PixmapMut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Semibold,
}

/// One run of text in one weight, size (logical px, WPF FontSize) and colour.
#[derive(Clone, Copy, Debug)]
pub struct Span<'a> {
    pub text: &'a str,
    pub weight: Weight,
    pub size: f32,
    pub color: Color,
}

impl<'a> Span<'a> {
    pub fn new(text: &'a str, weight: Weight, size: f32, color: Color) -> Self {
        Self {
            text,
            weight,
            size,
            color,
        }
    }
}

/// The two fallback chains, regular and semibold.
pub struct Fonts {
    regular: Vec<FontRef<'static>>,
    semibold: Vec<FontRef<'static>>,
}

/// `%WINDIR%\Fonts`.
pub fn fonts_dir() -> PathBuf {
    let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    windir.join("Fonts")
}

fn load(dir: &Path, file: &str) -> Option<FontRef<'static>> {
    let bytes = crate::sys::mmap::map_font(&dir.join(file))?;
    FontRef::try_from_slice_and_index(bytes, 0).ok()
}

impl Fonts {
    /// The system fonts, or None when not even Segoe UI is there.
    pub fn system() -> Option<Fonts> {
        Self::from_dir(&fonts_dir())
    }

    pub fn from_dir(dir: &Path) -> Option<Fonts> {
        // "Segoe UI Variable Text, Segoe UI": the variable font at the Text optical size.
        let (regular, semibold) = match load(dir, "SegUIVar.ttf") {
            Some(var) => {
                let mut r = var.clone();
                r.set_variation(b"wght", 400.0);
                r.set_variation(b"opsz", 10.5);
                let mut s = var;
                s.set_variation(b"wght", 600.0);
                s.set_variation(b"opsz", 10.5);
                (r, s)
            }
            None => {
                let r = load(dir, "segoeui.ttf")?;
                let s = load(dir, "seguisb.ttf").unwrap_or_else(|| r.clone());
                (r, s)
            }
        };
        let fallback: Vec<FontRef<'static>> = ["seguisym.ttf", "seguiemj.ttf", "msyh.ttc", "YuGothM.ttc", "malgun.ttf"]
            .iter()
            .filter_map(|f| load(dir, f))
            .collect();
        let mut reg = vec![regular];
        reg.extend(fallback.iter().cloned());
        let mut semi = vec![semibold];
        semi.extend(fallback);
        Some(Fonts {
            regular: reg,
            semibold: semi,
        })
    }

    fn chain(&self, w: Weight) -> &[FontRef<'static>] {
        match w {
            Weight::Regular => &self.regular,
            Weight::Semibold => &self.semibold,
        }
    }

    fn pick(&self, w: Weight, c: char) -> (&FontRef<'static>, GlyphId) {
        let chain = self.chain(w);
        for f in chain {
            let id = f.glyph_id(c);
            if id.0 != 0 {
                return (f, id);
            }
        }
        (&chain[0], chain[0].glyph_id(c))
    }

    /// The ab_glyph scale for an em size of `size` px.
    fn scale(f: &FontRef<'static>, size: f32) -> PxScale {
        let upem = f.units_per_em().unwrap_or(2048.0);
        PxScale::from(size * f.height_unscaled() / upem)
    }

    /// Ascent and line height of the primary font at `size`, as WPF lays a line out.
    pub fn line(&self, size: f32) -> (f32, f32) {
        let f = &self.regular[0];
        let s = f.as_scaled(Self::scale(f, size));
        (s.ascent(), s.ascent() - s.descent() + s.line_gap())
    }

    fn layout(&self, spans: &[Span]) -> Vec<Placed> {
        let mut out = Vec::new();
        let mut x = 0.0;
        for (si, span) in spans.iter().enumerate() {
            let mut prev: Option<(usize, GlyphId)> = None;
            for c in span.text.chars() {
                let (font, id) = self.pick(span.weight, c);
                let fi = self.chain(span.weight).iter().position(|f| std::ptr::eq(f, font)).unwrap_or(0);
                let sf = font.as_scaled(Self::scale(font, span.size));
                if let Some((pfi, pid)) = prev {
                    if pfi == fi {
                        x += sf.kern(pid, id);
                    }
                }
                let adv = sf.h_advance(id);
                out.push(Placed {
                    span: si,
                    font: fi,
                    id,
                    x,
                    adv,
                });
                x += adv;
                prev = Some((fi, id));
            }
        }
        out
    }

    /// Width of the spans in logical px.
    pub fn measure(&self, spans: &[Span]) -> f32 {
        self.layout(spans).last().map_or(0.0, |p| p.x + p.adv)
    }

    /// Draws the spans on one line: left at `x`, baseline at `baseline` (logical px), cut with
    /// "…" to `max_w` like WPF's CharacterEllipsis. Everything is multiplied by `scale`.
    pub fn draw(&self, pm: &mut PixmapMut, spans: &[Span], x: f32, baseline: f32, max_w: Option<f32>, scale: f32) {
        let mut glyphs = self.layout(spans);
        let total = glyphs.last().map_or(0.0, |p| p.x + p.adv);
        let mut ellipsis: Option<(usize, f32)> = None;
        if let Some(max) = max_w.filter(|m| total > *m) {
            // Keep what fits with the ellipsis after it, in the weight of the run it cuts.
            let mut keep = 0;
            for (i, g) in glyphs.iter().enumerate() {
                let span = &spans[g.span];
                let (ef, eid) = self.pick(span.weight, '…');
                let ew = ef.as_scaled(Self::scale(ef, span.size)).h_advance(eid);
                if g.x + g.adv + ew > max {
                    break;
                }
                keep = i + 1;
            }
            let at_span = glyphs.get(keep).or(glyphs.last()).map_or(0, |g| g.span);
            let at_x = if keep == 0 { 0.0 } else { glyphs[keep - 1].x + glyphs[keep - 1].adv };
            glyphs.truncate(keep);
            ellipsis = Some((at_span, at_x));
        }
        for g in &glyphs {
            let span = &spans[g.span];
            let font = &self.chain(span.weight)[g.font];
            self.blit(pm, font, g.id, span.size * scale, (x + g.x) * scale, baseline * scale, span.color);
        }
        if let Some((si, ex)) = ellipsis {
            let span = &spans[si];
            let (font, id) = self.pick(span.weight, '…');
            self.blit(pm, font, id, span.size * scale, (x + ex) * scale, baseline * scale, span.color);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit(&self, pm: &mut PixmapMut, font: &FontRef<'static>, id: GlyphId, size: f32, x: f32, y: f32, color: Color) {
        let glyph = id.with_scale_and_position(Self::scale(font, size), ab_glyph::point(x, y));
        let Some(outline) = font.outline_glyph(glyph) else {
            return;
        };
        let bounds = outline.px_bounds();
        let (w, h) = (pm.width() as i32, pm.height() as i32);
        let c = color.premultiply().to_color_u8();
        let (cr, cg, cb, ca) = (c.red(), c.green(), c.blue(), c.alpha());
        let data = pm.data_mut();
        outline.draw(|gx, gy, cov| {
            let px = bounds.min.x as i32 + gx as i32;
            let py = bounds.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= w || py >= h {
                return;
            }
            // A touch of gamma, so light text on black does not look thin.
            let cov = cov.clamp(0.0, 1.0).powf(0.8);
            let i = ((py * w + px) * 4) as usize;
            let blend = |dst: u8, src: u8| -> u8 {
                let s = f32::from(src) * cov;
                let a = f32::from(ca) * cov / 255.0;
                (s + f32::from(dst) * (1.0 - a)).round().clamp(0.0, 255.0) as u8
            };
            data[i] = blend(data[i], cr);
            data[i + 1] = blend(data[i + 1], cg);
            data[i + 2] = blend(data[i + 2], cb);
            data[i + 3] = blend(data[i + 3], ca);
        });
    }
}

struct Placed {
    span: usize,
    font: usize,
    id: GlyphId,
    x: f32,
    adv: f32,
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn measures_and_cuts_with_an_ellipsis() {
        let Some(fonts) = Fonts::system() else { return };
        let white = Color::WHITE;
        let long = [Span::new("a very long session name that goes on and on", Weight::Semibold, 13.0, white)];
        let w = fonts.measure(&long);
        assert!(w > 200.0, "{w}");
        let mut pm = Pixmap::new(200, 30).expect("pixmap");
        fonts.draw(&mut pm.as_mut(), &long, 0.0, 20.0, Some(100.0), 1.0);
        // Nothing drawn past the cut.
        for y in 0..30 {
            for x in 102..200 {
                assert_eq!(pm.pixel(x, y).expect("px").alpha(), 0);
            }
        }
        assert!(pm.pixels().iter().any(|p| p.alpha() > 0));
        // CJK falls back to another font rather than drawing boxes.
        let cjk = [Span::new("会话", Weight::Regular, 12.0, white)];
        assert!(fonts.measure(&cjk) > 15.0);
        let (ascent, line) = fonts.line(13.0);
        assert!(ascent > 10.0 && line > 15.0 && line < 20.0, "{ascent} {line}");
    }
}
