//! The island window (IslandWindow.xaml.cs): polls the cursor, feeds the trigger, slides the
//! pill in and out, paints it and the glow, and handles the screenshot keys, the clipboard
//! offer, file drops and the menu. Everything runs on the island's own thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use kiln_core::island::mention::mention;
use kiln_core::island::trigger::{
    IslandInput, IslandNote, IslandState, IslandTrigger, NoteKind, NoteSource, PxRect,
};
use tiny_skia::Pixmap;

use crate::anim::{Ease, Tween};
use crate::glow::{self, GlowAnim, Rgb};
use crate::mascot::{self, MascotMove, Rig};
use crate::sys::win::{self, Handler, Surface, Window};
use crate::text::Fonts;
use crate::view::{self, Body, Content, Menu, MenuItem, PickRow, Scene};
use crate::{Cmd, IslandHost, PickChoice, WM_WAKE};

const POLL: usize = 1;
const FRAME: usize = 2;
const POLL_NEAR: u32 = 50;
const POLL_FAR: u32 = 250;

const WM_SETCURSOR: u32 = 0x0020;
const WM_MOUSEACTIVATE: u32 = 0x0021;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_HOTKEY: u32 = 0x0312;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;
const MA_NOACTIVATE: isize = 3;

const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_NOREPEAT: u32 = 0x4000;
const SHOT_KEY: i32 = 1;
const PICK_KEYS: i32 = 10;
/// 1 2 3 N Esc.
const PICK_VKS: [u32; 5] = [0x31, 0x32, 0x33, 0x4E, 0x1B];
const VK_ESCAPE: i32 = 0x1B;

const POSE_STEP_MS: i64 = 1400;
const REST_MS: i64 = 30_000;

pub(crate) fn run(host: Arc<dyn IslandHost>, rx: Receiver<Cmd>, ready: Sender<isize>) {
    let ole = win::init_thread();
    let (Some(hwnd), Some(glow)) = (win::create_window("Kiln island", false), win::create_window("Kiln glow", true))
    else {
        let _ = ready.send(0);
        return;
    };
    let dropping = ole && win::register_drop(hwnd);
    let island = Island::new(host, rx, hwnd, glow, dropping);
    win::set_handler(Some(Box::new(island)));
    win::set_timer(hwnd, POLL, POLL_NEAR);
    let _ = ready.send(hwnd.raw());
    win::run_loop();
    win::set_handler(None);
    if ole {
        win::uninit_thread();
    }
}

struct Island {
    host: Arc<dyn IslandHost>,
    rx: Receiver<Cmd>,
    hwnd: Window,
    glow_hwnd: Window,
    dropping: bool,
    closed: bool,
    clock: Instant,
    trigger: IslandTrigger,
    fonts: Option<Option<Fonts>>,

    poll_ms: u32,
    frame_ms: Option<u32>,
    island_was_animating: bool,
    glow_was_animating: bool,
    busy: bool,
    busy_checked_at: i64,
    esc_was_down: bool,
    button_was_down: bool,
    pick_asked: bool,
    client_anim: bool,

    /// What is on screen (note queue number), so a new note while showing is a change.
    shown: Option<u64>,
    monitor: PxRect,
    scale: f64,
    placed: PxRect,
    window_x: i32,
    window_h: f64,
    pill_h: f64,
    visible: bool,
    content: Content,
    slide: Tween,
    width: Tween,
    out_end: Option<i64>,
    surface: Option<Surface>,

    mv: MascotMove,
    mascot_t0: i64,
    still: bool,
    glasses: Tween,
    pose_after: Option<(MascotMove, i64)>,
    rest_at: Option<i64>,

    glow: GlowAnim,
    glow_monitor: PxRect,
    glow_scale: f64,
    glow_shown: bool,
    glow_surface: Option<Surface>,

    shot_key: bool,
    pick_keys: bool,
    offering: bool,
    clip_seq: u32,
    choices: Vec<PickChoice>,

    drop_hover: bool,
    hover_row: Option<usize>,
    menu: Option<Menu>,
}

