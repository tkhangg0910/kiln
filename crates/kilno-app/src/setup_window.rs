//! The setup window (Setup/SetupWindow.xaml + AgentRow.xaml): finds Claude Code and Codex and
//! lets you choose which Kilno uses. A deferred egui viewport of its own, so it can show while
//! the main window is hidden (first run, a Reveal that needs setup) and its mascot animates
//! without repainting the main window.
//!
//! It saves nothing itself: Open Kilno hands the settings back to the app, which owns them.
//! Start at login is the exception (written at once, it lives in the Run key); PATH is written
//! on Open Kilno. What it says comes from `kilno_core::setup`.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use kilno_core::agent::detect::{self, AgentDetector};
use kilno_core::agent::{helper, AgentInfo, AgentKind};
use kilno_core::paths::OsEnv;
use kilno_core::settings::{KilnoSettings, NewSessionAgent};
use kilno_core::setup::{self, AgentRowView, SetupMove, SetupView};
use kilno_core::system::login_startup::LoginStartup;
use kilno_core::system::user_path::{self, UserPath};
use kilno_island::mascot::{self, MascotMove};
use egui::{
    Align2, Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, Ui, Vec2, ViewportBuilder,
    ViewportCommand, ViewportId,
};

use crate::theme::*;

pub const TITLE: &str = "Kilno setup";
pub const SIZE: Vec2 = Vec2::new(760.0, 630.0);

const HEAD: Color32 = MENU_HOVER_FG; // #E6E9EE
const CLAY_TEXT: Color32 = Color32::from_rgb(0xE3, 0x9A, 0x7E);
const TICK_HOVER: Color32 = Color32::from_rgb(0x1E, 0x23, 0x2B);
const DISABLED_FG: Color32 = Color32::from_rgb(0x6E, 0x76, 0x83);

pub fn viewport_id() -> ViewportId {
    ViewportId::from_hash_of("kilno-setup")
}

/// The window's builder: fixed size, centered on `over` (the main window when it shows), else
/// on the primary work area.
pub fn builder(over: Option<Rect>) -> ViewportBuilder {
    let mut b = ViewportBuilder::default()
        .with_title(TITLE)
        .with_inner_size(SIZE)
        .with_resizable(false)
        .with_maximize_button(false)
        .with_icon(Arc::new(egui::IconData::default()));
    let area = over.or_else(|| {
        #[cfg(windows)]
        {
            crate::win::primary_work_area().map(|(x, y, w, h)| Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h)))
        }
        #[cfg(not(windows))]
        None
    });
    if let Some(a) = area {
        // The caption (~32) sits above the client area.
        b = b.with_position(Pos2::new(a.center().x - SIZE.x / 2.0, a.center().y - (SIZE.y + 32.0) / 2.0).max(a.min));
    }
    b
}

enum Msg {
    Found(u64, Box<[AgentInfo; 3]>),
    HelperDone,
    Located { kind: AgentKind, file: String, ran: bool },
}

/// What came of the window, for the app to act on.
pub enum Outcome {
    /// Open Kilno: save these and show the main window.
    Done(Box<KilnoSettings>),
    /// Closed without Open: nothing changes; the next Reveal asks again.
    Closed,
}

enum Action {
    Install(AgentKind),
    SignIn(AgentKind),
    Locate(AgentKind),
    CheckAgain,
    Login(bool),
    Open,
}

pub struct SetupWindow {
    /// Locate… changes the paths here; they are saved with the rest on Open Kilno.
    settings: KilnoSettings,
    use_claude: bool,
    use_codex: bool,
    use_opencode: bool,
    pick: NewSessionAgent,
    login_on: bool,
    login_can: bool,
    path_on: bool,
    path_can: bool,
    found: Option<[AgentInfo; 3]>,
    view: SetupView,
    /// Locate…'s complaint, until the view next changes.
    note: Option<String>,
    /// Check again can be pressed before the last check is back; only the newest one counts.
    run: u64,
    /// Install or Sign in has its window open. One at a time.
    busy: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    mv: Option<SetupMove>,
    since: Instant,
    texture: Option<(TextureHandle, (SetupMove, u64, u32))>,
    #[cfg_attr(not(windows), allow(dead_code))] // the native window, for its dark caption on Windows
    hwnd: Option<isize>,
    focus: bool,
    pub outcome: Option<Outcome>,
}

pub type Shared = Arc<Mutex<SetupWindow>>;

pub fn lock(s: &Shared) -> MutexGuard<'_, SetupWindow> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

