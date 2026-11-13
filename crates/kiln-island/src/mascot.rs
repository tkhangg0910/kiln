//! The Voxel mascot (KilnMascot.cs): a 64x64 grid of rects and two lens polygons, drawn with
//! tiny-skia without antialiasing (the SVG's crispEdges), every part on its own transform so the
//! moves can bend it. Geometry, colours and keyframes are PORTING.md Appendix D "DRAWING".

use crate::anim::{back_out, looped};
use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, PixmapMut,
    Point, Rect, Shader, SpreadMode, Transform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MascotMove {
    Idle,
    Talk,
    Walk,
    Wave,
    Peek,
    Alert,
    Jump,
}

/// Grid units. The whole figure squashes from its bottom centre, the right arm pivots on its
/// left edge, the mouth opens downwards from its top.
const WHOLE_PIVOT: (f32, f32) = (32.0, 52.0);
const ARM_PIVOT: (f32, f32) = (53.0, 32.5);
const MOUTH_PIVOT: (f32, f32) = (31.0, 37.0);

/// The glasses slide down 9 units in Peek.
pub const GLASSES_PEEK: f64 = 9.0;
/// The glasses' transition: 500 ms BackEase out, amplitude .45.
pub const GLASSES_MS: i64 = 500;
pub const GLASSES_AMPLITUDE: f64 = 0.45;

/// Every animated value of the figure at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rig {
    pub scale_x: f64,
    pub scale_y: f64,
    pub turn: f64,
    pub move_x: f64,
    pub move_y: f64,
    pub leg_a: f64,
    pub leg_b: f64,
    pub arm_l: f64,
    pub arm_r_y: f64,
    pub arm_r_turn: f64,
    pub mouth: f64,
    pub pupils: f64,
    pub glasses: f64,
}

impl Rig {
    pub const REST: Rig = Rig {
        scale_x: 1.0,
        scale_y: 1.0,
        turn: 0.0,
        move_x: 0.0,
        move_y: 0.0,
        leg_a: 0.0,
        leg_b: 0.0,
        arm_l: 0.0,
        arm_r_y: 0.0,
        arm_r_turn: 0.0,
        mouth: 1.0,
        pupils: 0.0,
        glasses: 0.0,
    };

    /// The held pose: nothing moves, the glasses down in Peek.
    pub fn still(mv: MascotMove) -> Rig {
        Rig {
            glasses: glasses_for(mv),
            ..Rig::REST
        }
    }

    /// The pose `t` seconds into `mv` (all loops start together at 0), with the glasses where
    /// their own transition has them.
    pub fn at(mv: MascotMove, t: f64, glasses: f64) -> Rig {
        let mut r = Rig {
            glasses,
            ..Rig::REST
        };
        // Only one move can own the whole figure: Alert > Jump > Walk > breathing.
        match mv {
            MascotMove::Alert => {
                r.move_x = looped(&[(0.0, 0.0), (0.25, -1.5), (0.75, 1.5), (1.0, 0.0)], 0.45, t, None);
                r.turn = looped(&[(0.0, 0.0), (0.25, -2.0), (0.75, 2.0), (1.0, 0.0)], 0.45, t, None);
            }
            MascotMove::Jump => {
                // Twice, then still: a celebration, not a state.
                let n = Some(2);
                r.scale_x = looped(
                    &[(0.0, 1.0), (0.14, 1.12), (0.44, 0.95), (0.72, 1.09), (0.86, 0.98), (1.0, 1.0)],
                    1.0,
                    t,
                    n,
                );
                r.scale_y = looped(
                    &[(0.0, 1.0), (0.14, 0.86), (0.44, 1.07), (0.72, 0.91), (0.86, 1.02), (1.0, 1.0)],
                    1.0,
                    t,
                    n,
                );
                r.move_y = looped(&[(0.0, 0.0), (0.14, 0.0), (0.44, -10.0), (0.72, 0.0), (1.0, 0.0)], 1.0, t, n);
            }
            MascotMove::Walk => {
                r.move_y = looped(&[(0.0, 0.0), (0.5, -1.2), (1.0, 0.0)], 0.5, t, None);
                r.turn = looped(&[(0.0, 0.0), (0.5, 1.2), (1.0, 0.0)], 0.5, t, None);
            }
            _ => {
                r.scale_x = looped(&[(0.0, 1.0), (0.5, 1.018), (1.0, 1.0)], 3.6, t, None);
                r.scale_y = looped(&[(0.0, 1.0), (0.5, 0.982), (1.0, 1.0)], 3.6, t, None);
            }
        }
        match mv {
            MascotMove::Walk => {
                r.leg_a = looped(&[(0.0, 0.0), (0.5, -2.4), (1.0, 0.0)], 0.5, t, None);
                r.leg_b = looped(&[(0.0, -2.4), (0.5, 0.0), (1.0, -2.4)], 0.5, t, None);
                r.arm_l = looped(&[(0.0, 0.0), (0.5, 1.2), (1.0, 0.0)], 0.5, t, None);
                r.arm_r_y = looped(&[(0.0, 1.2), (0.5, 0.0), (1.0, 1.2)], 0.5, t, None);
            }
            MascotMove::Wave => {
                r.arm_r_turn = looped(&[(0.0, -18.0), (0.5, -60.0), (1.0, -18.0)], 1.1, t, None);
            }
            MascotMove::Talk => {
                r.mouth = looped(&[(0.0, 1.0), (0.4, 3.2), (0.7, 1.8), (1.0, 1.0)], 0.42, t, None);
            }
            MascotMove::Peek => {
                r.pupils = looped(
                    &[(0.0, 0.0), (0.2, 0.0), (0.35, -2.0), (0.5, -2.0), (0.65, 2.0), (0.8, 2.0), (1.0, 0.0)],
                    2.4,
                    t,
                    None,
                );
            }
            _ => {}
        }
        r
    }
}