impl Island {
    fn new(host: Arc<dyn IslandHost>, rx: Receiver<Cmd>, hwnd: Window, glow_hwnd: Window, dropping: bool) -> Self {
        Island {
            host,
            rx,
            hwnd,
            glow_hwnd,
            dropping,
            closed: false,
            clock: Instant::now(),
            trigger: IslandTrigger::new(),
            fonts: None,
            poll_ms: POLL_NEAR,
            frame_ms: None,
            island_was_animating: false,
            glow_was_animating: false,
            busy: false,
            busy_checked_at: -1000,
            esc_was_down: false,
            button_was_down: false,
            pick_asked: false,
            client_anim: win::client_area_animation(),
            shown: None,
            monitor: PxRect::default(),
            scale: 1.0,
            placed: PxRect::default(),
            window_x: 0,
            window_h: view::ISLAND_HEIGHT,
            pill_h: view::HEADER,
            visible: false,
            content: Content::default(),
            slide: Tween::hold(-view::ISLAND_HEIGHT),
            width: Tween::hold(view::PEEK_WIDTH),
            out_end: None,
            surface: None,
            mv: MascotMove::Idle,
            mascot_t0: 0,
            still: false,
            glasses: Tween::hold(0.0),
            pose_after: None,
            rest_at: None,
            glow: GlowAnim::default(),
            glow_monitor: PxRect::default(),
            glow_scale: 1.0,
            glow_shown: false,
            glow_surface: None,
            shot_key: false,
            pick_keys: false,
            offering: false,
            clip_seq: 0,
            choices: Vec::new(),
            drop_hover: false,
            hover_row: None,
            menu: None,
        }
    }

    fn now(&self) -> i64 {
        self.clock.elapsed().as_millis() as i64
    }

    // ------------------------------------------------------------------ commands

    fn drain(&mut self) {
        while let Ok(cmd) = self.rx.try_recv() {
            match cmd {
                // Only queued; the next poll decides whether it may show.
                Cmd::Notify(n) => self.trigger.notify(n),
                Cmd::Resolve(s) => self.trigger.resolve(&s),
                Cmd::Working(s, w) => self.trigger.set_working(s, w),
                Cmd::Greet => self.trigger.greet(),
                Cmd::Settings { hotkey, offer, reply } => {
                    let taken = self.apply_shots(hotkey, offer);
                    let _ = reply.send(taken);
                }
                Cmd::Shutdown => {
                    self.shutdown();
                    return;
                }
            }
        }
    }

    fn shutdown(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        win::kill_timer(self.hwnd, POLL);
        win::kill_timer(self.hwnd, FRAME);
        self.release_pick_keys();
        self.apply_shots(false, false);
        if self.dropping {
            win::revoke_drop(self.hwnd);
        }
        self.surface = None;
        self.glow_surface = None;
        win::destroy(self.glow_hwnd);
        win::destroy(self.hwnd);
        win::quit_loop();
    }

    // ---------------------------------------------------------------------- poll

