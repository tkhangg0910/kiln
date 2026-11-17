//! The light under the island and the dwell bar (IslandGlow.cs): three stacked radial layers in
//! a 760x200 logical window, painted with tiny-skia. The animation state lives in
//! [`GlowAnim`]; the window that shows it is in the Win32 part.

use crate::anim::{breath, Ease, Tween};
use tiny_skia::{Color, GradientStop, LinearGradient, Paint, PixmapMut, Point, Rect, SpreadMode, Transform};
#[cfg(test)]
use tiny_skia::RadialGradient;

pub const GLOW_WIDTH: f64 = 760.0;
pub const GLOW_HEIGHT: f64 = 200.0;

pub type Rgb = (u8, u8, u8);

pub const WARM: Rgb = (255, 186, 140);
pub const BLUE: Rgb = (120, 165, 255);
pub const AMBER: Rgb = (255, 168, 60);
pub const GREEN: Rgb = (60, 214, 160);

/// The core's colour: the state's colour taken halfway to white.
pub fn lift(c: Rgb) -> Rgb {
    let h = |v: u8| ((u16::from(v) + 255) / 2) as u8;
    (h(c.0), h(c.1), h(c.2))
}

/// The aura's colour: each state's colour leaning to a neighbouring hue.
pub fn aura(c: Rgb) -> Rgb {
    match c {
        BLUE => (160, 130, 255),
        AMBER => (255, 120, 120),
        GREEN => (70, 200, 235),
        WARM => (255, 150, 180),
        other => other,
    }
}

/// A layer's alpha at radius `r` (0 centre .. 1 ellipse edge): a Gaussian with the tail
/// subtracted so it reaches exactly 0 at the ellipse.
pub fn falloff(peak: f64, k: f64, r: f64) -> f64 {
    if r >= 1.0 {
        return 0.0;
    }
    let tail = (-k).exp();
    peak * ((-k * r * r).exp() - tail) / (1.0 - tail)
}

/// How wide, tall and bright the light is under a pill `spread` (pill width / 420) wide.
pub fn spread(s: f64) -> (f64, f64, f64) {
    (0.35 + 0.65 * s, 0.55 + 0.45 * s, 0.5 + 0.5 * s)
}

/// One layer: peak, k, rx, ry, cy.
type Layer = (f64, f64, f64, f64, f64);
const AURA: Layer = (0.30, 2.2, 0.50, 0.80, 0.20);
const HALO: Layer = (0.50, 3.6, 0.45, 0.55, 0.20);
const CORE: Layer = (0.95, 5.0, 0.27, 0.20, 0.25);

/// Everything the glow window shows at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowFrame {
    pub tint: (f64, f64, f64),
    pub core_tint: (f64, f64, f64),
    pub aura_tint: (f64, f64, f64),
    pub scale_x: f64,
    pub scale_y: f64,
    pub drop: f64,
    /// The light group's opacity times the breath.
    pub light: f64,
    /// The core brush opacity times the core's own breath.
    pub core: f64,
    /// Dwell bar width in logical px (0 = none).
    pub bar: f64,
}

/// The tiny-skia gradient the glow was painted with before; kept to check [`paint`] against.
#[cfg(test)]
fn color(c: (f64, f64, f64), a: f64) -> Color {
    Color::from_rgba(
        (c.0 / 255.0) as f32,
        (c.1 / 255.0) as f32,
        (c.2 / 255.0) as f32,
        a.clamp(0.0, 1.0) as f32,
    )
    .unwrap_or(Color::TRANSPARENT)
}

#[cfg(test)]
fn layer(pm: &mut PixmapMut, (peak, k, rx, ry, cy): Layer, tint: (f64, f64, f64), opacity: f64, group: Transform) {
    if opacity <= 0.0 {
        return;
    }
    let stops: Vec<GradientStop> = (0..=32)
        .map(|s| {
            let r = f64::from(s) / 32.0;
            GradientStop::new(r as f32, color(tint, falloff(peak, k, r) * opacity))
        })
        .collect();
    let (w, h) = (GLOW_WIDTH as f32, GLOW_HEIGHT as f32);
    // A unit circle stretched to the ellipse (rx·w, ry·h) about (.5w, cy·h).
    let shape = Transform::from_scale((rx * GLOW_WIDTH) as f32, (ry * GLOW_HEIGHT) as f32)
        .post_translate(w / 2.0, (cy * GLOW_HEIGHT) as f32);
    let Some(shader) = RadialGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.0, 0.0),
        1.0,
        stops,
        SpreadMode::Pad,
        shape,
    ) else {
        return;
    };
    let paint = Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    };
    if let Some(rect) = Rect::from_xywh(0.0, 0.0, w, h) {
        pm.fill_rect(rect, &paint, group, None);
    }
}