impl SetupWindow {
    pub fn new(settings: &KilnoSettings, data: &kilno_core::paths::DataDir, ctx: &egui::Context) -> Self {
        let startup = LoginStartup::for_this_user(data);
        #[cfg(windows)]
        let login_on = startup.is_on();
        #[cfg(not(windows))]
        let login_on = false;
        let path = UserPath::for_this_user();
        let folder = user_path::kilno_folder();
        let path_on = setup::path_tick(settings.setup_done, path.can_write(), || {
            #[cfg(windows)]
            {
                folder.as_ref().is_some_and(|f| path.contains(&f.to_string_lossy(), &OsEnv))
            }
            #[cfg(not(windows))]
            {
                let _ = &folder;
                false
            }
        });
        let (tx, rx) = channel();
        let mut w = Self {
            settings: settings.clone(),
            use_claude: settings.use_claude,
            use_codex: settings.use_codex,
            use_opencode: settings.use_opencode,
            pick: settings.default_agent,
            login_on,
            login_can: startup.can_write(),
            path_on,
            path_can: path.can_write(),
            found: None,
            view: setup::checking(),
            note: None,
            run: 0,
            busy: false,
            tx,
            rx,
            ctx: ctx.clone(),
            mv: None,
            since: Instant::now(),
            texture: None,
            hwnd: None,
            focus: true,
            outcome: None,
        };
        w.detect();
        w
    }

    /// Brings the window forward (it was asked for again while open).
    pub fn activate(&mut self) {
        self.focus = true;
        self.ctx.request_repaint_of(viewport_id());
    }

    fn detect(&mut self) {
        self.run += 1;
        self.found = None;
        self.view = setup::checking();
        self.note = None;
        let (run, tx, ctx) = (self.run, self.tx.clone(), self.ctx.clone());
        let (claude, codex, opencode) =
            (self.settings.claude_path.clone(), self.settings.codex_path.clone(), self.settings.opencode_path.clone());
        let _ = std::thread::Builder::new().name("kilno-detect".into()).spawn(move || {
            let found = AgentDetector::for_this_machine().detect_all([claude.as_deref(), codex.as_deref(), opencode.as_deref()]);
            let _ = tx.send(Msg::Found(run, Box::new(found)));
            ctx.request_repaint_of(viewport_id());
        });
    }

    fn show_found(&mut self) {
        if let Some([claude, codex, opencode]) = &self.found {
            self.view = setup::from_all(claude, codex, opencode, [self.use_claude, self.use_codex, self.use_opencode]);
            self.note = None;
        }
    }