    fn tick(&mut self) {
        if self.closed {
            return;
        }
        // Fails on the secure desktop (lock screen, UAC prompt). Nothing to decide there.
        let Some((cx, cy)) = win::cursor_pos() else { return };
        let Some((bounds, scale)) = win::monitor_at(cx, cy) else { return };
        let now = self.now();

        // The only call that asks the shell; once a second is plenty.
        if now - self.busy_checked_at >= 1000 {
            self.busy = win::is_busy();
            self.busy_checked_at = now;
        }

        // Edge-detected so an Esc held from before does not count. Never swallowed.
        let esc_down = win::key_down(VK_ESCAPE);
        let esc_pressed = esc_down && !self.esc_was_down;
        self.esc_was_down = esc_down;

        // A press outside the open menu closes it, as a popup would.
        let button = win::any_button_down();
        if button && !self.button_was_down && self.menu.is_some() {
            let (lx, ly) = self.to_logical(cx, cy);
            if !self.menu.as_ref().is_some_and(|m| m.contains(lx, ly)) {
                self.close_menu();
            }
        }
        self.button_was_down = button;
        if esc_pressed && self.menu.is_some() {
            self.close_menu();
        }

        let was = self.trigger.state();
        if was != IslandState::Hidden && esc_pressed {
            self.trigger.dismiss();
        }
        // Here, after `was`, so the picker is seen as a change and slides in.
        if self.pick_asked {
            self.pick_asked = false;
            self.trigger.pick(now);
        }

        // The pill grows downwards with the body, so its height is read again every poll.
        self.placed.bottom = self.placed.top + (view::ISLAND_HEIGHT.max(self.pill_h) * self.scale).round() as i32;

        let input = IslandInput {
            cursor_x: cx,
            cursor_y: cy,
            monitor: bounds,
            scale,
            button_down: win::primary_button_down(),
            moving_or_sizing: win::in_move_size(),
            busy: self.busy,
            // The open menu hangs below the pill; reaching for it must not send it away.
            over_island: self.visible && (self.placed.contains(cx, cy) || self.menu.is_some()),
            now_ms: now,
            kiln_active: self.host.kiln_active(),
            dragging: win::in_ole_drag(),
        };
        let state = self.trigger.update(&input);

        if was == IslandState::Pick && state != IslandState::Pick {
            self.release_pick_keys();
        }

        // A new note while one shows is a change too: same state, different content.
        if state != was || self.trigger.note_seq() != self.shown {
            if state == IslandState::Hidden {
                self.slide_out(now);
            } else if was == IslandState::Hidden {
                self.slide_in(bounds, scale, true, now);
            } else {
                // A note replacing another stays on the island's monitor.
                self.slide_in(self.monitor, self.scale, false, now);
            }
        }

        // The compact pill lets clicks through to the tabs under it; so does a leaving pill.
        win::set_click_through(self.hwnd, matches!(state, IslandState::Compact | IslandState::Hidden));

        // After slide_in, so the glow window is already lit when the bar stops needing it.
        let was_dwelling = self.glow.dwelling();
        if self.glow.dwell(now, self.trigger.dwell_progress(), self.trigger.dwell_length_ms()) {
            self.glow_monitor = bounds;
            self.glow_scale = scale;
            self.put_glow(None);
        } else if was_dwelling != self.glow.dwelling() {
            self.redraw_glow();
        }
        if !self.glow.visible() {
            self.hide_glow();
        }

        self.deadlines(now);
        self.update_hover(cx, cy);
        self.schedule(now);

        let interval =
            if state == IslandState::Hidden && f64::from(cy - bounds.top) > 200.0 * scale { POLL_FAR } else { POLL_NEAR };
        if interval != self.poll_ms {
            self.poll_ms = interval;
            win::set_timer(self.hwnd, POLL, interval);
        }
    }

    fn deadlines(&mut self, now: i64) {
        if let Some((next, at)) = self.pose_after {
            if now >= at {
                self.pose_after = None;
                self.play(next, now);
            }
        }
        if self.rest_at.is_some_and(|at| now >= at) {
            // Hours-long notes hold still: no redraw at all after 30 s.
            self.rest_at = None;
            self.still = true;
            self.apply_mascot(now);
            self.glow.rest();
            self.redraw();
            self.redraw_glow();
        }
        if self.out_end.is_some_and(|at| now >= at) {
            self.out_end = None;
            // A peek can start again while sliding out; then the window stays.
            if self.trigger.state() == IslandState::Hidden {
                self.visible = false;
                win::hide(self.hwnd);
                self.apply_mascot(now);
            }
        }
        if self.glow.settle(now) {
            self.redraw_glow();
            self.hide_glow();
        }
    }

    fn to_logical(&self, x: i32, y: i32) -> (f64, f64) {
        (f64::from(x - self.window_x) / self.scale, f64::from(y - self.monitor.top) / self.scale)
    }

    // ----------------------------------------------------------------- show/hide