/// Where the glasses rest for this move.
pub fn glasses_for(mv: MascotMove) -> f64 {
    if mv == MascotMove::Peek {
        GLASSES_PEEK
    } else {
        0.0
    }
}

/// The glasses' transition value `ms` into it, from `from` to `to`.
pub fn glasses_at(from: f64, to: f64, ms: i64) -> f64 {
    if ms >= GLASSES_MS {
        return to;
    }
    let t = (ms.max(0) as f64) / GLASSES_MS as f64;
    from + (to - from) * back_out(t, GLASSES_AMPLITUDE)
}

/// Whether the figure is still moving `t` seconds into `mv`: everything loops forever except
/// the jump, which stops after its two hops.
pub fn moving(mv: MascotMove, t: f64) -> bool {
    mv != MascotMove::Jump || t < 2.0
}

fn hex(rgb: u32) -> Color {
    Color::from_rgba8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255)
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> Path {
    PathBuilder::from_rect(Rect::from_xywh(x, y, w, h).expect("valid rect"))
}

fn add_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32) {
    pb.push_rect(Rect::from_xywh(x, y, w, h).expect("valid rect"));
}

fn add_poly(pb: &mut PathBuilder, xy: &[f32], d: f32) {
    pb.move_to(xy[0] + d, xy[1] + d);
    for p in xy[2..].chunks(2) {
        pb.line_to(p[0] + d, p[1] + d);
    }
    pb.close();
}

/// Top to bottom over the box `y0..y1`, like the SVG's objectBoundingBox default.
fn gradient(y0: f32, y1: f32, top: u32, bottom: u32) -> Shader<'static> {
    LinearGradient::new(
        Point::from_xy(0.0, y0),
        Point::from_xy(0.0, y1),
        vec![GradientStop::new(0.0, hex(top)), GradientStop::new(1.0, hex(bottom))],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .expect("two stops")
}

fn about(t: Transform, (cx, cy): (f32, f32)) -> Transform {
    Transform::from_translate(-cx, -cy)
        .post_concat(t)
        .post_translate(cx, cy)
}

fn solid(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = false;
    p
}