    fn drain(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Found(run, found) if run == self.run => {
                    self.found = Some(*found);
                    self.show_found();
                }
                Msg::Found(..) => {}
                Msg::HelperDone => {
                    self.busy = false;
                    self.focus = true;
                    self.detect();
                }
                Msg::Located { kind, file, ran } => {
                    if !ran {
                        self.note = Some(setup::locate_failed(kind));
                        continue;
                    }
                    match kind {
                        AgentKind::Claude => self.settings.claude_path = Some(file),
                        AgentKind::Codex => self.settings.codex_path = Some(file),
                        AgentKind::OpenCode => self.settings.opencode_path = Some(file),
                    }
                    self.detect();
                }
            }
        }
    }

    /// One frame of the window (the deferred viewport's callback).
    pub fn frame(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) {
            self.outcome = Some(Outcome::Closed);
            ctx.request_repaint_of(ViewportId::ROOT);
            return;
        }
        self.drain();
        #[cfg(windows)]
        if self.hwnd.is_none() {
            if let Some(h) = crate::win::shell::find_own_window(TITLE) {
                ctx.send_viewport_cmd(ViewportCommand::SetTheme(egui::SystemTheme::Dark));
                crate::win::dark_caption(h);
                crate::win::shell::set_window_icon(h);
                self.hwnd = Some(h);
            }
        }
        if std::mem::take(&mut self.focus) {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        let mut actions = Vec::new();
        self.draw(ui, &mut actions);
        for a in actions {
            self.act(a, &ctx);
        }
    }

    fn act(&mut self, a: Action, ctx: &egui::Context) {
        match a {
            Action::CheckAgain => self.detect(),
            Action::Install(kind) | Action::SignIn(kind) => {
                let install = matches!(a, Action::Install(_));
                self.run_helper(kind, install);
            }
            Action::Locate(kind) => self.locate(kind),
            Action::Login(on) => {
                self.login_on = on;
                if let (Ok(exe), Some(data)) = (std::env::current_exe(), data_dir()) {
                    let _ = LoginStartup::for_this_user(&data).set(on, &exe);
                }
            }
            Action::Open => {
                self.open();
                ctx.request_repaint_of(ViewportId::ROOT);
            }
        }
    }

    /// Install or Sign in: the agent's own command in a PowerShell window, then a fresh check
    /// once that window is closed, however it went.
    fn run_helper(&mut self, kind: AgentKind, install: bool) {
        if self.busy {
            return;
        }
        let Some([claude, codex, opencode]) = &self.found else { return };
        let agent = match kind {
            AgentKind::Claude => claude,
            AgentKind::Codex => codex,
            AgentKind::OpenCode => opencode,
        };
        let command = if install { helper::install_command(agent) } else { helper::sign_in_command(agent) };
        let Some(command) = command else { return };
        self.busy = true;
        let found = setup::from_all(claude, codex, opencode, [self.use_claude, self.use_codex, self.use_opencode]);
        self.view = if install { setup::installing(&found, kind) } else { setup::signing_in(&found, kind) };
        self.note = None;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let _ = std::thread::Builder::new().name("kilno-helper".into()).spawn(move || {
            // No powershell.exe to start: the check after shows the row as it was.
            let _ = helper::run_in_window(&command);
            let _ = tx.send(Msg::HelperDone);
            ctx.request_repaint_of(viewport_id());
        });
    }

    /// Locate…: a program that answers --version becomes that agent's saved path.
    fn locate(&mut self, kind: AgentKind) {
        #[cfg_attr(not(windows), allow(unused_mut))] // owned by the window on Windows only
        let mut d = rfd::FileDialog::new()
            .set_title(setup::locate_title(kind))
            .add_filter("Programs (*.exe;*.cmd)", &["exe", "cmd"]);
        #[cfg(windows)]
        if let Some(h) = self.hwnd {
            d = d.set_parent(&crate::win::shell::Owner(h));
        }
        let Some(file) = d.pick_file() else { return };
        let file = file.to_string_lossy().into_owned();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let _ = std::thread::Builder::new().name("kilno-locate".into()).spawn(move || {
            let ran = detect::run(&file, "--version", detect::DEFAULT_TIMEOUT).is_some();
            let _ = tx.send(Msg::Located { kind, file, ran });
            ctx.request_repaint_of(viewport_id());
        });
    }

    /// Open Kilno: PATH per its tick, then the settings with SetupDone go back to the app.
    fn open(&mut self) {
        if !self.view.can_open {
            return;
        }
        let claude = self.found.as_ref().map(|[c, _, _]| c);
        let mut agent = setup::default_agent_on_open(self.view.show_picker, self.pick, claude, self.use_claude);
        // Only OpenCode there to start with: it is the one, not a Codex that is not installed.
        let ready = |k: AgentKind| self.found.as_ref().is_some_and(|f| f.iter().any(|a| a.kind == k && setup::state_of(a) == setup::AgentRowState::Ready));
        if !self.view.show_picker && agent == NewSessionAgent::Codex && !(ready(AgentKind::Codex) && self.use_codex) && ready(AgentKind::OpenCode) && self.use_opencode {
            agent = NewSessionAgent::OpenCode;
        }
        let path = UserPath::for_this_user();
        if path.can_write() {
            if let Some(folder) = user_path::kilno_folder() {
                #[cfg(windows)]
                if let Ok(true) = path.set(self.path_on, &folder.to_string_lossy(), &OsEnv) {
                    crate::win::shell::broadcast_environment();
                }
                #[cfg(not(windows))]
                let _ = folder;
            }
        }
        let s = KilnoSettings { use_opencode: self.use_opencode, ..setup::done_settings(&self.settings, self.use_claude, self.use_codex, agent) };
        self.outcome = Some(Outcome::Done(Box::new(s)));
    }

    // ------------------------------------------------------------------ drawing

    fn draw(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0, TERMINAL_BG);
        let inner = Rect::from_min_max(full.min + Vec2::new(36.0, 28.0), full.max - Vec2::new(36.0, 22.0));

        // The mascot and headline.
        let top = inner.top();
        let tx = inner.left() + 44.0 + 16.0;
        let tw = inner.right() - tx;
        let head = ui.painter().layout(self.view.title.clone(), semibold(21.0), HEAD, tw);
        let intro = ui.painter().layout(self.view.intro.clone(), ui_font(14.0), MUTED_AA, tw);
        let th = head.size().y + 2.0 + intro.size().y;
        let header_h = th.max(44.0);
        self.draw_mascot(ui, Rect::from_min_size(Pos2::new(inner.left(), top + (header_h - 44.0) / 2.0), Vec2::splat(44.0)));
        let ty = top + (header_h - th) / 2.0;
        let hh = head.size().y;
        ui.painter().galley(Pos2::new(tx, ty), head, HEAD);
        ui.painter().galley(Pos2::new(tx, ty + hh + 2.0), intro, MUTED_AA);

        // The agents.
        let card_top = top + header_h + 20.0;
        let found = self.found.clone();
        let info = |i: usize| found.as_ref().map(|f| &f[i]);
        let rows = [
            (self.view.claude.clone(), info(0)),
            (self.view.codex.clone(), info(1)),
            (self.view.opencode.clone(), info(2)),
        ];
        let heights: Vec<f32> = rows.iter().map(|(r, f)| row_height(r, *f)).collect();
        let card_h = heights.iter().sum::<f32>() + (rows.len() - 1) as f32;
        let card = Rect::from_min_size(Pos2::new(inner.left(), card_top), Vec2::new(inner.width(), card_h));
        ui.painter().rect_filled(card, 10, SURFACE_BG);
        ui.painter().rect_stroke(card, 10, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
        let mut y = card.top();
        for (i, (row, info)) in rows.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(card.left(), y), Vec2::new(card.width(), heights[i]));
            self.draw_row(ui, r, row, *info, actions);
            y += heights[i];
            if i + 1 < rows.len() {
                ui.painter().hline(card.x_range().shrink(1.0), y + 0.5, Stroke::new(1.0, HAIRLINE));
                y += 1.0;
            }
        }

        // Options.
        let enabled = self.view.options_enabled;
        let mut y = card.bottom() + 20.0;
        if self.view.show_picker {
            self.draw_picker(ui, Pos2::new(inner.left(), y), enabled);
            y += 34.0 + 12.0;
        }
        let login_text = if self.login_can {
            setup::LOGIN_TEXT.to_string()
        } else {
            format!("{}{}", setup::LOGIN_TEXT, setup::INSTALLED_ONLY)
        };
        let login_rect = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 36.0));
        let mut on = self.login_on;
        if tick(ui, login_rect, "login", &mut on, enabled && self.login_can, setup::LOGIN_TITLE, &[(&login_text, false)]) {
            actions.push(Action::Login(on));
        }
        y += 36.0 + 12.0;
        let path_rect = Rect::from_min_size(Pos2::new(inner.left(), y), Vec2::new(inner.width(), 36.0));
        let suffix = if self.path_can { "" } else { setup::INSTALLED_ONLY };
        let path_text: [(&str, bool); 4] =
            [("Type ", false), ("kilno", true), (" in any folder to open a session there.", false), (suffix, false)];
        tick(ui, path_rect, "path", &mut self.path_on, enabled && self.path_can, setup::PATH_TITLE, &path_text);

        // Footer.
        let bar = inner.bottom() - 36.0 - 14.0;
        ui.painter().hline(inner.x_range(), bar + 0.5, Stroke::new(1.0, HAIRLINE));
        let row_y = bar + 1.0 + 14.0;
        let check = Rect::from_min_size(Pos2::new(inner.left(), row_y), Vec2::new(text_w(ui, "Check again", ui_font(13.0)) + 28.0, 36.0));
        if outline_button(ui, check, "check", "Check again", ui_font(13.0), TEXT_PRIMARY, hex(0x3B4453), None, !self.busy) {
            actions.push(Action::CheckAgain);
        }
        let open_w = text_w(ui, "Open Kilno", semibold(14.0)) + 36.0;
        let open = Rect::from_min_size(Pos2::new(inner.right() - open_w, row_y), Vec2::new(open_w, 36.0));
        if clay_button(ui, open, "Open Kilno", self.view.can_open) {
            actions.push(Action::Open);
        }
        if self.view.can_open && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            actions.push(Action::Open); // IsDefault
        }
        let note = self.note.as_deref().unwrap_or(&self.view.note);
        let nx = check.right() + 12.0;
        let g = elided(ui, note, ui_font(12.0), MUTED_AA, open.left() - 12.0 - nx);
        let gy = check.center().y - g.size().y / 2.0;
        ui.painter().galley(Pos2::new(nx, gy), g, MUTED_AA);
    }

    fn draw_mascot(&mut self, ui: &mut Ui, rect: Rect) {
        let mv = setup::mascot_move(&self.view);
        if self.mv != Some(mv) {
            self.mv = Some(mv); // only on a change, so ticking Use doesn't jump again
            self.since = Instant::now();
        }
        let island_mv = match mv {
            SetupMove::Peek => MascotMove::Peek,
            SetupMove::Walk => MascotMove::Walk,
            SetupMove::Jump => MascotMove::Jump,
            SetupMove::Idle => MascotMove::Idle,
        };
        let t = self.since.elapsed().as_secs_f64();
        let moving = mascot::moving(island_mv, t);
        // Frames at 30 fps while it moves; a held last pose (no repaint) once it stops.
        const FRAME: Duration = Duration::from_millis(33);
        let frame = if moving { (t / FRAME.as_secs_f64()) as u64 } else { u64::MAX };
        let px = (44.0 * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
        let key = (mv, frame, px);
        if self.texture.as_ref().map(|(_, k)| *k) != Some(key) {
            let t_draw = if moving { t } else { t.max(2.0) };
            let pm = mascot::render(island_mv, Some(t_draw), px);
            let img = egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pm.data());
            match &mut self.texture {
                Some((tex, k)) => {
                    tex.set(img, egui::TextureOptions::NEAREST);
                    *k = key;
                }
                None => {
                    let tex = ui.ctx().load_texture("setup-mascot", img, egui::TextureOptions::NEAREST);
                    self.texture = Some((tex, key));
                }
            }
        }
        if let Some((tex, _)) = &self.texture {
            ui.painter().image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        if moving {
            ui.ctx().request_repaint_after(FRAME);
        }
    }

    fn draw_row(&mut self, ui: &mut Ui, r: Rect, row: &AgentRowView, info: Option<&AgentInfo>, actions: &mut Vec<Action>) {
        let parts = setup::row_parts(row, info);
        let kind = row.kind;
        let left = r.left() + 18.0;
        let right = r.right() - 18.0;
        let head = Rect::from_min_size(Pos2::new(left, r.top() + 14.0), Vec2::new(right - left, 36.0));
        let cy = head.center().y;

        // Badge.
        let b = setup::badge(kind, row.state);
        let badge = Rect::from_min_size(head.min, Vec2::splat(36.0));
        ui.painter().rect_filled(badge, 9, hex(b.bg));
        ui.painter().rect_stroke(badge, 9, Stroke::new(1.0, hex(b.border)), StrokeKind::Inside);
        crate::agent_logo::paint(ui.painter(), Rect::from_center_size(badge.center(), Vec2::splat(20.0)), kind, hex(b.fg));

        // Right side, right to left: Use, Sign in, the pill.
        let mut x = right;
        if parts.use_tick {
            let mut on = match kind {
                AgentKind::Claude => self.use_claude,
                AgentKind::Codex => self.use_codex,
                AgentKind::OpenCode => self.use_opencode,
            };
            let w = 16.0 + 10.0 + text_w(ui, "Use", ui_font(13.0));
            let rect = Rect::from_min_size(Pos2::new(x - w, cy - 10.0), Vec2::new(w, 20.0));
            if small_tick(ui, rect, kind, &mut on, "Use") {
                match kind {
                    AgentKind::Claude => self.use_claude = on,
                    AgentKind::Codex => self.use_codex = on,
                    AgentKind::OpenCode => self.use_opencode = on,
                }
                if !self.busy {
                    self.show_found();
                }
            }
            x = rect.left() - 10.0 - 4.0;
        }
        if parts.sign_in {
            let w = text_w(ui, "Sign in", semibold(12.0)) + 24.0;
            let rect = Rect::from_min_size(Pos2::new(x - w, cy - 15.0), Vec2::new(w, 30.0));
            let id = format!("signin-{}", kind.tag());
            if filled_button(ui, rect, &id, "Sign in") {
                actions.push(Action::SignIn(kind));
            }
            x = rect.left() - 12.0;
        }
        let (pill_text, pill_rgb) = setup::pill(row.state);
        let pc = hex(pill_rgb);
        let pw = 10.0 + 6.0 + 6.0 + text_w(ui, pill_text, semibold(12.0)) + 10.0;
        let pill = Rect::from_min_size(Pos2::new(x - pw, cy - 11.0), Vec2::new(pw, 22.0));
        ui.painter().rect_filled(pill, 10, pc.gamma_multiply(0x1F as f32 / 255.0));
        ui.painter().circle_filled(Pos2::new(pill.left() + 13.0, cy), 3.0, pc);
        ui.painter().text(Pos2::new(pill.left() + 22.0, cy), Align2::LEFT_CENTER, pill_text, semibold(12.0), pc);
        x = pill.left() - 12.0;

        // Name over the grey line.
        let nx = badge.right() + 14.0;
        let name = ui.painter().layout_no_wrap(kind.name().to_string(), semibold(15.0), HEAD);
        let meta = elided(ui, &row.meta, ui_font(12.5), MUTED_AA, (x - nx).max(10.0));
        let total = name.size().y + meta.size().y;
        let ny = cy - total / 2.0;
        let name_h = name.size().y;
        ui.painter().galley(Pos2::new(nx, ny), name, HEAD);
        ui.painter().galley(Pos2::new(nx, ny + name_h), meta, MUTED_AA);

        // A missing agent: its install command, Install (or Get Node.js first) and Locate….
        if parts.fix {
            let fy = head.bottom() + 10.0;
            let line = Rect::from_min_max(Pos2::new(left + 50.0, fy), Pos2::new(right, fy + 30.0));
            let mut x = line.right();
            let lw = text_w(ui, "Locate…", ui_font(12.0));
            let lr = Rect::from_min_size(Pos2::new(x - lw, line.top()), Vec2::new(lw, 30.0));
            let lid = format!("locate-{}", kind.tag());
            if link(ui, lr, &lid, "Locate…") {
                actions.push(Action::Locate(kind));
            }
            x = lr.left() - 10.0;
            if parts.get_node {
                let gw = text_w(ui, "Get Node.js first", ui_font(12.0));
                let gr = Rect::from_min_size(Pos2::new(x - gw, line.top()), Vec2::new(gw, 30.0));
                if link(ui, gr, &format!("get-node-{}", kind.tag()), "Get Node.js first") {
                    crate::os::shell::open(setup::NODE_URL);
                }
                x = gr.left() - 10.0;
            } else {
                let iw = text_w(ui, "Install", semibold(12.0)) + 24.0;
                let ir = Rect::from_min_size(Pos2::new(x - iw, line.top()), Vec2::new(iw, 30.0));
                let iid = format!("install-{}", kind.tag());
                if outline_button(ui, ir, &iid, "Install", semibold(12.0), CLAY_TEXT, CLAY, None, true) {
                    actions.push(Action::Install(kind));
                }
                x = ir.left() - 10.0;
            }
            let cmd = Rect::from_min_max(Pos2::new(line.left(), line.top()), Pos2::new(x, line.bottom()));
            ui.painter().rect_filled(cmd, 6, TERMINAL_BG);
            ui.painter().rect_stroke(cmd, 6, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
            let g = elided(ui, setup::install_shown(kind), mono(12.0), MUTED_AA, cmd.width() - 20.0);
            let gy = cmd.center().y - g.size().y / 2.0;
            ui.painter().galley(Pos2::new(cmd.left() + 10.0, gy), g, MUTED_AA);
        }
    }

    /// "New sessions use" and its choices.
    fn draw_picker(&mut self, ui: &mut Ui, at: Pos2, enabled: bool) {
        let label_fg = if enabled { HEAD } else { DISABLED_FG };
        let label = ui.painter().layout_no_wrap("New sessions use".into(), ui_font(14.0), label_fg);
        let lw = label.size().x;
        ui.painter().galley(Pos2::new(at.x, at.y + 17.0 - label.size().y / 2.0), label, label_fg);
        let choices = [
            (NewSessionAgent::Claude, "Claude Code"),
            (NewSessionAgent::Codex, "Codex"),
            (NewSessionAgent::OpenCode, "OpenCode"),
            (NewSessionAgent::Ask, "Ask each time"),
        ];
        let widths: Vec<f32> = choices.iter().map(|(_, t)| text_w(ui, t, semibold(12.0)) + 24.0).collect();
        let bx = at.x + lw + 12.0;
        let boxr = Rect::from_min_size(Pos2::new(bx, at.y), Vec2::new(widths.iter().sum::<f32>() + 6.0, 34.0));
        ui.painter().rect_filled(boxr, 8, SURFACE_BG);
        ui.painter().rect_stroke(boxr, 8, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
        let mut x = boxr.left() + 3.0;
        for ((value, text), w) in choices.iter().zip(widths) {
            let r = Rect::from_min_size(Pos2::new(x, boxr.top() + 3.0), Vec2::new(w, 28.0));
            let sense = if enabled { Sense::click() } else { Sense::hover() };
            let resp = ui.interact(r, egui::Id::NULL.with(("pick", *text)), sense);
            let checked = self.pick == *value;
            let (bg, fg, font) = if checked {
                (hex(0x2A221F), CLAY_TEXT, semibold(12.0))
            } else if enabled && resp.hovered() {
                (TICK_HOVER, MUTED_AA, ui_font(12.0))
            } else {
                (Color32::TRANSPARENT, MUTED_AA, ui_font(12.0))
            };
            let fg = if enabled { fg } else { DISABLED_FG };
            ui.painter().rect_filled(r, 6, bg);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, *text, font, fg);
            focus_ring(ui, &resp, r);
            if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.pick = *value;
            }
            x += w;
        }
    }
}