    fn slide_in(&mut self, monitor: PxRect, scale: f64, fresh: bool, now: i64) {
        let state = self.trigger.state();
        let note = self.trigger.note().cloned();
        let compact = state == IslandState::Compact;
        let drop = state == IslandState::Drop;
        let pick = state == IslandState::Pick;
        self.shown = self.trigger.note_seq();
        self.monitor = monitor;
        self.scale = scale;
        self.client_anim = win::client_area_animation();

        let pill_width = view::pill_width(state, note.is_some());
        let greeting = self.trigger.greeting();
        let mut c = Content {
            compact,
            dot: view::compact_dot(greeting),
            ..Content::default()
        };
        if pick {
            c.head_name = "send screenshot".into();
            c.head_rest = " to…".into();
            c.sub = Some(view::pick_hint(self.choices.len()));
            let mut rows: Vec<PickRow> = self
                .choices
                .iter()
                .enumerate()
                .map(|(i, ch)| PickRow {
                    key: (i + 1).to_string(),
                    name: ch.name.clone(),
                    folder: ch.folder.clone(),
                    dot: Some(view::status_color(ch.status)),
                })
                .collect();
            rows.push(PickRow {
                key: "N".into(),
                name: "New session…".into(),
                folder: self.choices.first().map(|c| c.folder.clone()).unwrap_or_default(),
                dot: None,
            });
            c.body = Body::Pick(rows);
        } else if drop {
            c.sub = Some("drop to start".into());
            c.body = Body::Drop;
        } else if let Some(n) = &note {
            c.head_name = n.name.clone();
            c.head_rest = view::note_rest(n.kind).into();
        } else {
            c.sub = Some(view::sessions_line(self.host.open_session_count()));
        }
        self.content = c;
        self.hover_row = None;
        self.drop_hover = false;

        self.present(pill_width, fresh, now);

        self.pose_after = None;
        self.still = false;
        self.rest_at = Some(now + REST_MS);
        match note.as_ref().map(|n| n.kind) {
            None if drop || pick => self.pose(MascotMove::Peek, None, now),
            // The login hello waves until it leaves.
            None if greeting => self.pose(MascotMove::Wave, None, now),
            None if compact => self.pose(MascotMove::Walk, None, now),
            None => self.pose(MascotMove::Wave, Some(MascotMove::Idle), now),
            Some(NoteKind::NeedsYou) => self.pose(MascotMove::Alert, Some(MascotMove::Talk), now),
            Some(NoteKind::Error | NoteKind::Reserve) => self.pose(MascotMove::Alert, None, now),
            Some(NoteKind::Done) => self.pose(MascotMove::Jump, None, now),
            Some(NoteKind::Tip) => self.pose(MascotMove::Wave, Some(MascotMove::Idle), now),
        }

        let light: Rgb = match note.as_ref().map(|n| n.kind) {
            None if compact && !greeting => glow::BLUE,
            None => glow::WARM,
            Some(NoteKind::NeedsYou) => glow::BLUE,
            Some(NoteKind::Done) => glow::GREEN,
            Some(NoteKind::Tip) => glow::WARM,
            Some(_) => glow::AMBER,
        };
        self.glow_monitor = self.monitor;
        self.glow_scale = self.scale;
        self.glow.light(now, light, (pill_width / view::NOTE_WIDTH).min(1.0), self.client_anim);
        self.put_glow(Some(self.hwnd));
        self.redraw();
        self.schedule(now);
    }

    /// Puts the window at the top of the monitor and brings the pill out at this width, or
    /// grows it there when it is already out.
    fn present(&mut self, pill_width: f64, fresh: bool, now: i64) {
        let state = self.trigger.state();
        self.window_h = view::window_height(state, self.choices.len());
        self.pill_h = view::pill_height(state, self.choices.len());
        let (x, _, _, _) = view::centred_px(self.monitor.left, self.monitor.right, self.monitor.top, view::NOTE_WIDTH, 1.0, self.scale);
        self.window_x = x;
        // Only the pill keeps a peek open, not the transparent rest of the window.
        let (px, py, pw, ph) =
            view::centred_px(self.monitor.left, self.monitor.right, self.monitor.top, pill_width, view::ISLAND_HEIGHT, self.scale);
        self.placed = PxRect::new(px, py, px + pw, py + ph);

        let spring = Ease::BackOut(0.3);
        if fresh {
            self.width.set(pill_width);
        } else {
            self.width.retarget(now, pill_width, 400, spring);
        }
        self.slide.retarget(now, 0.0, 450, spring);
        self.out_end = None;
        self.visible = true;
        self.redraw();
        win::show_below(self.hwnd, None);
    }