fn shaded(s: Shader<'static>) -> Paint<'static> {
    Paint {
        shader: s,
        anti_alias: false,
        ..Paint::default()
    }
}

/// Draws the figure in grid units through `base` (grid → pixmap pixels).
pub fn draw(pm: &mut PixmapMut, rig: &Rig, base: Transform) {
    let f = |v: f64| v as f32;
    let whole = about(Transform::from_scale(f(rig.scale_x), f(rig.scale_y)), WHOLE_PIVOT)
        .post_concat(Transform::from_rotate_at(f(rig.turn), WHOLE_PIVOT.0, WHOLE_PIVOT.1))
        .post_translate(f(rig.move_x), f(rig.move_y))
        .post_concat(base);
    let leg_a = Transform::from_translate(0.0, f(rig.leg_a));
    let leg_b = Transform::from_translate(0.0, f(rig.leg_b));
    let arm_l = Transform::from_translate(0.0, f(rig.arm_l));
    let arm_r = Transform::from_rotate_at(f(rig.arm_r_turn), ARM_PIVOT.0, ARM_PIVOT.1)
        .post_translate(0.0, f(rig.arm_r_y));
    let id = Transform::identity();
    let parts: [((f32, f32, f32, f32), Transform); 7] = [
        ((13.0, 44.0, 3.0, 8.0), leg_a),
        ((20.0, 44.0, 3.0, 8.0), leg_b),
        ((39.0, 44.0, 3.0, 8.0), leg_b),
        ((46.0, 44.0, 3.0, 8.0), leg_a),
        ((2.0, 28.0, 9.0, 9.0), arm_l),
        ((53.0, 28.0, 9.0, 9.0), arm_r),
        ((10.0, 12.0, 43.0, 33.0), id),
    ];

    // All the dark copies behind all the faces (the body's copy does not cover the legs).
    let dark = solid(hex(0x6e2d1b));
    for ((x, y, w, h), t) in parts {
        let mut pb = PathBuilder::new();
        for d in [0.7, 1.4, 2.1] {
            add_rect(&mut pb, x + d, y + d, w, h);
        }
        if let Some(p) = pb.finish() {
            pm.fill_path(&p, &dark, FillRule::Winding, t.post_concat(whole), None);
        }
    }
    for ((x, y, w, h), t) in parts {
        let paint = shaded(gradient(y, y + h, 0xe07f60, 0xb55a3f));
        pm.fill_path(&rect_path(x, y, w, h), &paint, FillRule::Winding, t.post_concat(whole), None);
    }

    // Light from the top left: a bright top edge and a fainter left edge.
    let mut top = Color::WHITE;
    top.apply_opacity(0.28);
    pm.fill_path(&rect_path(10.0, 12.0, 43.0, 1.0), &solid(top), FillRule::Winding, whole, None);
    let mut left = Color::WHITE;
    left.apply_opacity(0.12);
    pm.fill_path(&rect_path(10.0, 12.0, 1.0, 33.0), &solid(left), FillRule::Winding, whole, None);

    let mut eyes = PathBuilder::new();
    add_rect(&mut eyes, 15.0, 23.0, 8.0, 6.0);
    add_rect(&mut eyes, 37.0, 23.0, 8.0, 6.0);
    if let Some(p) = eyes.finish() {
        pm.fill_path(&p, &solid(hex(0xf4f6f8)), FillRule::Winding, whole, None);
    }
    let mut pupils = PathBuilder::new();
    add_rect(&mut pupils, 18.0, 25.0, 3.0, 3.0);
    add_rect(&mut pupils, 40.0, 25.0, 3.0, 3.0);
    if let Some(p) = pupils.finish() {
        let t = Transform::from_translate(f(rig.pupils), 0.0).post_concat(whole);
        pm.fill_path(&p, &solid(hex(0x1b2126)), FillRule::Winding, t, None);
    }
    let mouth = about(Transform::from_scale(1.0, f(rig.mouth)), MOUTH_PIVOT).post_concat(whole);
    pm.fill_path(&rect_path(29.0, 37.0, 4.0, 1.0), &solid(hex(0x2a0e06)), FillRule::Winding, mouth, None);

    // The glasses are one rigid piece.
    let g = Transform::from_translate(0.0, f(rig.glasses)).post_concat(whole);
    const LENS_L: [f32; 12] = [11.0, 21.0, 28.0, 21.0, 28.0, 30.0, 26.0, 32.0, 13.0, 32.0, 11.0, 30.0];
    const LENS_R: [f32; 12] = [32.0, 21.0, 50.0, 21.0, 50.0, 30.0, 48.0, 32.0, 34.0, 32.0, 32.0, 30.0];
    let mut ext = PathBuilder::new();
    for d in [0.6, 1.2] {
        add_rect(&mut ext, 9.0 + d, 20.0 + d, 45.0, 2.0);
        add_poly(&mut ext, &LENS_L, d);
        add_poly(&mut ext, &LENS_R, d);
        add_rect(&mut ext, 28.0 + d, 22.0 + d, 4.0, 2.0);
    }
    if let Some(p) = ext.finish() {
        pm.fill_path(&p, &solid(hex(0x050607)), FillRule::Winding, g, None);
    }
    let frame = solid(hex(0x1a1f23));
    pm.fill_path(&rect_path(9.0, 20.0, 45.0, 2.0), &frame, FillRule::Winding, g, None);
    let mut lenses = PathBuilder::new();
    add_poly(&mut lenses, &LENS_L, 0.0);
    add_poly(&mut lenses, &LENS_R, 0.0);
    if let Some(p) = lenses.finish() {
        // One gradient over the box of both lenses.
        let paint = shaded(gradient(21.0, 32.0, 0x3a4652, 0x0c1013));
        pm.fill_path(&p, &paint, FillRule::Winding, g, None);
    }
    pm.fill_path(&rect_path(28.0, 22.0, 4.0, 2.0), &frame, FillRule::Winding, g, None);
    let mut glints = PathBuilder::new();
    for (x, y, w, h) in [(12.0, 22.0, 2.0, 1.0), (44.0, 22.0, 2.0, 1.0), (45.0, 23.0, 2.0, 1.0), (46.0, 24.0, 1.0, 1.0)] {
        add_rect(&mut glints, x, y, w, h);
    }
    if let Some(p) = glints.finish() {
        pm.fill_path(&p, &solid(Color::WHITE), FillRule::Winding, g, None);
    }
}

/// The grid → pixels transform for a `size` px square at (x, y): the Viewbox's uniform scale.
pub fn placed(x: f32, y: f32, size: f32) -> Transform {
    Transform::from_scale(size / 64.0, size / 64.0).post_translate(x, y)
}

/// The mascot alone on a transparent `size`x`size` pixmap. `t` is seconds into the move, or
/// None for the held pose (what the setup window and the checks use).
pub fn render(mv: MascotMove, t: Option<f64>, size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size.max(1), size.max(1)).expect("non-zero size");
    let rig = match t {
        Some(t) => Rig::at(mv, t, glasses_at(0.0, glasses_for(mv), (t * 1000.0) as i64)),
        None => Rig::still(mv),
    };
    draw(&mut pm.as_mut(), &rig, placed(0.0, 0.0, size as f32));
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let c = pm.pixel(x, y).expect("in bounds");
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    #[test]
    fn still_pose_is_identity_but_for_the_glasses() {
        assert_eq!(Rig::still(MascotMove::Idle), Rig::REST);
        assert_eq!(Rig::still(MascotMove::Peek).glasses, 9.0);
    }

    #[test]
    fn moves_follow_their_keyframes() {
        let a = Rig::at(MascotMove::Alert, 0.45 * 0.25, 0.0);
        assert!((a.move_x + 1.5).abs() < 1e-9 && (a.turn + 2.0).abs() < 1e-9);
        let w = Rig::at(MascotMove::Wave, 0.55, 0.0);
        assert!((w.arm_r_turn + 60.0).abs() < 1e-9);
        // Breathing underneath the wave.
        assert!((Rig::at(MascotMove::Wave, 1.8, 0.0).scale_x - 1.018).abs() < 1e-9);
        let j = Rig::at(MascotMove::Jump, 0.44, 0.0);
        assert!((j.move_y + 10.0).abs() < 1e-9);
        assert_eq!(Rig::at(MascotMove::Jump, 2.44, 0.0).move_y, 0.0);
        assert!(!moving(MascotMove::Jump, 2.0) && moving(MascotMove::Jump, 1.9));
        assert!(moving(MascotMove::Idle, 1000.0));
        let t = Rig::at(MascotMove::Talk, 0.42 * 0.4, 0.0);
        assert!((t.mouth - 3.2).abs() < 1e-9);
        let walk = Rig::at(MascotMove::Walk, 0.0, 0.0);
        assert_eq!((walk.leg_b, walk.arm_r_y), (-2.4, 1.2));
    }

    #[test]
    fn glasses_spring_into_place() {
        assert_eq!(glasses_at(0.0, 9.0, 0), 0.0);
        assert_eq!(glasses_at(0.0, 9.0, 500), 9.0);
        assert!(glasses_at(0.0, 9.0, 350) > 9.0, "overshoots");
    }

    #[test]
    fn renders_the_body_and_the_glasses() {
        let pm = render(MascotMove::Idle, None, 64);
        // Body face near its top (gradient top colour), not covered by anything.
        let body = px(&pm, 30, 14);
        assert_eq!(body[3], 255);
        assert!(body[0] > 200 && body[1] > 110 && body[1] < 140, "{body:?}");
        // Eye white between the glasses bar and the lens bottom, under the lens: lens colour.
        let lens = px(&pm, 20, 26);
        assert!(lens[0] < 80 && lens[3] == 255, "{lens:?}");
        // Corners are transparent.
        assert_eq!(px(&pm, 0, 0)[3], 0);
        assert_eq!(px(&pm, 63, 63)[3], 0);
        // Peek: glasses slid down 9 units, so the eye whites show at y 24.
        let peek = render(MascotMove::Peek, None, 64);
        assert_eq!(px(&peek, 16, 24), [0xf4, 0xf6, 0xf8, 255]);
    }

    #[test]
    fn aliased_edges() {
        let pm = render(MascotMove::Idle, None, 44);
        for p in pm.pixels() {
            assert!(p.alpha() == 0 || p.alpha() == 255, "only crisp edges");
        }
    }
}