fn data_dir() -> Option<kilno_core::paths::DataDir> {
    Some(kilno_core::paths::data_dir(&OsEnv))
}

/// Header line 64; a missing agent's fix line adds 10 + 30.
fn row_height(row: &AgentRowView, info: Option<&AgentInfo>) -> f32 {
    if setup::row_parts(row, info).fix {
        14.0 + 36.0 + 10.0 + 30.0 + 14.0
    } else {
        14.0 + 36.0 + 14.0
    }
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn text_w(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font, TEXT_PRIMARY).size().x
}

/// The Tick style with a title over a grey line (pieces in mono where flagged). The hover wash
/// reaches 10 px left of the box, as the C# margins pull it back. True when flipped.
fn tick(ui: &mut Ui, rect: Rect, id: &str, on: &mut bool, enabled: bool, title: &str, text: &[(&str, bool)]) -> bool {
    let hit = rect.expand2(Vec2::new(10.0, 8.0));
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(hit, egui::Id::NULL.with(("tick", id)), sense);
    if enabled && resp.hovered() {
        ui.painter().rect_filled(hit, 6, TICK_HOVER);
    }
    tick_box(ui, Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 8.0), Vec2::splat(16.0)), *on, enabled);
    let x = rect.left() + 26.0;
    let title_fg = if enabled { HEAD } else { DISABLED_FG };
    let text_fg = if enabled { MUTED_AA } else { DISABLED_FG };
    ui.painter().text(Pos2::new(x, rect.top()), Align2::LEFT_TOP, title, ui_font(14.0), title_fg);
    let mut job = egui::text::LayoutJob::default();
    for (s, is_mono) in text {
        let (font, color) = if *is_mono { (mono(12.5), if enabled { TEXT_PRIMARY } else { DISABLED_FG }) } else { (ui_font(12.5), text_fg) };
        job.append(s, 0.0, egui::TextFormat::simple(font, color));
    }
    job.wrap.max_width = rect.right() - x;
    job.wrap.max_rows = 1;
    let g = ui.painter().layout_job(job);
    ui.painter().galley(Pos2::new(x, rect.top() + 19.0), g, text_fg);
    if enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// The row's "Use" tick (13 px text).