    fn slide_out(&mut self, now: i64) {
        self.shown = None;
        self.pose_after = None;
        self.rest_at = None;
        self.menu = None;
        self.glow.dim(now, view::ISLAND_HEIGHT);
        self.slide.retarget(now, view::slide_out_to(self.pill_h), 260, Ease::CubicInOut);
        self.out_end = Some(now + 260);
        self.schedule(now);
    }

    fn pose(&mut self, mv: MascotMove, then: Option<MascotMove>, now: i64) {
        self.play(mv, now);
        if let Some(next) = then {
            self.pose_after = Some((next, now + POSE_STEP_MS));
        }
    }

    fn play(&mut self, mv: MascotMove, now: i64) {
        self.mv = mv;
        self.apply_mascot(now);
    }

    fn mascot_animates(&self) -> bool {
        self.visible && self.client_anim && !self.still
    }

    /// KilnMascot.Apply: restarts the loops and moves the glasses to where this move wants
    /// them, sliding when animating.
    fn apply_mascot(&mut self, now: i64) {
        let target = mascot::glasses_for(self.mv);
        self.mascot_t0 = now;
        if self.mascot_animates() {
            self.glasses.retarget(now, target, mascot::GLASSES_MS, Ease::BackOut(mascot::GLASSES_AMPLITUDE));
        } else {
            self.glasses.set(target);
        }
    }

    fn rig(&self, now: i64) -> Rig {
        if self.mascot_animates() {
            Rig::at(self.mv, (now - self.mascot_t0) as f64 / 1000.0, self.glasses.value(now))
        } else {
            Rig {
                glasses: self.glasses.value(now),
                ..Rig::REST
            }
        }
    }

    // -------------------------------------------------------------------- frames

    fn island_animating(&self, now: i64) -> bool {
        self.visible
            && (self.slide.running(now)
                || self.width.running(now)
                || self.out_end.is_some()
                || self.glasses.running(now)
                || (self.mascot_animates() && mascot::moving(self.mv, (now - self.mascot_t0) as f64 / 1000.0)))
    }

    fn tweening(&self, now: i64) -> bool {
        (self.visible && (self.slide.running(now) || self.width.running(now) || self.glasses.running(now)))
            || (self.glow.visible() && !self.glow.breathing_only(now))
    }

    /// Keeps the frame timer running only while something moves: ~60 fps during a slide or
    /// a spring, ~30 fps for the loops and the breath, none at rest.
    fn schedule(&mut self, now: i64) {
        let want = if self.closed {
            None
        } else if self.tweening(now) {
            Some(16)
        } else if self.island_animating(now) || self.glow.animating(now) {
            Some(33)
        } else {
            None
        };
        if want != self.frame_ms {
            match want {
                Some(ms) => win::set_timer(self.hwnd, FRAME, ms),
                None => win::kill_timer(self.hwnd, FRAME),
            }
            self.frame_ms = want;
        }
    }

    fn frame(&mut self) {
        if self.closed {
            return;
        }
        let now = self.now();
        let (island, glow) = (self.island_animating(now), self.glow.animating(now));
        self.deadlines(now);
        // One more frame after a move ends, so its last value is what stays on screen.
        if island || self.island_was_animating {
            self.redraw();
        }
        if glow || self.glow_was_animating {
            self.redraw_glow();
        }
        self.island_was_animating = island;
        self.glow_was_animating = glow;
        self.schedule(now);
    }

