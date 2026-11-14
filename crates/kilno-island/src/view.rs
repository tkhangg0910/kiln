//! The pill (IslandWindow.xaml): sizes, layout, hit tests and painting, in logical px inside the
//! 420 px wide island window. Pure apart from the fonts, so the maths is tested.

use crate::glow::{Rgb, BLUE, WARM};
use crate::mascot::{self, Rig};
use crate::text::{Fonts, Span, Weight};
use kilno_core::island::trigger::{IslandState, NoteKind};
use kilno_core::pane_status::PaneStatus;
use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, PixmapMut, Rect, Stroke, StrokeDash, Transform,
};

pub const PEEK_WIDTH: f64 = 300.0;
pub const COMPACT_WIDTH: f64 = 92.0;
pub const NOTE_WIDTH: f64 = 420.0;
pub const ISLAND_HEIGHT: f64 = 60.0;
pub const TALL_HEIGHT: f64 = 190.0;
pub const HEADER: f64 = 52.0;
pub const ROW: f64 = 34.0;
pub const DROP_BODY: f64 = 110.0;

const BRIGHT: u32 = 0xECEEF1;
const FAINT: u32 = 0x8B919A;

fn rgb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)
}

fn argb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8)
}

fn col(c: Rgb) -> Color {
    Color::from_rgba8(c.0, c.1, c.2, 255)
}

/// The pill's width for a state: the compact pill, the peek, or the wider note/drop/pick.
pub fn pill_width(state: IslandState, has_note: bool) -> f64 {
    match state {
        IslandState::Compact => COMPACT_WIDTH,
        IslandState::Drop | IslandState::Pick => NOTE_WIDTH,
        _ if has_note => NOTE_WIDTH,
        _ => PEEK_WIDTH,
    }
}

/// The window's height: the header, or the drop target, or the picker's rows (choices + New
/// session), each with the spring's slack.
pub fn window_height(state: IslandState, choices: usize) -> f64 {
    match state {
        IslandState::Drop => TALL_HEIGHT,
        IslandState::Pick => HEADER + ROW * (choices as f64 + 1.0) + 12.0 + 10.0,
        _ => ISLAND_HEIGHT,
    }
}

/// The pill's own height (WPF ActualHeight): the header plus whichever body shows.
pub fn pill_height(state: IslandState, choices: usize) -> f64 {
    match state {
        IslandState::Drop => HEADER + DROP_BODY + 14.0,
        IslandState::Pick => HEADER + ROW * (choices as f64 + 1.0) + 12.0,
        _ => HEADER,
    }
}

/// How far up the pill slides to leave: all the way, however far the body has grown it.
pub fn slide_out_to(pill_h: f64) -> f64 {
    -ISLAND_HEIGHT.max(pill_h + 8.0)
}

/// A rect in physical px: `logical` wide/tall, centred on the monitor (left..right), at `top`.
pub fn centred_px(mon_left: i32, mon_right: i32, top: i32, logical_w: f64, logical_h: f64, scale: f64) -> (i32, i32, i32, i32) {
    let w = (logical_w * scale).round() as i32;
    let h = (logical_h * scale).round() as i32;
    let x = (mon_left + mon_right) / 2 - w / 2;
    (x, top, w, h)
}

/// The note line: name (semibold) and the rest, as the sidebar words it.
pub fn note_rest(kind: NoteKind) -> &'static str {
    match kind {
        NoteKind::NeedsYou => " · needs you",
        NoteKind::Error => " · api error",
        NoteKind::Reserve => " · past your reserve",
        NoteKind::Tip => "",
        NoteKind::Done => " · finished",
    }
}

/// "1 session" / "N sessions".
pub fn sessions_line(n: usize) -> String {
    if n == 1 {
        "1 session".into()
    } else {
        format!("{n} sessions")
    }
}

/// The picker's hint under its title.
pub fn pick_hint(n: usize) -> String {
    if n == 0 {
        "N for a new session · Esc to cancel".into()
    } else {
        format!("1–{n} or N · Esc to cancel")
    }
}

/// The sidebar's status dot colours (App.xaml).
pub fn status_color(s: PaneStatus) -> Rgb {
    match s {
        PaneStatus::Working | PaneStatus::Starting => (0xD4, 0xA2, 0x4C),
        PaneStatus::NeedsInput => (0x5A, 0xA9, 0xE6),
        PaneStatus::Done => (0x6B, 0xBF, 0x8A),
        PaneStatus::Error => (0xD2, 0x60, 0x5F),
        PaneStatus::Exited => (0x5A, 0x64, 0x72),
    }
}