fn small_tick(ui: &mut Ui, rect: Rect, kind: AgentKind, on: &mut bool, label: &str) -> bool {
    let hit = rect.expand2(Vec2::new(10.0, 8.0));
    let resp = ui.interact(hit, egui::Id::NULL.with(("use", kind.tag())), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(hit, 6, TICK_HOVER);
    }
    tick_box(ui, Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - 8.0), Vec2::splat(16.0)), *on, true);
    ui.painter().text(Pos2::new(rect.left() + 26.0, rect.center().y), Align2::LEFT_CENTER, label, ui_font(13.0), TEXT_PRIMARY);
    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
        *on = !*on;
        return true;
    }
    false
}

/// GhostButton with its own rim and text colour (Check again, Install).
#[allow(clippy::too_many_arguments)]
fn outline_button(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    text: &str,
    font: FontId,
    fg: Color32,
    rim: Color32,
    fill: Option<Color32>,
    enabled: bool,
) -> bool {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, egui::Id::NULL.with(("btn", id)), sense);
    let hover = enabled && resp.hovered();
    let p = ui.painter();
    if let Some(f) = fill {
        p.rect_filled(rect, 6, f);
    }
    p.rect_stroke(rect, 6, Stroke::new(1.0, if enabled { rim } else { HAIRLINE }), StrokeKind::Inside);
    if hover {
        p.rect_filled(rect, 6, fg.gamma_multiply(if resp.is_pointer_button_down_on() { 0.03 } else { 0.08 }));
    }
    let face = if !enabled { TEXT_FAINT } else if hover { fg } else { fg.gamma_multiply(0.75) };
    p.text(rect.center(), Align2::CENTER_CENTER, text, font, face);
    enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// Sign in: the GhostButton filled amber, dark semibold text.