    fn redraw(&mut self) {
        if !self.visible || self.closed {
            return;
        }
        let now = self.now();
        let scale = self.scale;
        let mut h = self.window_h;
        if let Some(m) = &self.menu {
            h = h.max(m.bottom() + 4.0);
        }
        let (pw, ph) = ((view::NOTE_WIDTH * scale).round() as u32, (h * scale).round() as u32);
        let Some(mut pm) = Pixmap::new(pw.max(1), ph.max(1)) else { return };
        let rig = self.rig(now);
        let menu = self.menu;
        let content = self.content.clone();
        let scene = Scene {
            width: self.width.value(now),
            height: self.pill_h,
            slide: self.slide.value(now),
            rig,
            content: &content,
            hover_row: self.hover_row,
            drop_hover: self.drop_hover,
            menu: menu.as_ref(),
        };
        let fonts = self.fonts.get_or_insert_with(Fonts::system).as_ref();
        view::paint(&mut pm.as_mut(), scale, &scene, fonts);
        win::present(self.hwnd, &mut self.surface, self.window_x, self.monitor.top, &pm);
    }

    fn put_glow(&mut self, below: Option<Window>) {
        self.redraw_glow_forced();
        win::show_below(self.glow_hwnd, below);
        self.glow_shown = true;
    }

    fn hide_glow(&mut self) {
        if self.glow_shown && !self.glow.visible() {
            win::hide(self.glow_hwnd);
            self.glow_shown = false;
            self.glow_surface = None;
        }
    }

    fn redraw_glow(&mut self) {
        if self.glow_shown {
            self.redraw_glow_forced();
        }
    }

    fn redraw_glow_forced(&mut self) {
        let now = self.now();
        let s = self.glow_scale;
        let (x, y, w, h) =
            view::centred_px(self.glow_monitor.left, self.glow_monitor.right, self.glow_monitor.top, glow::GLOW_WIDTH, glow::GLOW_HEIGHT, s);
        let Some(mut pm) = Pixmap::new(w.max(1) as u32, h.max(1) as u32) else { return };
        glow::paint(&mut pm.as_mut(), s, &self.glow.frame(now));
        win::present(self.glow_hwnd, &mut self.glow_surface, x, y, &pm);
    }

    // ------------------------------------------------------------------- input

    fn update_hover(&mut self, cx: i32, cy: i32) {
        if !self.visible {
            return;
        }
        let (x, y) = self.to_logical(cx, cy);
        let mut changed = false;
        let row = match (&self.content.body, self.trigger.state()) {
            (Body::Pick(rows), IslandState::Pick) if self.menu.is_none() => {
                view::pick_row_at(self.width.value(self.now()), rows.len(), x, y)
            }
            _ => None,
        };
        if row != self.hover_row {
            self.hover_row = row;
            changed = true;
        }
        if let Some(m) = self.menu.as_mut() {
            let hit = m.hit(x, y);
            if hit != m.hover {
                m.hover = hit;
                changed = true;
            }
        }
        if changed {
            self.redraw();
        }
    }

    fn close_menu(&mut self) {
        if self.menu.take().is_some() {
            self.redraw();
        }
    }

    fn client_to_logical(&self, lp: isize) -> (f64, f64) {
        let x = (lp & 0xFFFF) as u16 as i16;
        let y = ((lp >> 16) & 0xFFFF) as u16 as i16;
        (f64::from(x) / self.scale, f64::from(y) / self.scale)
    }

    fn left_click(&mut self, x: f64, y: f64) {
        if let Some(m) = self.menu {
            self.menu = None;
            self.redraw();
            match m.hit(x, y) {
                Some(MenuItem::Login) => self.host.set_login(!m.login_on),
                Some(MenuItem::Quit) => self.host.quit(),
                None => {}
            }
            return;
        }
        let state = self.trigger.state();
        let now = self.now();
        match state {
            IslandState::Pick => {
                let rows = self.choices.len() + 1;
                if let Some(i) = view::pick_row_at(self.width.value(now), rows, x, y) {
                    self.choose(if i < self.choices.len() { Some(i) } else { None });
                }
            }
            // A click on the drop target is meant for it, not a way into Kiln.
            IslandState::Drop | IslandState::Hidden | IslandState::Compact => {}
            IslandState::Peek | IslandState::Notify => {
                if !view::in_pill(self.width.value(now), self.pill_h, x, y) {
                    return;
                }
                let note: Option<IslandNote> = self.trigger.note().cloned();
                self.trigger.dismiss();
                self.slide_out(now);
                // The click was this process's last input, so the main window may come to front.
                self.host.reveal();
                if let Some(NoteSource::Pane(p)) = note.map(|n| n.source) {
                    self.host.show_session(p);
                }
            }
        }
    }