/// The compact pill's dot: the working blue, or warm for the login hello.
pub fn compact_dot(greeting: bool) -> Rgb {
    if greeting {
        WARM
    } else {
        BLUE
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickRow {
    pub key: String,
    pub name: String,
    pub folder: String,
    pub dot: Option<Rgb>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    None,
    Drop,
    Pick(Vec<PickRow>),
}

/// What the pill says, set when it slides in.
#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    pub compact: bool,
    pub dot: Rgb,
    pub head_name: String,
    pub head_rest: String,
    pub sub: Option<String>,
    pub body: Body,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            compact: false,
            dot: BLUE,
            head_name: "kilno".into(),
            head_rest: String::new(),
            sub: None,
            body: Body::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Login,
    Quit,
}

pub const MENU_WIDTH: f64 = 200.0;
const MENU_PAD: f64 = 4.0;
const MENU_ITEM: f64 = 30.0;
const MENU_SEP: f64 = 9.0;

/// The right-click menu, drawn inside the island window (TrackPopupMenu would take focus).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub login_on: bool,
    pub login_enabled: bool,
    pub hover: Option<MenuItem>,
}

impl Menu {
    pub fn height() -> f64 {
        MENU_PAD * 2.0 + MENU_ITEM * 2.0 + MENU_SEP
    }

    /// Opens at the click, kept inside the window's width.
    pub fn at(x: f64, y: f64, login_on: bool, login_enabled: bool) -> Menu {
        Menu {
            x: x.clamp(0.0, NOTE_WIDTH - MENU_WIDTH),
            y,
            login_on,
            login_enabled,
            hover: None,
        }
    }

    pub fn bottom(&self) -> f64 {
        self.y + Self::height()
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + MENU_WIDTH && y >= self.y && y < self.bottom()
    }

    fn item_top(item: MenuItem) -> f64 {
        match item {
            MenuItem::Login => MENU_PAD,
            MenuItem::Quit => MENU_PAD + MENU_ITEM + MENU_SEP,
        }
    }

    /// The item under (x, y); a disabled "Start at login" is not one.
    pub fn hit(&self, x: f64, y: f64) -> Option<MenuItem> {
        if !self.contains(x, y) {
            return None;
        }
        [MenuItem::Login, MenuItem::Quit].into_iter().find(|&it| {
            let top = self.y + Self::item_top(it);
            y >= top && y < top + MENU_ITEM && (it != MenuItem::Login || self.login_enabled)
        })
    }
}

/// The pill's left edge in the window.
pub fn pill_left(width: f64) -> f64 {
    (NOTE_WIDTH - width) / 2.0
}

/// Inside the pill (at rest, no slide).
pub fn in_pill(width: f64, height: f64, x: f64, y: f64) -> bool {
    let l = pill_left(width);
    x >= l && x < l + width && y >= 0.0 && y < height
}

/// The picker row under (x, y): 0.. for sessions, `rows - 1` for New session.
pub fn pick_row_at(width: f64, rows: usize, x: f64, y: f64) -> Option<usize> {
    let l = pill_left(width) + 10.0;
    let r = pill_left(width) + width - 10.0;
    if x < l || x >= r || y < HEADER {
        return None;
    }
    let i = ((y - HEADER) / ROW).floor() as usize;
    (i < rows).then_some(i)
}

/// Everything painted in one frame.
pub struct Scene<'a> {
    pub width: f64,
    pub height: f64,
    pub slide: f64,
    pub rig: Rig,
    pub content: &'a Content,
    pub hover_row: Option<usize>,
    pub drop_hover: bool,
    pub menu: Option<&'a Menu>,
}

fn fill(c: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(c);
    p.anti_alias = true;
    p
}

fn rounded(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: [f32; 4]) {
    // r = top-left, top-right, bottom-right, bottom-left. Quarter circles as cubics.
    const K: f32 = 0.552_284_8;
    let [tl, tr, br, bl] = r.map(|v| v.min(w / 2.0).min(h / 2.0).max(0.0));
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    if tr > 0.0 {
        pb.cubic_to(x + w - tr + tr * K, y, x + w, y + tr - tr * K, x + w, y + tr);
    }
    pb.line_to(x + w, y + h - br);
    if br > 0.0 {
        pb.cubic_to(x + w, y + h - br + br * K, x + w - br + br * K, y + h, x + w - br, y + h);
    }
    pb.line_to(x + bl, y + h);
    if bl > 0.0 {
        pb.cubic_to(x + bl - bl * K, y + h, x, y + h - bl + bl * K, x, y + h - bl);
    }
    pb.line_to(x, y + tl);
    if tl > 0.0 {
        pb.cubic_to(x, y + tl - tl * K, x + tl - tl * K, y, x + tl, y);
    }
    pb.close();
}