fn filled_button(ui: &mut Ui, rect: Rect, id: &str, text: &str) -> bool {
    let resp = ui.interact(rect, egui::Id::NULL.with(("btn", id)), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 6, WORKING);
    if resp.hovered() {
        p.rect_filled(rect, 6, Color32::WHITE.gamma_multiply(0.08));
    }
    p.text(rect.center(), Align2::CENTER_CENTER, text, semibold(12.0), TERMINAL_BG);
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// ClayButton: clay, 8 radius, 14 semibold dark text; #DC8466 hover, #B9654A pressed, grey off.
fn clay_button(ui: &mut Ui, rect: Rect, text: &str, enabled: bool) -> bool {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, egui::Id::NULL.with("open-kilno"), sense);
    let (bg, fg) = if !enabled {
        (hex(0x2A2F38), hex(0x6C7480))
    } else if resp.is_pointer_button_down_on() {
        (hex(0xB9654A), TERMINAL_BG)
    } else if resp.hovered() {
        (hex(0xDC8466), TERMINAL_BG)
    } else {
        (CLAY, TERMINAL_BG)
    };
    ui.painter().rect_filled(rect, 8, bg);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, semibold(14.0), fg);
    focus_ring(ui, &resp, rect);
    enabled && resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

/// A Hyperlink: clay text, underlined.
fn link(ui: &mut Ui, rect: Rect, id: &str, text: &str) -> bool {
    let resp = ui.interact(rect, egui::Id::NULL.with(("link", id)), Sense::click());
    let g = ui.painter().layout_no_wrap(text.to_string(), ui_font(12.0), CLAY_TEXT);
    let pos = Pos2::new(rect.left(), rect.center().y - g.size().y / 2.0);
    let under = pos.y + g.size().y - 1.0;
    let w = g.size().x;
    ui.painter().galley(pos, g, CLAY_TEXT);
    ui.painter().hline(pos.x..=pos.x + w, under, Stroke::new(1.0, CLAY_TEXT));
    resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kilno_core::setup::{checking, from};

    fn info(kind: AgentKind, exe: bool, signed_in: bool, npm: bool) -> AgentInfo {
        AgentInfo {
            kind,
            exe: exe.then(|| "x".to_string()),
            version: exe.then(|| "1.2.3".to_string()),
            signed_in,
            data_folder: String::new(),
            sessions: 0,
            projects: 0,
            npm: npm.then(|| "npm.cmd".to_string()),
        }
    }

    #[test]
    fn rows_grow_by_the_fix_line_only_when_missing() {
        let v = from(&info(AgentKind::Claude, true, true, false), &info(AgentKind::Codex, false, false, false), true, true);
        assert_eq!(row_height(&v.claude, None), 64.0);
        assert_eq!(row_height(&v.codex, None), 104.0);
        assert_eq!(row_height(&checking().claude, None), 64.0);
    }

    #[test]
    fn the_window_fits_its_content_with_both_agents_missing_and_the_picker() {
        // Worst case: both fix lines and the picker (the picker needs both ready, so it never
        // comes with a fix line; check each against the 524 px content).
        let header = 44.0 + 20.0;
        let card = 104.0 + 1.0 + 104.0;
        let options = 20.0 + 36.0 + 12.0 + 36.0;
        let footer = 14.0 + 36.0 + 1.0;
        assert!(28.0 + header + card + options + footer + 22.0 <= SIZE.y);
        let with_picker = 28.0 + header + 129.0 + options + 46.0 + footer + 22.0;
        assert!(with_picker <= SIZE.y);
    }

    #[test]
    fn builder_is_fixed_size_and_titled() {
        let b = builder(Some(Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(1280.0, 820.0))));
        assert_eq!(b.title.as_deref(), Some(TITLE));
        assert_eq!(b.inner_size, Some(SIZE));
        assert_eq!(b.resizable, Some(false));
        let p = b.position.unwrap();
        assert_eq!(p.x, 100.0 + 640.0 - 380.0);
    }
}