    fn right_click(&mut self, x: f64, y: f64) {
        if matches!(self.trigger.state(), IslandState::Hidden | IslandState::Compact) {
            return;
        }
        let (on, can) = self.host.login_state();
        self.menu = Some(Menu::at(x, y, on, can));
        self.redraw();
    }

    // -------------------------------------------------------------- screenshots

    /// Ctrl+Alt+S on or off, and the clipboard listener for the automatic offer. Returns
    /// whether the hotkey is wanted but taken.
    fn apply_shots(&mut self, hotkey: bool, offer: bool) -> bool {
        if self.closed && (hotkey || offer) {
            return false;
        }
        if hotkey && !self.shot_key {
            self.shot_key = win::register_hotkey(self.hwnd, SHOT_KEY, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 0x53);
            // Another app has it (or a second Kiln): carry on without, and say so once.
            if !self.shot_key {
                self.trigger.notify(IslandNote::new(
                    NoteSource::Kiln("ShotKey"),
                    "Ctrl+Alt+S is taken by another app",
                    NoteKind::Tip,
                ));
            }
        } else if !hotkey && self.shot_key {
            win::unregister_hotkey(self.hwnd, SHOT_KEY);
            self.shot_key = false;
        }
        if offer && !self.offering {
            self.clip_seq = win::clipboard_seq();
            self.offering = win::add_clipboard_listener(self.hwnd);
        } else if !offer && self.offering {
            win::remove_clipboard_listener(self.hwnd);
            self.offering = false;
        }
        hotkey && !self.shot_key
    }

    fn on_hotkey(&mut self, id: i32) {
        if id == SHOT_KEY {
            if self.trigger.state() == IslandState::Pick {
                return;
            }
            if win::clipboard_has_image() {
                self.start_pick();
            } else {
                self.no_shot();
            }
            return;
        }
        let key = id - PICK_KEYS;
        if (0..=2).contains(&key) && (key as usize) < self.choices.len() {
            self.choose(Some(key as usize));
        } else if key == 3 {
            self.choose(None);
        } else if key == 4 {
            self.cancel_pick();
        }
    }

    fn no_shot(&mut self) {
        self.trigger.notify(IslandNote::new(
            NoteSource::Kiln("Screenshots"),
            "Copy a screenshot first (Win+Shift+S)",
            NoteKind::Tip,
        ));
    }

    /// The automatic offer: a new image on the clipboard brings the picker by itself, but not
    /// with Kiln in front, nor over a fullscreen app or Do Not Disturb.
    fn on_clipboard(&mut self) {
        let seq = win::clipboard_seq();
        if seq == self.clip_seq {
            return;
        }
        self.clip_seq = seq;
        if !self.offering
            || self.trigger.state() == IslandState::Pick
            || self.host.kiln_active()
            || win::is_busy()
            || !win::clipboard_has_image()
        {
            return;
        }
        self.start_pick();
    }

    fn start_pick(&mut self) {
        self.choices = self.host.recent_sessions(3);
        self.choices.truncate(3);
        self.pick_asked = true;
        // A key another app holds is just not offered by keyboard; a click still works.
        for (i, vk) in PICK_VKS.iter().enumerate() {
            win::register_hotkey(self.hwnd, PICK_KEYS + i as i32, MOD_NOREPEAT, *vk);
        }
        self.pick_keys = true;
        // Now, not on the next poll: the key press was this moment.
        self.tick();
    }