/// Entries in a layer's falloff table, indexed by r² (0..1).
const LUT: usize = 1024;

/// One layer, ready for the pixel loop: its falloff by r², colour and where its ellipse is in
/// device pixels (centre, and 1/radius² on each axis).
struct Prepared {
    lut: [f32; LUT],
    rgb: [f32; 3],
    cx: f32,
    cy: f32,
    inv_rx2: f32,
    inv_ry2: f32,
}

fn prepare((peak, k, rx, ry, cy): Layer, tint: (f64, f64, f64), opacity: f64, f: &GlowFrame, scale: f64) -> Option<Prepared> {
    if opacity <= 0.0 || f.scale_x <= 0.0 || f.scale_y <= 0.0 {
        return None;
    }
    let mut lut = [0f32; LUT];
    for (i, v) in lut.iter_mut().enumerate() {
        let r2 = i as f64 / (LUT - 1) as f64;
        *v = (falloff(peak, k, r2.sqrt()) * opacity).clamp(0.0, 1.0) as f32;
    }
    // The ellipse (rx·w, ry·h) about (w/2, cy·h), squeezed about the centre line by the frame's
    // scale, dropped, then scaled to device pixels: the same group the window always used.
    let half = GLOW_WIDTH / 2.0;
    let rx_px = rx * GLOW_WIDTH * f.scale_x * scale;
    let ry_px = ry * GLOW_HEIGHT * f.scale_y * scale;
    Some(Prepared {
        lut,
        rgb: [tint.0 as f32 / 255.0, tint.1 as f32 / 255.0, tint.2 as f32 / 255.0],
        cx: (half * scale) as f32,
        cy: ((cy * GLOW_HEIGHT * f.scale_y + f.drop) * scale) as f32,
        inv_rx2: (1.0 / (rx_px * rx_px)) as f32,
        inv_ry2: (1.0 / (ry_px * ry_px)) as f32,
    })
}