#[allow(clippy::too_many_arguments)]
fn round_rect(pm: &mut PixmapMut, x: f64, y: f64, w: f64, h: f64, r: [f32; 4], paint: &Paint, t: Transform) {
    let mut pb = PathBuilder::new();
    rounded(&mut pb, x as f32, y as f32, w as f32, h as f32, r);
    if let Some(p) = pb.finish() {
        pm.fill_path(&p, paint, FillRule::Winding, t, None);
    }
}

fn dot(pm: &mut PixmapMut, cx: f64, cy: f64, c: Rgb, t: Transform) {
    if let Some(p) = PathBuilder::from_circle(cx as f32, cy as f32, 4.0) {
        pm.fill_path(&p, &fill(col(c)), FillRule::Winding, t, None);
    }
}

/// Paints the scene onto a cleared pixmap `scale` times the window's logical size.
pub fn paint(pm: &mut PixmapMut, scale: f64, scene: &Scene, fonts: Option<&Fonts>) {
    let s = scale as f32;
    let t = Transform::from_scale(s, s);
    let w = scene.width;
    let x0 = pill_left(w);
    let y0 = scene.slide;
    let c = scene.content;

    // Black, square at the top, 24 px corners at the bottom.
    round_rect(pm, x0, y0, w, scene.height, [0.0, 0.0, 24.0, 24.0], &fill(Color::BLACK), t);

    // Header: 8 px in, the 44 px mascot, a 10 px gap, then the text or the dot.
    let m = mascot::placed(((x0 + 8.0) * scale) as f32, ((y0 + 4.0) * scale) as f32, (44.0 * scale) as f32);
    mascot::draw(pm, &scene.rig, m);
    let text_x = x0 + 8.0 + 44.0 + 10.0;
    if c.compact {
        dot(pm, text_x + 4.0, y0 + HEADER / 2.0, c.dot, t);
    } else if let Some(f) = fonts {
        let (a13, l13) = f.line(13.0);
        let (a12, l12) = f.line(12.0);
        let total = l13 + if c.sub.is_some() { l12 } else { 0.0 };
        let top = y0 as f32 + (HEADER as f32 - total) / 2.0;
        let max = (w - 62.0 - 22.0).max(0.0) as f32;
        let head = [
            Span::new(&c.head_name, Weight::Semibold, 13.0, rgb(BRIGHT)),
            Span::new(&c.head_rest, Weight::Regular, 13.0, rgb(FAINT)),
        ];
        f.draw(pm, &head, text_x as f32, top + a13, Some(max), s);
        if let Some(sub) = &c.sub {
            f.draw(pm, &[Span::new(sub, Weight::Regular, 12.0, rgb(FAINT))], text_x as f32, top + l13 + a12, Some(max), s);
        }
    }

    match &c.body {
        Body::None => {}
        Body::Drop => {
            let (bx, by, bw, bh) = (x0 + 14.0, y0 + HEADER, (w - 28.0).max(0.0), DROP_BODY);
            let warm = |a: u8| Color::from_rgba8(WARM.0, WARM.1, WARM.2, a);
            let mut pb = PathBuilder::new();
            rounded(&mut pb, bx as f32, by as f32, bw as f32, bh as f32, [16.0; 4]);
            if let Some(p) = pb.finish() {
                let f = fill(warm(if scene.drop_hover { 0x29 } else { 0x0F }));
                pm.fill_path(&p, &f, FillRule::Winding, t, None);
                let stroke = Stroke {
                    width: 1.5,
                    dash: StrokeDash::new(vec![4.5, 3.0], 0.0),
                    ..Stroke::default()
                };
                pm.stroke_path(&p, &fill(argb(0xB3FFBA8C)), &stroke, t, None);
            }
            if let Some(f) = fonts {
                let big = [Span::new("Drop here", Weight::Semibold, 14.0, rgb(BRIGHT))];
                let small = [Span::new(
                    "A file to ask about it · a folder to open it in Kilno",
                    Weight::Regular,
                    12.5,
                    rgb(FAINT),
                )];
                let (a14, l14) = f.line(14.0);
                let (a125, l125) = f.line(12.5);
                let top = by as f32 + (bh as f32 - (l14 + 2.0 + l125)) / 2.0;
                let mid = (bx + bw / 2.0) as f32;
                f.draw(pm, &big, mid - f.measure(&big) / 2.0, top + a14, None, s);
                f.draw(pm, &small, mid - f.measure(&small) / 2.0, top + l14 + 2.0 + a125, Some(bw as f32), s);
            }
        }
        Body::Pick(rows) => {
            let (rx, rw) = (x0 + 10.0, (w - 20.0).max(0.0));
            for (i, row) in rows.iter().enumerate() {
                let ry = y0 + HEADER + ROW * i as f64;
                if scene.hover_row == Some(i) {
                    round_rect(pm, rx, ry, rw, ROW, [10.0; 4], &fill(argb(0x14FFFFFF)), t);
                }
                let bx = rx + 8.0;
                round_rect(pm, bx, ry + 7.0, 20.0, 20.0, [5.0; 4], &fill(argb(0x1FFFFFFF)), t);
                let mut right = rx + rw - 10.0;
                if let Some(d) = row.dot {
                    dot(pm, right - 4.0, ry + ROW / 2.0, d, t);
                    right -= 8.0 + 10.0;
                }
                if let Some(f) = fonts {
                    let key = [Span::new(&row.key, Weight::Semibold, 11.5, rgb(BRIGHT))];
                    let (ak, lk) = f.line(11.5);
                    let kx = bx as f32 + (20.0 - f.measure(&key)) / 2.0;
                    let ky = (ry + 7.0) as f32 + (20.0 - lk) / 2.0 + ak;
                    f.draw(pm, &key, kx, ky, None, s);
                    let folder = if row.folder.is_empty() { String::new() } else { format!("  {}", row.folder) };
                    let line = [
                        Span::new(&row.name, Weight::Semibold, 13.0, rgb(BRIGHT)),
                        Span::new(&folder, Weight::Regular, 13.0, rgb(FAINT)),
                    ];
                    let (a, l) = f.line(13.0);
                    let tx = bx + 20.0 + 10.0;
                    let ty = ry as f32 + (ROW as f32 - l) / 2.0 + a;
                    f.draw(pm, &line, tx as f32, ty, Some((right - tx).max(0.0) as f32), s);
                }
            }
        }
    }

    if let Some(menu) = scene.menu {
        paint_menu(pm, menu, s, fonts);
    }
}