    fn release_pick_keys(&mut self) {
        if !self.pick_keys {
            return;
        }
        for i in 0..PICK_VKS.len() {
            win::unregister_hotkey(self.hwnd, PICK_KEYS + i as i32);
        }
        self.pick_keys = false;
    }

    fn cancel_pick(&mut self) {
        self.trigger.end_pick();
        self.release_pick_keys();
        let now = self.now();
        self.slide_out(now);
    }

    /// An answer: a session by its place in the list, or None for New session. The image is
    /// only written now, so a cancelled offer leaves nothing on disk.
    fn choose(&mut self, index: Option<usize>) {
        if self.trigger.state() != IslandState::Pick {
            return;
        }
        let pane = index.and_then(|i| self.choices.get(i)).map(|c| c.pane);
        self.cancel_pick();
        let dir = self.host.shots_dir();
        match crate::shots::save_from_clipboard(self.hwnd, &dir) {
            Some(path) => self.host.send_image(pane, path),
            None => self.no_shot(),
        }
    }

    // ---------------------------------------------------------------------- drop

    fn drop_taken(&self, path: Option<&Path>) -> bool {
        self.trigger.state() == IslandState::Drop && path.is_some()
    }
}

impl Handler for Island {
    fn message(&mut self, w: Window, msg: u32, wp: usize, lp: isize) -> Option<isize> {
        if w != self.hwnd {
            return None;
        }
        match msg {
            WM_WAKE => {
                self.drain();
                Some(0)
            }
            WM_TIMER => {
                match wp {
                    POLL => self.tick(),
                    FRAME => self.frame(),
                    _ => {}
                }
                Some(0)
            }
            WM_HOTKEY => {
                self.on_hotkey(wp as i32);
                Some(0)
            }
            WM_CLIPBOARDUPDATE => {
                self.on_clipboard();
                Some(0)
            }
            WM_MOUSEACTIVATE => Some(MA_NOACTIVATE),
            WM_SETCURSOR => {
                let state = self.trigger.state();
                let on_menu = self.menu.is_some_and(|m| {
                    win::cursor_pos().is_some_and(|(x, y)| {
                        let (lx, ly) = self.to_logical(x, y);
                        m.contains(lx, ly)
                    })
                });
                let hand = !on_menu
                    && (matches!(state, IslandState::Peek | IslandState::Notify)
                        || (state == IslandState::Pick && self.hover_row.is_some()));
                win::set_cursor(hand);
                Some(1)
            }
            WM_MOUSEMOVE => {
                if let Some((x, y)) = win::cursor_pos() {
                    self.update_hover(x, y);
                }
                Some(0)
            }
            WM_LBUTTONUP => {
                let (x, y) = self.client_to_logical(lp);
                self.left_click(x, y);
                let now = self.now();
                self.schedule(now);
                Some(0)
            }
            WM_RBUTTONUP => {
                let (x, y) = self.client_to_logical(lp);
                self.right_click(x, y);
                Some(0)
            }
            // Placed by hand in physical px on every present; Windows' suggested rect would
            // move it off the monitor's top centre.
            WM_DPICHANGED => Some(0),
            _ => None,
        }
    }

    fn drag_over(&mut self, path: Option<&Path>) -> bool {
        let take = self.drop_taken(path);
        if take != self.drop_hover {
            self.drop_hover = take;
            self.redraw();
        }
        take
    }

    fn drag_leave(&mut self) {
        if self.drop_hover {
            self.drop_hover = false;
            self.redraw();
        }
    }

    /// A folder opens a session there; a file opens one in its folder with @file typed in.
    fn drop_path(&mut self, path: Option<PathBuf>) {
        self.drag_leave();
        let Some(path) = path.filter(|p| self.drop_taken(Some(p))) else { return };
        self.trigger.release();
        let now = self.now();
        self.slide_out(now);
        if path.is_dir() {
            self.host.adopt_folder(path, None);
        } else if path.is_file() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(dir) = path.parent() {
                self.host.adopt_folder(dir.to_path_buf(), Some(mention(&name)));
            }
        }
    }
}

impl Drop for Island {
    fn drop(&mut self) {
        self.shutdown();
    }
}