/// Paints a frame onto a cleared pixmap `scale` times the logical size. The three layers are
/// worked out per pixel from a table (a few multiplies each) rather than as tiny-skia radial
/// gradients: those cost 30 ms a frame at 150 % and starved the island's clicks of its thread.
pub fn paint(pm: &mut PixmapMut, scale: f64, f: &GlowFrame) {
    let layers: Vec<Prepared> = [
        prepare(AURA, f.aura_tint, f.light, f, scale),
        prepare(HALO, f.tint, f.light, f, scale),
        prepare(CORE, f.core_tint, f.light * f.core, f, scale),
    ]
    .into_iter()
    .flatten()
    .collect();
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    // The layers were always clipped to the window's rectangle squeezed and dropped with them.
    let half = GLOW_WIDTH / 2.0;
    let (x0, x1) = ((half - f.scale_x * half) * scale, (half + f.scale_x * half) * scale);
    let (y0, y1) = (f.drop * scale, (f.scale_y * GLOW_HEIGHT + f.drop) * scale);
    let clip_x = |px: f32| f64::from(px) >= x0 && f64::from(px) < x1;
    let data = pm.data_mut();
    for y in 0..h {
        let py = y as f32 + 0.5;
        if f64::from(py) < y0 || f64::from(py) >= y1 {
            continue;
        }
        // Each layer's v² for this row; a row outside every ellipse is left clear.
        let dy: Vec<f32> = layers.iter().map(|l| (py - l.cy) * (py - l.cy) * l.inv_ry2).collect();
        if dy.iter().all(|v| *v >= 1.0) {
            continue;
        }
        // Only the columns some layer's ellipse reaches on this row.
        let (mut from, mut to) = (f32::MAX, f32::MIN);
        for (l, v2) in layers.iter().zip(&dy) {
            if *v2 < 1.0 {
                let half_w = ((1.0 - v2) / l.inv_rx2).sqrt();
                from = from.min(l.cx - half_w);
                to = to.max(l.cx + half_w);
            }
        }
        let from = (from.floor().max(0.0) as usize).min(w);
        let to = (to.ceil().max(0.0) as usize).min(w);
        let row = &mut data[y * w * 4..(y + 1) * w * 4];
        for x in from..to {
            let px = x as f32 + 0.5;
            if !clip_x(px) {
                continue;
            }
            let (mut r, mut g, mut b, mut a) = (0f32, 0f32, 0f32, 0f32);
            for (l, v2) in layers.iter().zip(&dy) {
                let r2 = (px - l.cx) * (px - l.cx) * l.inv_rx2 + v2;
                if r2 >= 1.0 {
                    continue;
                }
                let la = l.lut[(r2 * (LUT - 1) as f32) as usize];
                // Source over, premultiplied.
                let keep = 1.0 - la;
                r = l.rgb[0] * la + r * keep;
                g = l.rgb[1] * la + g * keep;
                b = l.rgb[2] * la + b * keep;
                a = la + a * keep;
            }
            if a > 0.0 {
                let px = &mut row[x * 4..x * 4 + 4];
                px[0] = (r * 255.0 + 0.5) as u8;
                px[1] = (g * 255.0 + 0.5) as u8;
                px[2] = (b * 255.0 + 0.5) as u8;
                px[3] = (a * 255.0 + 0.5) as u8;
            }
        }
    }

    let s = scale as f32;
    let cx = (GLOW_WIDTH / 2.0) as f32;
    if f.bar > 0.0 {
        let at = |a: f32| Color::from_rgba8(WARM.0, WARM.1, WARM.2, (a * 255.0) as u8);
        let x0 = cx - f.bar as f32 / 2.0;
        let Some(shader) = LinearGradient::new(
            Point::from_xy(x0, 0.0),
            Point::from_xy(x0 + f.bar as f32, 0.0),
            vec![
                GradientStop::new(0.0, at(0.0)),
                GradientStop::new(0.3, at(0.9)),
                GradientStop::new(0.7, at(0.9)),
                GradientStop::new(1.0, at(0.0)),
            ],
            SpreadMode::Pad,
            Transform::identity(),
        ) else {
            return;
        };
        let paint = Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        };
        if let Some(rect) = Rect::from_xywh(x0, 0.0, f.bar as f32, 2.0) {
            pm.fill_rect(rect, &paint, Transform::from_scale(s, s), None);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ColorTween {
    r: Tween,
    g: Tween,
    b: Tween,
}

impl ColorTween {
    fn new(c: Rgb) -> Self {
        Self {
            r: Tween::hold(f64::from(c.0)),
            g: Tween::hold(f64::from(c.1)),
            b: Tween::hold(f64::from(c.2)),
        }
    }
    fn value(&self, now: i64) -> (f64, f64, f64) {
        (self.r.value(now), self.g.value(now), self.b.value(now))
    }
    fn to(&mut self, now: i64, c: Rgb, dur: i64) {
        let e = Ease::CubicInOut;
        self.r.retarget(now, f64::from(c.0), dur, e);
        self.g.retarget(now, f64::from(c.1), dur, e);
        self.b.retarget(now, f64::from(c.2), dur, e);
    }
    fn running(&self, now: i64) -> bool {
        self.r.running(now)
    }
}

/// The glow's animation state, on the island's millisecond clock. The window code asks
/// [`GlowAnim::frame`] for what to paint and [`GlowAnim::visible`] whether to show at all.
#[derive(Clone, Debug)]
pub struct GlowAnim {
    lit: bool,
    fading: bool,
    dwelling: bool,
    fade_end: i64,
    tint: ColorTween,
    core_tint: ColorTween,
    aura_tint: ColorTween,
    sx: Tween,
    sy: Tween,
    core: Tween,
    drop: Tween,
    light: Tween,
    breath_since: Option<i64>,
    /// Dwell bar: started at, from width, over ms.
    bar: Option<(i64, f64, i64)>,
}

impl Default for GlowAnim {
    fn default() -> Self {
        Self {
            lit: false,
            fading: false,
            dwelling: false,
            fade_end: 0,
            tint: ColorTween::new(WARM),
            core_tint: ColorTween::new(lift(WARM)),
            aura_tint: ColorTween::new(aura(WARM)),
            sx: Tween::hold(1.0),
            sy: Tween::hold(1.0),
            core: Tween::hold(1.0),
            drop: Tween::hold(0.0),
            light: Tween::hold(0.0),
            breath_since: None,
            bar: None,
        }
    }
}

impl GlowAnim {
    pub fn lit(&self) -> bool {
        self.lit
    }

    /// Shown only while lit, fading or dwelling.
    pub fn visible(&self) -> bool {
        self.lit || self.fading || self.dwelling
    }

    pub fn dwelling(&self) -> bool {
        self.dwelling
    }

    /// Lights up in `c`: instantly from nothing, easing from the old colour otherwise.
    /// `breathe` is Windows' client-area animation setting.
    pub fn light(&mut self, now: i64, c: Rgb, spread_frac: f64, breathe: bool) {
        let fresh = !self.lit && !self.fading;
        self.lit = true;
        self.fading = false;
        let (wide, tall, core) = spread(spread_frac);
        if fresh {
            self.tint = ColorTween::new(c);
            self.core_tint = ColorTween::new(lift(c));
            self.aura_tint = ColorTween::new(aura(c));
            self.sx.set(wide);
            self.sy.set(tall);
            self.core.set(core);
        } else {
            self.tint.to(now, c, 500);
            self.core_tint.to(now, lift(c), 500);
            self.aura_tint.to(now, aura(c), 500);
            self.sx.retarget(now, wide, 400, Ease::BackOut(0.3));
            self.sy.retarget(now, tall, 400, Ease::BackOut(0.3));
            self.core.retarget(now, core, 400, Ease::CubicInOut);
        }
        self.drop.retarget(now, 0.0, 450, Ease::BackOut(0.3));
        self.light.retarget(now, 1.0, 450, Ease::Linear);
        if breathe && self.breath_since.is_none() {
            self.breath_since = Some(now);
        }
    }

    /// Stops the breathing; the next light breathes again.
    pub fn rest(&mut self) {
        self.breath_since = None;
    }

    /// Slides and fades out with the pill.
    pub fn dim(&mut self, now: i64, slide: f64) {
        if !self.lit {
            return;
        }
        self.lit = false;
        self.fading = true;
        self.drop.retarget(now, -slide, 260, Ease::CubicInOut);
        self.light.retarget(now, 0.0, 260, Ease::CubicInOut);
        self.fade_end = now + 260;
    }

    /// The dwell bar. Returns true when the window must be put up for it (it was not lit).
    pub fn dwell(&mut self, now: i64, progress: f64, length_ms: i64) -> bool {
        if progress <= 0.0 {
            if self.dwelling {
                self.dwelling = false;
                self.bar = None;
            }
            return false;
        }
        if self.dwelling {
            return false;
        }
        self.dwelling = true;
        let zone = kilno_core::island::trigger::ZONE_WIDTH;
        self.bar = Some((now, progress * zone, ((1.0 - progress) * length_ms as f64) as i64));
        !self.lit && !self.fading
    }

    /// Ends a finished fade. True when it just ended.
    pub fn settle(&mut self, now: i64) -> bool {
        if self.fading && !self.lit && now >= self.fade_end {
            self.fading = false;
            self.breath_since = None;
            return true;
        }
        false
    }

    fn bar_width(&self, now: i64) -> f64 {
        let zone = kilno_core::island::trigger::ZONE_WIDTH;
        match self.bar {
            None => 0.0,
            Some((start, from, dur)) => {
                if dur <= 0 || now >= start + dur {
                    zone
                } else {
                    from + (zone - from) * ((now - start) as f64 / dur as f64)
                }
            }
        }
    }

    /// Something still moves: a tween, the breath, a growing bar, or a fade to finish.
    pub fn animating(&self, now: i64) -> bool {
        if !self.visible() {
            return false;
        }
        self.breath_since.is_some()
            || self.fading
            || [self.sx, self.sy, self.core, self.drop, self.light]
                .iter()
                .any(|t| t.running(now))
            || self.tint.running(now)
            || self.bar.is_some_and(|(s, _, d)| now < s + d)
    }

    /// Only the breath moves (a 30 fps frame is plenty).
    pub fn breathing_only(&self, now: i64) -> bool {
        !self.fading
            && ![self.sx, self.sy, self.core, self.drop, self.light]
                .iter()
                .any(|t| t.running(now))
            && !self.tint.running(now)
            && !self.bar.is_some_and(|(s, _, d)| now < s + d)
    }

    pub fn frame(&self, now: i64) -> GlowFrame {
        let (b, cb) = match self.breath_since {
            Some(t0) => {
                let t = (now - t0) as f64 / 1000.0;
                (breath(0.6, 2.8, t), breath(0.7, 3.7, t))
            }
            None => (1.0, 1.0),
        };
        GlowFrame {
            tint: self.tint.value(now),
            core_tint: self.core_tint.value(now),
            aura_tint: self.aura_tint.value(now),
            scale_x: self.sx.value(now),
            scale_y: self.sy.value(now),
            drop: self.drop.value(now),
            light: self.light.value(now) * b,
            core: self.core.value(now) * cb,
            bar: self.bar_width(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    /// The per-pixel glow looks like the gradients it replaced: no channel more than a few
    /// levels off, at several scales, squeezes and drops.
    #[test]
    fn per_pixel_matches_the_gradients() {
        let frames = [
            GlowFrame { tint: (120.0, 165.0, 255.0), core_tint: (187.0, 210.0, 255.0), aura_tint: (160.0, 130.0, 255.0), scale_x: 1.0, scale_y: 1.0, drop: 0.0, light: 1.0, core: 1.0, bar: 0.0 },
            GlowFrame { tint: (60.0, 214.0, 160.0), core_tint: (157.0, 234.0, 207.0), aura_tint: (70.0, 200.0, 235.0), scale_x: 0.5, scale_y: 0.7, drop: 12.0, light: 0.6, core: 0.8, bar: 0.0 },
        ];
        for scale in [1.0, 1.5] {
            for f in &frames {
                let (w, h) = ((GLOW_WIDTH * scale) as u32, (GLOW_HEIGHT * scale) as u32);
                let mut new = Pixmap::new(w, h).unwrap();
                paint(&mut new.as_mut(), scale, f);
                let mut old = Pixmap::new(w, h).unwrap();
                let s = scale as f32;
                let cx = (GLOW_WIDTH / 2.0) as f32;
                let group = Transform::from_translate(-cx, 0.0)
                    .post_scale(f.scale_x as f32, f.scale_y as f32)
                    .post_translate(cx, f.drop as f32)
                    .post_scale(s, s);
                let mut pm = old.as_mut();
                layer(&mut pm, AURA, f.aura_tint, f.light, group);
                layer(&mut pm, HALO, f.tint, f.light, group);
                layer(&mut pm, CORE, f.core_tint, f.light * f.core, group);
                let worst = new.data().iter().zip(old.data()).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                assert!(worst <= 6, "scale {scale}: a channel {worst} levels off");
                assert!(new.data().iter().any(|v| *v > 0), "something is lit");
            }
        }
    }

    #[test]
    fn colours() {
        assert_eq!(lift(WARM), (255, 220, 197));
        assert_eq!(lift(BLUE), (187, 210, 255));
        assert_eq!(aura(GREEN), (70, 200, 235));
        assert_eq!(aura((1, 2, 3)), (1, 2, 3));
    }

    #[test]
    fn falloff_peaks_in_the_middle_and_ends_at_the_rim() {
        assert!((falloff(0.5, 3.6, 0.0) - 0.5).abs() < 1e-12);
        assert!(falloff(0.5, 3.6, 1.0).abs() < 1e-12);
        assert_eq!(falloff(0.5, 3.6, 1.5), 0.0);
        assert!(falloff(0.5, 3.6, 0.5) < 0.5 && falloff(0.5, 3.6, 0.5) > 0.0);
    }

    #[test]
    fn spread_follows_the_pill() {
        assert_eq!(spread(1.0), (1.0, 1.0, 1.0));
        let (w, h, c) = spread(92.0 / 420.0);
        assert!(w < 0.5 && h < 0.7 && c < 0.7);
    }

    #[test]
    fn light_dim_and_settle() {
        let mut g = GlowAnim::default();
        assert!(!g.visible());
        g.light(0, BLUE, 1.0, false);
        assert!(g.visible() && g.animating(100));
        assert_eq!(g.frame(450).tint, (120.0, 165.0, 255.0));
        assert!((g.frame(450).light - 1.0).abs() < 1e-9);
        // Recolour eases.
        g.light(1000, GREEN, 1.0, false);
        let mid = g.frame(1250).tint;
        assert!(mid.0 > 60.0 && mid.0 < 120.0);
        g.dim(2000, 60.0);
        assert!(g.visible());
        assert!(!g.settle(2200));
        assert!(g.settle(2260));
        assert!(!g.visible());
        assert!(!g.animating(3000));
    }

    #[test]
    fn dwell_bar_grows_to_the_zone() {
        let mut g = GlowAnim::default();
        assert!(g.dwell(0, 0.5, 600));
        assert_eq!(g.frame(0).bar, 120.0);
        assert_eq!(g.frame(150).bar, 180.0);
        assert_eq!(g.frame(300).bar, 240.0);
        assert!(!g.dwell(50, 0.6, 600), "one animation per dwell");
        g.dwell(400, 0.0, 600);
        assert_eq!(g.frame(400).bar, 0.0);
        assert!(!g.visible());
    }

    #[test]
    fn paints_light_under_the_centre() {
        let mut pm = Pixmap::new(760, 200).expect("pixmap");
        let mut g = GlowAnim::default();
        g.light(0, WARM, 1.0, false);
        paint(&mut pm.as_mut(), 1.0, &g.frame(1000));
        let centre = pm.pixel(380, 40).expect("px");
        let edge = pm.pixel(5, 195).expect("px");
        assert!(centre.alpha() > 150, "{}", centre.alpha());
        assert_eq!(edge.alpha(), 0);
    }
}