fn paint_menu(pm: &mut PixmapMut, m: &Menu, s: f32, fonts: Option<&Fonts>) {
    let t = Transform::from_scale(s, s);
    let h = Menu::height();
    round_rect(pm, m.x, m.y, MENU_WIDTH, h, [8.0; 4], &fill(rgb(0x1F2328)), t);
    let mut pb = PathBuilder::new();
    rounded(&mut pb, m.x as f32 + 0.5, m.y as f32 + 0.5, MENU_WIDTH as f32 - 1.0, h as f32 - 1.0, [8.0; 4]);
    if let Some(p) = pb.finish() {
        pm.stroke_path(&p, &fill(argb(0x26FFFFFF)), &Stroke::default(), t, None);
    }
    let sep_y = m.y + MENU_PAD + MENU_ITEM + MENU_SEP / 2.0;
    if let Some(r) = Rect::from_xywh((m.x + 8.0) as f32, sep_y as f32, (MENU_WIDTH - 16.0) as f32, 1.0) {
        pm.fill_rect(r, &fill(argb(0x1FFFFFFF)), t, None);
    }
    for item in [MenuItem::Login, MenuItem::Quit] {
        let top = m.y + Menu::item_top(item);
        let enabled = item != MenuItem::Login || m.login_enabled;
        if m.hover == Some(item) && enabled {
            round_rect(pm, m.x + MENU_PAD, top, MENU_WIDTH - 2.0 * MENU_PAD, MENU_ITEM, [5.0; 4], &fill(argb(0x14FFFFFF)), t);
        }
        let Some(f) = fonts else { continue };
        let color = if enabled { rgb(BRIGHT) } else { rgb(FAINT) };
        let (a, l) = f.line(13.0);
        let y = top as f32 + (MENU_ITEM as f32 - l) / 2.0 + a;
        if item == MenuItem::Login && m.login_on {
            f.draw(pm, &[Span::new("✓", Weight::Regular, 13.0, color)], (m.x + 12.0) as f32, y, None, s);
        }
        let label = if item == MenuItem::Login { "Start at login" } else { "Quit Kilno" };
        f.draw(pm, &[Span::new(label, Weight::Regular, 13.0, color)], (m.x + 34.0) as f32, y, None, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Pixmap;

    #[test]
    fn widths_and_heights() {
        assert_eq!(pill_width(IslandState::Compact, false), 92.0);
        assert_eq!(pill_width(IslandState::Peek, false), 300.0);
        assert_eq!(pill_width(IslandState::Notify, true), 420.0);
        assert_eq!(pill_width(IslandState::Drop, false), 420.0);
        assert_eq!(pill_width(IslandState::Pick, false), 420.0);
        assert_eq!(window_height(IslandState::Peek, 3), 60.0);
        assert_eq!(window_height(IslandState::Drop, 0), 190.0);
        assert_eq!(window_height(IslandState::Pick, 2), 52.0 + 34.0 * 3.0 + 22.0);
        assert_eq!(pill_height(IslandState::Drop, 0), 176.0);
        assert_eq!(slide_out_to(52.0), -60.0);
        assert_eq!(slide_out_to(176.0), -184.0);
        // Compact: 8 + 44 + 10 + 8 dot + 22 = 92.
        assert_eq!(8.0 + 44.0 + 10.0 + 8.0 + 22.0, COMPACT_WIDTH);
    }

    #[test]
    fn placement_in_physical_px() {
        assert_eq!(centred_px(0, 1920, 0, 420.0, 60.0, 1.0), (750, 0, 420, 60));
        assert_eq!(centred_px(1920, 4480, -200, 420.0, 60.0, 1.5), (2885, -200, 630, 90));
    }

    #[test]
    fn words() {
        assert_eq!(sessions_line(1), "1 session");
        assert_eq!(sessions_line(0), "0 sessions");
        assert_eq!(pick_hint(0), "N for a new session · Esc to cancel");
        assert_eq!(pick_hint(3), "1–3 or N · Esc to cancel");
        assert_eq!(note_rest(NoteKind::NeedsYou), " · needs you");
        assert_eq!(note_rest(NoteKind::Tip), "");
        assert_eq!(status_color(PaneStatus::Done), (0x6B, 0xBF, 0x8A));
        assert_eq!(compact_dot(true), WARM);
    }

    #[test]
    fn hit_tests() {
        assert!(in_pill(300.0, 52.0, 60.0, 10.0));
        assert!(!in_pill(300.0, 52.0, 59.0, 10.0));
        assert!(!in_pill(300.0, 52.0, 200.0, 52.0));
        assert_eq!(pick_row_at(420.0, 3, 20.0, 52.0), Some(0));
        assert_eq!(pick_row_at(420.0, 3, 20.0, 52.0 + 34.0 * 2.5), Some(2));
        assert_eq!(pick_row_at(420.0, 3, 20.0, 52.0 + 34.0 * 3.0), None);
        assert_eq!(pick_row_at(420.0, 3, 5.0, 60.0), None);
        let m = Menu::at(400.0, 40.0, true, false);
        assert_eq!(m.x, 220.0);
        assert_eq!(m.hit(230.0, 50.0), None, "disabled login");
        assert_eq!(m.hit(230.0, 40.0 + 4.0 + 30.0 + 9.0 + 5.0), Some(MenuItem::Quit));
        let m = Menu::at(10.0, 40.0, false, true);
        assert_eq!(m.hit(20.0, 50.0), Some(MenuItem::Login));
        assert_eq!(m.bottom(), 40.0 + 77.0);
    }

    #[test]
    fn paints_a_black_pill_with_round_bottom_corners() {
        let mut pm = Pixmap::new(420, 60).expect("pixmap");
        let content = Content::default();
        let scene = Scene {
            width: 300.0,
            height: 52.0,
            slide: 0.0,
            rig: Rig::REST,
            content: &content,
            hover_row: None,
            drop_hover: false,
            menu: None,
        };
        paint(&mut pm.as_mut(), 1.0, &scene, None);
        let at = |x, y| pm.pixel(x, y).expect("px");
        assert_eq!(at(61, 0).alpha(), 255, "square top corner");
        assert_eq!(at(60, 51).alpha(), 0, "round bottom corner");
        assert_eq!(at(200, 40).alpha(), 255);
        assert_eq!(at(30, 20).alpha(), 0, "outside the pill");
        assert_eq!(at(200, 55).alpha(), 0, "below the pill");
        // Mascot body at 8 px in.
        let body = at(60 + 8 + 22, 4 + 12);
        assert!(body.red() > 150);
    }
}
