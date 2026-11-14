//! The Settings page (Settings/SettingsPane.xaml + .cs): in the terminal side's place, header
//! included; the sessions keep running behind it. General / Status bar / Security / Agents
//! (opens setup) / Island. Every change is handed back at once as [`Out::Changed`]; the app
//! saves and applies it. The words come from `kilno_core::settings_view`.

use std::collections::HashSet;
use std::path::PathBuf;

use kilno_core::paths::{claude_dir, codex_home, DataDir, OsEnv};
use kilno_core::security::ask_rules::{self, AskGroup, GROUPS};
use kilno_core::security::own_rules::{self, OwnRule, OwnRules};
use kilno_core::settings::{KilnoSettings, StatusTheme, RESERVE_STEPS};
use kilno_core::settings_view as view;
use kilno_core::system::login_startup::LoginStartup;
use kilno_core::system::updater::{self, Version};
use egui::epaint::RectShape;
use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Galley, Id, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, UiBuilder, Vec2,
};
use std::sync::Arc;

use crate::theme::*;

const LATER_TEXT: Color32 = hex(0x6E7683);
const LATER_RIM: Color32 = hex(0x2A3039);
const ROW_HOVER_BG: Color32 = hex(0x1E232B);
const LINK: Color32 = hex(0xE39A7E);
const CHIP_BG: Color32 = hex(0x1F242C);
const BAND_YELLOW: Color32 = hex(0xD9BE5C);
const PAGE_TITLE: Color32 = MENU_HOVER_FG;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Page {
    #[default]
    General,
    Status,
    Security,
    Island,
}

/// What the page asks of the app this frame.
pub enum Out {
    Close,
    Changed(Box<KilnoSettings>),
    OpenSetup,
    Update,
}

/// The app's state the page shows.
pub struct Info {
    pub shot_key_taken: bool,
    pub newer: Option<Version>,
    pub waiting: bool,
}

/// One agent's own rules as shown: chip, name, what was read.
type Own = (&'static str, &'static str, OwnRules);

pub struct SettingsPage {
    data: DataDir,
    page: Page,
    /// Which Ask cards are open, and Your own rules. Not saved: each open starts the same way.
    open_cards: HashSet<&'static str>,
    own_open: bool,
    /// Start at login: (on, can change).
    login: (bool, bool),
    /// Your own rules and a leftover kilno.rules, read for these settings.
    own: Option<(KilnoSettings, Vec<Own>, Option<PathBuf>)>,
}

impl SettingsPage {
    pub fn new(data: DataDir) -> Self {
        Self { data, page: Page::General, open_cards: HashSet::new(), own_open: false, login: (false, false), own: None }
    }

    /// Each time Settings opens (SettingsPane.Refresh): cards closed (a lone card open), Your
    /// own rules closed and read again, the login entry read again.
    pub fn refresh(&mut self) {
        self.open_cards.clear();
        if GROUPS.len() == 1 {
            self.open_cards.insert(GROUPS[0].id);
        }
        self.own_open = false;
        self.own = None;
        self.read_login();
    }

    /// Debug builds' KILNO_DEV_SETTINGS: start on this page (by its nav label).
    pub fn dev_page(&mut self, label: &str) {
        self.page = match label {
            "Status bar" => Page::Status,
            "Security" => Page::Security,
            "Island" => Page::Island,
            _ => Page::General,
        };
    }

    fn read_login(&mut self) {
        let s = LoginStartup::for_this_user(&self.data);
        #[cfg(windows)]
        let on = s.is_on();
        #[cfg(not(windows))]
        let on = false;
        self.login = (on, s.can_write());
    }

    fn set_login(&mut self, on: bool) {
        #[cfg(windows)]
        if let Ok(exe) = std::env::current_exe() {
            let _ = LoginStartup::for_this_user(&self.data).set(on, &exe);
        }
        #[cfg(not(windows))]
        let _ = on;
        self.read_login();
    }

    fn own_rules(&mut self, s: &KilnoSettings) -> (Vec<Own>, Option<PathBuf>) {
        if self.own.as_ref().is_none_or(|(k, ..)| k != s) {
            let mut agents = Vec::new();
            if s.use_claude {
                agents.push(("CC", "Claude Code", own_rules::claude(&claude_dir(&OsEnv).join("settings.json"), s)));
            }
            let home = codex_home(&OsEnv);
            if s.use_codex {
                agents.push(("CX", "Codex", own_rules::codex(home.path(), s)));
            }
            let left = ask_rules::codex_rules_file_still_active(&home, s);
            self.own = Some((s.clone(), agents, left));
        }
        let (_, a, l) = self.own.as_ref().expect("just read");
        (a.clone(), l.clone())
    }

    pub fn show(&mut self, ui: &mut Ui, settings: &KilnoSettings, info: &Info) -> Vec<Out> {
        let mut out = Vec::new();
        let mut s = settings.clone();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0, TERMINAL_BG);

        // The header, as a session's: 44 high, the title strong, a close button.
        let head = top_band(rect, 44.0);
        let p = ui.painter();
        p.rect_filled(head, 0, SURFACE_BG);
        p.hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, HAIRLINE));
        p.text(Pos2::new(head.left() + 14.0, head.center().y), Align2::LEFT_CENTER, "Settings", semibold(12.0), TEXT_PRIMARY);
        let close = icon_button(ui, Id::NULL.with("settings-close"), Pos2::new(head.right() - 14.0, head.center().y), '\u{E8BB}', true);
        if close.on_hover_text("Close (Esc)").clicked() {
            out.push(Out::Close);
        }

        let body = Rect::from_min_max(Pos2::new(rect.left(), head.bottom()), rect.max);
        let nav = Rect::from_min_max(body.min, Pos2::new(body.left() + 188.0, body.bottom()));
        ui.painter().vline(nav.right() - 0.5, nav.y_range(), Stroke::new(1.0, HAIRLINE));
        self.nav(ui, nav, &mut out);

        let content = Rect::from_min_max(Pos2::new(nav.right(), body.top()), body.max);
        ui.scope_builder(UiBuilder::new().max_rect(content), |ui| {
            egui::ScrollArea::vertical().id_salt("settings-scroll").auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let w = (ui.available_width() - 56.0).clamp(200.0, 700.0);
                ui.add_space(22.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(28.0);
                    ui.vertical(|ui| {
                        ui.set_width(w);
                        match self.page {
                            Page::General => self.general(ui, &mut s, info, &mut out),
                            Page::Status => status_page(ui, &mut s),
                            Page::Security => self.security(ui, &mut s),
                            Page::Island => island_page(ui, &mut s, info),
                        }
                    });
                });
                ui.add_space(40.0);
            });
        });

        if s != *settings {
            out.push(Out::Changed(Box::new(s)));
        }
        out
    }

    fn nav(&mut self, ui: &mut Ui, rect: Rect, out: &mut Vec<Out>) {
        let mut y = rect.top() + 14.0;
        let x = rect.left() + 8.0;
        let w = rect.width() - 16.0;
        let items: [(Option<Page>, char, &str); 5] = [
            (Some(Page::General), '\u{E713}', "General"),
            (Some(Page::Status), '\u{E9D2}', "Status bar"),
            (Some(Page::Security), '\u{EA18}', "Security"),
            (None, '\u{E99A}', "Agents"),
            (Some(Page::Island), '\u{E7F4}', "Island"),
        ];
        for (i, (page, glyph, label)) in items.into_iter().enumerate() {
            if i == 3 {
                ui.painter().hline(x + 8.0..=x + w - 8.0, y + 8.5, Stroke::new(1.0, HAIRLINE));
                y += 17.0;
            }
            let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 32.0));
            let resp = ui.interact(r, Id::NULL.with(("settings-nav", label)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            let checked = page == Some(self.page);
            let p = ui.painter();
            if checked {
                p.rect_filled(r, 4, ROW_SELECTED);
                let bar = Rect::from_center_size(Pos2::new(r.left() + 1.0, r.center().y), Vec2::new(2.0, 16.0));
                p.rect_filled(bar, 1, WORKING);
            } else if resp.hovered() {
                p.rect_filled(r, 4, ROW_HOVER);
            }
            p.text(Pos2::new(r.left() + 13.0, r.center().y), Align2::CENTER_CENTER, glyph, icon_font(13.0), MUTED_AA);
            p.text(Pos2::new(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, label, ui_font(12.5), TEXT_PRIMARY);
            focus_ring(ui, &resp, r);
            if resp.clicked() {
                match page {
                    Some(pg) => self.page = pg,
                    // Not a page: it opens setup, and the page you were on stays picked.
                    None => out.push(Out::OpenSetup),
                }
            }
            y += 34.0;
        }
    }

    // ------------------------------------------------------------------ General

    fn general(&mut self, ui: &mut Ui, s: &mut KilnoSettings, info: &Info, out: &mut Vec<Out>) {
        page_head(ui, "General", "How Kilno starts and what the window's close button does.");
        let card = card_begin(ui);
        let (on, can) = self.login;
        let r = row(
            ui,
            Id::NULL.with("login"),
            Row::new(Lead::Icon('\u{E7E8}'), "Start at login", Right::Switch { on, enabled: can })
                .line(view::login_text(can), MUTED_AA)
                .corners(true, false),
        );
        if r.toggled {
            self.set_login(!on);
        }
        row(
            ui,
            Id::NULL.with("close-info"),
            Row::new(Lead::Icon('\u{E768}'), "Close button", Right::None)
                .line(
                    "Closing the window keeps sessions running. Quit Kilno from the gear menu or the island to end them.",
                    MUTED_AA,
                )
                .sep(),
        );
        card_end(ui, card);

        group_label(ui, "Updates", 22.0);
        let card = card_begin(ui);
        let title = format!("Kilno {}", updater::current());
        let mut spec = match info.newer {
            None => Row::new(Lead::Icon('\u{E946}'), &title, Right::None).link("What's new in this version"),
            Some(v) => {
                let (text, button) = view::newer_text(v, info.waiting);
                Row::new(Lead::Icon('\u{E946}'), &title, Right::Button(button)).line_owned(text, MUTED_AA)
            }
        };
        spec = spec.corners(true, false);
        let r = row(ui, Id::NULL.with("updates"), spec);
        if r.link == Some(0) {
            #[cfg(windows)]
            crate::win::shell::open(&updater::release_url(updater::current()));
        }
        if r.button {
            out.push(Out::Update);
        }
        let r = row(
            ui,
            Id::NULL.with("update-check"),
            Row::new(Lead::Icon('\u{E895}'), "Check for updates", Right::Switch { on: s.update_check, enabled: true })
                .line("Once at start and every 12 hours, from GitHub. Updating is always your click.", MUTED_AA)
                .sep()
                .corners(false, true),
        );
        if r.toggled {
            s.update_check = !s.update_check;
        }
        card_end(ui, card);

        group_label(ui, "Coming later", 22.0);
        let top = ui.cursor().top();
        let mut spec = Row::new(Lead::Icon('\u{E7F4}'), "Island", Right::Later)
            .line("Where it sits, what it announces, how long notices stay.", hex(0x5E6672));
        spec.title_color = LATER_TEXT;
        spec.lead_color = hex(0x4A515C);
        row(ui, Id::NULL.with("later-island"), spec);
        let r = Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
        dashed_round_rect(ui, r.shrink(0.5), 8.0, LATER_RIM);
    }

    // ------------------------------------------------------------------ Security

    fn security(&mut self, ui: &mut Ui, s: &mut KilnoSettings) {
        page_head(ui, "Security", "What sessions Kilno starts have to ask you before they do it.");
        let (own, left) = self.own_rules(s);

        // The agents, for every card. Hidden with only Claude in use: the intro says enough.
        if s.use_codex {
            let card = card_begin(ui);
            if s.use_claude {
                let mut spec = Row::new(Lead::Chip("CC"), "Claude Code", Right::None).line("Only sessions Kilno starts.", MUTED_AA);
                spec.pad_v = 11.0;
                spec.min_h = 0.0;
                row(ui, Id::NULL.with("sec-claude"), spec);
            }
            // PORTING.md D1: Kilno never writes Codex's rules, so the switch is shown locked.
            let mut spec = Row::new(Lead::Chip("CX"), "Also apply to Codex", Right::Switch { on: s.also_for_codex, enabled: false })
                .line("Codex rules are read-only in this build", MUTED_AA)
                .corners(!s.use_claude, true);
            if s.use_claude {
                spec = spec.sep();
            }
            if left.is_some() {
                spec = spec.line("Still active in Codex; delete this file to turn it off", BAND_YELLOW).link("Open file");
            }
            spec.pad_v = 11.0;
            spec.min_h = 0.0;
            let r = row(ui, Id::NULL.with("sec-codex"), spec);
            if let (Some(_), Some(path)) = (r.link, &left) {
                #[cfg(windows)]
                crate::win::shell::open_file_or_select(path);
                #[cfg(not(windows))]
                let _ = path;
            }
            card_end(ui, card);
        }

        group_label(ui, "Ask before", if s.use_codex { 22.0 } else { 0.0 });
        let card = card_begin(ui);
        for (i, g) in GROUPS.iter().enumerate() {
            self.ask_card(ui, s, g, i == 0, i == GROUPS.len() - 1);
        }
        card_end(ui, card);
        ui.add_space(10.0);
        wrapped(ui, view::ask_foot(s), ui_font(11.5), MUTED_AA, 2.0);

        ui.add_space(22.0);
        self.own_section(ui, &own);
    }

    fn ask_card(&mut self, ui: &mut Ui, s: &mut KilnoSettings, g: &'static AskGroup, first: bool, last: bool) {
        let on = !s.is_group_off(g.id);
        let open = self.open_cards.contains(g.id);
        let state = view::group_state(s, g);
        let icon = g.icon.chars().next().unwrap_or(' ');
        let mut spec = Row::new(Lead::Icon(icon), g.title, Right::Group { state: &state, on, open })
            .line(g.line, MUTED_AA)
            .corners(first, last && !open);
        spec.pad_r = 10.0;
        if !first {
            spec = spec.sep();
        }
        let r = row(ui, Id::NULL.with(("ask-card", g.id)), spec);
        if r.toggled {
            s.ask_groups_on = view::toggled(&s.ask_groups_on, g.id, !on);
        }
        if r.open_toggled && !self.open_cards.remove(g.id) {
            self.open_cards.insert(g.id);
        }
        if !open {
            return;
        }
        // Off: the list stays readable but can't be clicked, so you see what you'd get back.
        let w = ui.available_width();
        let top = ui.cursor().top();
        ui.painter().hline(ui.max_rect().x_range(), top + 0.5, Stroke::new(1.0, HAIRLINE));
        ui.add_space(6.0);
        for rule in g.rules {
            let v = view::rule_view(s, g, rule);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::hover());
            let inner = rect.shrink2(Vec2::new(8.0, 0.0));
            let tick_r = Rect::from_min_size(inner.min, Vec2::new(210.0, 32.0));
            let yes = !s.is_ask_off(rule.id);
            if tick(ui, tick_r, Id::NULL.with(("ask-rule", rule.id)), yes, on, rule.label).clicked() {
                s.ask_off = view::toggled(&s.ask_off, rule.id, yes);
            }
            // The chips, right; the line between, cut to fit.
            let mut x = inner.right() - 10.0;
            let cy = inner.center().y;
            if s.use_codex {
                x = agent_chip(ui, Pos2::new(x, cy), "CX", if v.codex_active { 1.0 } else { 0.4 }, Some(v.codex_tip)) - 4.0;
            }
            if s.use_claude {
                x = agent_chip(ui, Pos2::new(x, cy), "CC", if v.claude_active { 1.0 } else { 0.4 }, Some(v.claude_tip));
            }
            let lx = tick_r.right() + 2.0;
            let line_c = TEXT_MUTED.gamma_multiply(if on { 1.0 } else { 0.6 });
            let g = elided(ui, &v.line, mono(11.0), line_c, (x - 12.0 - lx).max(1.0));
            ui.painter().galley(Pos2::new(lx, cy - g.size().y / 2.0), g, line_c);
        }
        ui.add_space(6.0);
    }

    fn own_section(&mut self, ui: &mut Ui, own: &[Own]) {
        let card = card_begin(ui);
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 40.0), Sense::hover());
        let resp = ui.interact(rect, Id::NULL.with("own-head"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
        let radius = if self.own_open { CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 } } else { CornerRadius::same(7) };
        if resp.hovered() {
            ui.painter().rect_filled(rect, radius, ROW_HOVER_BG);
        }
        let cy = rect.center().y;
        let p = ui.painter();
        let label = p.layout_no_wrap("Your own rules".into(), semibold(12.0), TEXT_PRIMARY);
        let lw = label.size().x;
        p.galley(Pos2::new(rect.left() + 16.0, cy - label.size().y / 2.0), label, TEXT_PRIMARY);
        later_tag(ui, Pos2::new(rect.left() + 16.0 + lw + 8.0, cy), "read-only", 10.0, 5.0);
        let chevron = icon_button(ui, Id::NULL.with("own-chevron"), Pos2::new(rect.right() - 10.0, cy), chevron_glyph(self.own_open), false);
        let counts = view::own_counts(&own.iter().map(|(_, n, r)| (*n, r.rules.len())).collect::<Vec<_>>());
        ui.painter().text(Pos2::new(chevron.rect.left() - 8.0, cy), Align2::RIGHT_CENTER, counts, ui_font(11.5), MUTED_AA);
        if resp.clicked() || chevron.clicked() {
            self.own_open = !self.own_open;
        }

        if self.own_open {
            ui.painter().hline(rect.x_range(), rect.bottom() + 0.5, Stroke::new(1.0, HAIRLINE));
            for (i, (chip, name, rules)) in own.iter().enumerate() {
                let (head, _) = ui.allocate_exact_size(Vec2::new(w, 40.0), Sense::hover());
                if i > 0 {
                    ui.painter().hline(head.x_range(), head.top() + 0.5, Stroke::new(1.0, HAIRLINE));
                }
                ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, HAIRLINE));
                let cy = head.center().y;
                let cw = chip_width(ui, chip);
                agent_chip(ui, Pos2::new(head.left() + 16.0 + cw, cy), chip, 1.0, None);
                ui.painter().text(Pos2::new(head.left() + 16.0 + cw + 10.0, cy), Align2::LEFT_CENTER, *name, ui_font(13.0), TEXT_PRIMARY);
                if let Some(file) = &rules.file {
                    let g = ui.painter().layout_no_wrap("Open file".into(), ui_font(11.5), LINK);
                    let r = Rect::from_min_size(Pos2::new(head.right() - 16.0 - g.size().x, cy - g.size().y / 2.0), g.size());
                    if link_at(ui, Id::NULL.with(("own-open", *chip)), r, g).clicked() {
                        #[cfg(windows)]
                        crate::win::shell::open_file_or_select(file);
                        #[cfg(not(windows))]
                        let _ = file;
                    }
                }
                ui.add_space(4.0);
                if let Some(problem) = &rules.problem {
                    ui.add_space(2.0);
                    indented(ui, 16.0, |ui| wrapped(ui, problem, ui_font(11.5), MUTED_AA, 0.0));
                    ui.add_space(6.0);
                }
                for (j, r) in rules.rules.iter().enumerate() {
                    own_row(ui, Id::NULL.with(("own-rule", *chip, j)), r);
                }
                ui.add_space(4.0);
            }
        }
        card_end(ui, card);
    }
}

// ---------------------------------------------------------------------- Status bar

fn status_page(ui: &mut Ui, s: &mut KilnoSettings) {
    page_head(
        ui,
        "Status bar",
        "Live session info drawn by Kilno itself, not the terminal. Colours follow how full each limit is.",
    );
    let card = card_begin(ui);
    let themes: Vec<String> = StatusTheme::ALL.iter().map(|t| t.name().to_string()).collect();
    let chosen = StatusTheme::ALL.iter().position(|t| *t == s.theme).unwrap_or(0);
    let r = row(
        ui,
        Id::NULL.with("theme"),
        Row::new(Lead::Icon('\u{E790}'), "Theme", Right::Segments { labels: &themes, chosen })
            .line("How the numbers are drawn, in the pane header and the sidebar footer.", MUTED_AA),
    );
    if let Some(i) = r.segment {
        s.theme = StatusTheme::ALL[i];
    }
    card_end(ui, card);

    group_label(ui, "Where it shows", 22.0);
    let card = card_begin(ui);
    let r = row(
        ui,
        Id::NULL.with("status-header"),
        Row::new(Lead::Icon('\u{E737}'), "Show in the pane header", Right::Switch { on: s.status_header, enabled: true })
            .line("Model, branch, context and effort beside the session title.", MUTED_AA)
            .corners(true, false),
    );
    if r.toggled {
        s.status_header = !s.status_header;
    }
    let r = row(
        ui,
        Id::NULL.with("status-footer"),
        Row::new(Lead::Icon('\u{E90C}'), "Show limits in the sidebar footer", Right::Switch { on: s.status_footer, enabled: true })
            .line(
                "The 5-hour and 7-day limits are account-wide, so they sit above New session. Off moves them into the pane header.",
                MUTED_AA,
            )
            .sep()
            .corners(false, true),
    );
    if r.toggled {
        s.status_footer = !s.status_footer;
    }
    card_end(ui, card);

    group_label(ui, "Fields", 22.0);
    let card = card_begin(ui);
    ui.add_space(6.0);
    // A field whose every placement is off keeps its tick but greys out, and says why.
    let (head, foot) = (s.status_header, s.status_footer);
    let (why_head, why_foot) = (view::why_head(s), view::why_foot(s));
    indented(ui, 6.0, |ui| {
        let w = ui.available_width() - 6.0;
        field_group(ui, "Per session", why_head);
        tick_row(
            ui,
            "session",
            w,
            &mut [
                ("Model", &mut s.status_model, head),
                ("Branch and diff", &mut s.status_branch, head),
                ("Context used", &mut s.status_context, head),
                ("Effort", &mut s.status_effort, head),
            ],
        );
        field_group(ui, "Account", why_foot);
        tick_row(ui, "account", w, &mut [("5-hour limit", &mut s.status_five_hour, head || foot), ("7-day limit", &mut s.status_seven_day, head || foot)]);
    });
    ui.add_space(6.0);
    card_end(ui, card);

    group_label(ui, "Reserve", 22.0);
    let card = card_begin(ui);
    let steps: Vec<String> = RESERVE_STEPS.iter().map(|&n| if n == 0 { "Off".into() } else { format!("{n}%") }).collect();
    let chosen = RESERVE_STEPS.iter().position(|&n| n == s.reserve_at).unwrap_or(0);
    let r = row(
        ui,
        Id::NULL.with("reserve"),
        Row::new(Lead::Icon('\u{E7BA}'), "Warn near a limit", Right::Segments { labels: &steps, chosen }).line(
            "Past this much of a limit, the island says so once and the limit turns red. Prompts still go through.",
            MUTED_AA,
        ),
    );
    if let Some(i) = r.segment {
        s.reserve_at = RESERVE_STEPS[i];
    }
    // With no reserve there is nothing for the ticks to watch.
    let watch = s.reserve_at > 0;
    indented(ui, 6.0, |ui| {
        let w = ui.available_width() - 6.0;
        tick_row(ui, "reserve", w, &mut [("5-hour limit", &mut s.reserve_five_hour, watch), ("7-day limit", &mut s.reserve_seven_day, watch)]);
    });
    ui.add_space(6.0);
    card_end(ui, card);
}

/// A FieldGroup line: 11 px muted label, maybe a fainter note 8 px after it.
fn field_group(ui: &mut Ui, label: &str, note: &str) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 25.0), Sense::hover());
    let p = ui.painter();
    let g = p.layout_no_wrap(label.into(), ui_font(11.0), MUTED_AA);
    let lw = g.size().x;
    let y = rect.top() + 8.0;
    p.galley(Pos2::new(rect.left() + 10.0, y), g, MUTED_AA);
    if !note.is_empty() {
        p.text(Pos2::new(rect.left() + 18.0 + lw, y), Align2::LEFT_TOP, note, ui_font(11.0), LATER_TEXT);
    }
}

/// Ticks in four equal columns.
fn tick_row(ui: &mut Ui, key: &str, w: f32, ticks: &mut [(&str, &mut bool, bool)]) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::hover());
    let cw = w / 4.0;
    for (i, (label, on, enabled)) in ticks.iter_mut().enumerate() {
        let r = Rect::from_min_size(rect.min + Vec2::new(cw * i as f32, 0.0), Vec2::new(cw, 32.0));
        if tick(ui, r, Id::NULL.with(("field-tick", key, *label)), **on, *enabled, label).clicked() {
            **on = !**on;
        }
    }
}

// ---------------------------------------------------------------------- Island

fn island_page(ui: &mut Ui, s: &mut KilnoSettings, info: &Info) {
    page_head(
        ui,
        "Island",
        "Send a screenshot to a session: the island asks which one, and the image lands in its prompt, ready for your question.",
    );
    group_label(ui, "Screenshots", 0.0);
    let card = card_begin(ui);
    let r = row(
        ui,
        Id::NULL.with("shot-key"),
        Row::new(Lead::Icon('\u{E92E}'), "Send a screenshot with Ctrl+Alt+S", Right::Switch { on: s.screenshot_hotkey, enabled: true })
            .line(view::shot_key_text(s.screenshot_hotkey, info.shot_key_taken), MUTED_AA)
            .corners(true, false),
    );
    if r.toggled {
        s.screenshot_hotkey = !s.screenshot_hotkey;
    }
    let r = row(
        ui,
        Id::NULL.with("shot-offer"),
        Row::new(Lead::Icon('\u{E722}'), "Offer to send every new screenshot", Right::Switch { on: s.screenshot_offer, enabled: true })
            .line(
                "The island asks each time you take one with Win+Shift+S, without the shortcut. Not while Kilno is in front.",
                MUTED_AA,
            )
            .sep()
            .corners(false, true),
    );
    if r.toggled {
        s.screenshot_offer = !s.screenshot_offer;
    }
    card_end(ui, card);
}

// ---------------------------------------------------------------------- pieces

fn page_head(ui: &mut Ui, title: &str, intro: &str) {
    wrapped(ui, title, semibold(18.0), PAGE_TITLE, 0.0);
    ui.add_space(4.0);
    wrapped(ui, intro, ui_font(12.5), MUTED_AA, 0.0);
    ui.add_space(18.0);
}

fn group_label(ui: &mut Ui, text: &str, top: f32) {
    ui.add_space(top);
    wrapped(ui, text, semibold(12.0), TEXT_PRIMARY, 0.0);
    ui.add_space(8.0);
}

/// A wrapped paragraph `inset` px in from both sides.
fn wrapped(ui: &mut Ui, text: &str, font: FontId, color: Color32, inset: f32) {
    let w = ui.available_width();
    let g = ui.painter().layout(text.to_string(), font, color, (w - 2.0 * inset).max(1.0));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, g.size().y), Sense::hover());
    ui.painter().galley(rect.min + Vec2::new(inset, 0.0), g, color);
}

fn indented(ui: &mut Ui, by: f32, add: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        ui.add_space(by);
        ui.vertical(add);
    });
}

/// A card: SurfaceBg, hairline rim, radius 8, behind whatever rows follow until [`card_end`].
fn card_begin(ui: &mut Ui) -> (egui::layers::ShapeIdx, f32) {
    (ui.painter().add(Shape::Noop), ui.cursor().top())
}

fn card_end(ui: &mut Ui, (idx, top): (egui::layers::ShapeIdx, f32)) {
    let r = Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
    ui.painter().set(idx, RectShape::new(r, 8, SURFACE_BG, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside));
}

enum Lead<'a> {
    Icon(char),
    Chip(&'a str),
}

enum Right<'a> {
    None,
    Switch { on: bool, enabled: bool },
    Segments { labels: &'a [String], chosen: usize },
    Button(&'a str),
    Later,
    /// An Ask card's header: state text, switch, chevron.
    Group { state: &'a str, on: bool, open: bool },
}

struct Line {
    text: String,
    color: Color32,
    link: bool,
}

struct Row<'a> {
    lead: Lead<'a>,
    lead_color: Color32,
    title: &'a str,
    title_color: Color32,
    lines: Vec<Line>,
    right: Right<'a>,
    pad_v: f32,
    pad_r: f32,
    min_h: f32,
    radius: CornerRadius,
    sep: bool,
}

impl<'a> Row<'a> {
    fn new(lead: Lead<'a>, title: &'a str, right: Right<'a>) -> Self {
        Self {
            lead,
            lead_color: MUTED_AA,
            title,
            title_color: TEXT_PRIMARY,
            lines: Vec::new(),
            right,
            pad_v: 13.0,
            pad_r: 16.0,
            min_h: 34.0,
            radius: CornerRadius::ZERO,
            sep: false,
        }
    }
    fn line(self, text: &str, color: Color32) -> Self {
        self.line_owned(text.to_string(), color)
    }
    fn line_owned(mut self, text: String, color: Color32) -> Self {
        self.lines.push(Line { text, color, link: false });
        self
    }
    fn link(mut self, text: &str) -> Self {
        self.lines.push(Line { text: text.to_string(), color: LINK, link: true });
        self
    }
    /// A hairline above the row.
    fn sep(mut self) -> Self {
        self.sep = true;
        self
    }
    /// Rounded hover corners for the card's first / last row.
    fn corners(mut self, top: bool, bottom: bool) -> Self {
        let t = if top { 7 } else { 0 };
        let b = if bottom { 7 } else { 0 };
        self.radius = CornerRadius { nw: t, ne: t, sw: b, se: b };
        self
    }
}

#[derive(Default)]
struct RowOut {
    toggled: bool,
    open_toggled: bool,
    segment: Option<usize>,
    button: bool,
    /// Which link line (counting links only) was clicked.
    link: Option<usize>,
}

fn segment_size(ui: &Ui, label: &str) -> Vec2 {
    ui.painter().layout_no_wrap(label.to_string(), ui_font(12.0), MUTED_AA).size() + Vec2::new(22.0, 10.0)
}

fn right_size(ui: &Ui, r: &Right) -> Vec2 {
    match r {
        Right::None => Vec2::ZERO,
        Right::Switch { .. } => Vec2::new(102.0, 20.0),
        Right::Segments { labels, .. } => {
            let sizes: Vec<Vec2> = labels.iter().map(|l| segment_size(ui, l)).collect();
            Vec2::new(20.0 + sizes.iter().map(|s| s.x + 6.0).sum::<f32>(), sizes.iter().map(|s| s.y).fold(0.0, f32::max))
        }
        Right::Button(t) => Vec2::new(20.0 + text_w(ui, t, semibold(12.0)) + 24.0, 30.0),
        Right::Later => Vec2::new(20.0 + text_w(ui, "Later", ui_font(10.5)) + 14.0, 20.0),
        Right::Group { state, .. } => Vec2::new(text_w(ui, state, ui_font(12.0)) + 40.0 + 40.0 + 38.0, 28.0),
    }
}

fn text_w(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font, TEXT_PRIMARY).size().x
}

/// A card row (the Row / SwitchRow styles): icon or agent chip, title over its lines, the
/// control on the right. A switch row toggles from a click anywhere on it.
fn row(ui: &mut Ui, id: Id, r: Row) -> RowOut {
    let mut out = RowOut::default();
    let w = ui.available_width();
    let rs = right_size(ui, &r.right);
    let lead_w = match r.lead {
        Lead::Icon(_) => 40.0,
        Lead::Chip(t) => chip_width(ui, t) + 22.0,
    };
    let wrap_w = (w - 16.0 - r.pad_r - lead_w - rs.x).max(40.0);
    let title = ui.painter().layout(r.title.to_string(), ui_font(13.0), r.title_color, wrap_w);
    let lines: Vec<Arc<Galley>> =
        r.lines.iter().map(|l| ui.painter().layout(l.text.clone(), ui_font(11.5), l.color, wrap_w)).collect();
    let text_h = title.size().y + lines.iter().map(|g| 2.0 + g.size().y).sum::<f32>();
    let content_h = text_h.max(r.min_h).max(rs.y);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, content_h + 2.0 * r.pad_v), Sense::hover());

    let toggles = matches!(r.right, Right::Switch { enabled: true, .. } | Right::Group { .. });
    let resp = ui.interact(rect, id, if toggles { Sense::click() } else { Sense::hover() });
    let resp = if toggles { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp };
    if toggles && resp.hovered() {
        ui.painter().rect_filled(rect, r.radius, ROW_HOVER_BG);
    }
    if r.sep {
        ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, HAIRLINE));
    }
    let cy = rect.center().y;
    let mut x = rect.left() + 16.0;
    match r.lead {
        Lead::Icon(c) => {
            ui.painter().text(Pos2::new(x + 10.0, cy), Align2::CENTER_CENTER, c, icon_font(16.0), r.lead_color);
        }
        Lead::Chip(t) => {
            agent_chip(ui, Pos2::new(x + chip_width(ui, t), cy), t, 1.0, None);
        }
    }
    x += lead_w;
    let mut y = cy - text_h / 2.0;
    let th = title.size().y;
    ui.painter().galley(Pos2::new(x, y), title, r.title_color);
    y += th;
    let mut link_n = 0;
    for (l, g) in r.lines.iter().zip(lines) {
        y += 2.0;
        let gh = g.size().y;
        if l.link {
            let lr = Rect::from_min_size(Pos2::new(x, y), g.size());
            if link_at(ui, id.with(("link", link_n)), lr, g).clicked() {
                out.link = Some(link_n);
            }
            link_n += 1;
        } else {
            ui.painter().galley(Pos2::new(x, y), g, l.color);
        }
        y += gh;
    }

    let right = rect.right() - r.pad_r;
    match r.right {
        Right::None => {}
        Right::Switch { on, enabled } => {
            let sw = Rect::from_min_size(Pos2::new(right - 40.0, cy - 10.0), Vec2::new(40.0, 20.0));
            let hover = ui.rect_contains_pointer(sw);
            paint_switch(ui, sw, id.with("switch"), on, enabled, hover);
            focus_ring(ui, &resp, sw);
            ui.painter().text(Pos2::new(sw.left() - 20.0, cy), Align2::RIGHT_CENTER, view::on_off(on), ui_font(12.0), MUTED_AA);
            out.toggled = enabled && resp.clicked();
        }
        Right::Segments { labels, chosen } => {
            let sizes: Vec<Vec2> = labels.iter().map(|l| segment_size(ui, l)).collect();
            let mut sx = right - sizes.iter().map(|s| s.x).sum::<f32>() - 6.0 * (sizes.len() as f32 - 1.0);
            for (i, (label, size)) in labels.iter().zip(sizes).enumerate() {
                let sr = Rect::from_min_size(Pos2::new(sx, cy - size.y / 2.0), size);
                let sresp = ui.interact(sr, id.with(("seg", i)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
                let picked = i == chosen;
                let p = ui.painter();
                let (bg, rim, fg) = if picked {
                    (Color32::from_rgba_unmultiplied(0xF2, 0x99, 0x4A, 0x24), CLAY, TEXT_PRIMARY)
                } else if sresp.hovered() {
                    (ROW_HOVER_BG, HAIRLINE, MUTED_AA)
                } else {
                    (Color32::TRANSPARENT, HAIRLINE, MUTED_AA)
                };
                p.rect_filled(sr, 6, bg);
                p.rect_stroke(sr, 6, Stroke::new(1.0, rim), StrokeKind::Inside);
                p.text(sr.center(), Align2::CENTER_CENTER, label, ui_font(12.0), fg);
                focus_ring(ui, &sresp, sr);
                if sresp.clicked() && !picked {
                    out.segment = Some(i);
                }
                sx += size.x + 6.0;
            }
        }
        Right::Button(label) => {
            let bw = text_w(ui, label, semibold(12.0)) + 24.0;
            let br = Rect::from_min_size(Pos2::new(right - bw, cy - 15.0), Vec2::new(bw, 30.0));
            let bresp = ui.interact(br, id.with("button"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            let p = ui.painter();
            let face = if bresp.is_pointer_button_down_on() {
                0.62
            } else if bresp.hovered() {
                1.0
            } else {
                0.75
            };
            if bresp.hovered() {
                p.rect_filled(br, 6, LINK.gamma_multiply(0.08));
            }
            p.rect_stroke(br, 6, Stroke::new(1.0, CLAY), StrokeKind::Inside);
            p.text(br.center(), Align2::CENTER_CENTER, label, semibold(12.0), LINK.gamma_multiply(face));
            focus_ring(ui, &bresp, br);
            out.button = bresp.clicked();
        }
        Right::Later => {
            let tw = text_w(ui, "Later", ui_font(10.5)) + 14.0;
            later_tag(ui, Pos2::new(right - tw, cy), "Later", 10.5, 6.0);
        }
        Right::Group { state, on, open } => {
            let ch = icon_button(ui, id.with("chevron"), Pos2::new(right, cy), chevron_glyph(open), false);
            let sw = Rect::from_min_size(Pos2::new(ch.rect.left() - 10.0 - 40.0, cy - 10.0), Vec2::new(40.0, 20.0));
            let swr = ui.interact(sw, id.with("switch-hit"), Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
            paint_switch(ui, sw, id.with("switch"), on, true, swr.hovered());
            focus_ring(ui, &swr, sw);
            ui.painter().text(Pos2::new(sw.left() - 20.0, cy), Align2::RIGHT_CENTER, state, ui_font(12.0), MUTED_AA);
            out.toggled = swr.clicked();
            // A click on the header opens or closes the card; one on its switch or chevron is theirs.
            out.open_toggled = ch.clicked() || resp.clicked();
        }
    }
    out
}

fn chevron_glyph(open: bool) -> char {
    if open {
        '\u{E70E}'
    } else {
        '\u{E70D}'
    }
}

/// A QuietButton holding a 10 px Segoe glyph: 28x28 (or the header's ✕ with padding 9,6 and a
/// hairline rim), its right edge at `right_center`.
fn icon_button(ui: &mut Ui, id: Id, right_center: Pos2, glyph: char, rim: bool) -> Response {
    let size = if rim {
        ui.painter().layout_no_wrap(glyph.to_string(), icon_font(10.0), MUTED_AA).size() + Vec2::new(18.0, 12.0)
    } else {
        Vec2::splat(28.0)
    };
    let r = Rect::from_min_size(Pos2::new(right_center.x - size.x, right_center.y - size.y / 2.0), size);
    let resp = ui.interact(r, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, 4, RAISED_BG);
    }
    if rim {
        p.rect_stroke(r, 4, Stroke::new(1.0, HAIRLINE), StrokeKind::Inside);
    }
    let fg = if rim && resp.hovered() {
        TEXT_PRIMARY
    } else if rim {
        TEXT_MUTED
    } else {
        MUTED_AA
    };
    p.text(r.center(), Align2::CENTER_CENTER, glyph, icon_font(10.0), fg);
    resp
}

/// A hyperlink galley at `r`, underlined, hand cursor.
fn link_at(ui: &Ui, id: Id, r: Rect, g: Arc<Galley>) -> Response {
    let resp = ui.interact(r, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let p = ui.painter();
    p.galley(r.min, g, LINK);
    p.hline(r.x_range(), r.bottom() - 1.0, Stroke::new(1.0, LINK));
    resp
}

/// The "Later" / "read-only" tag: LaterRim border, radius 4, faint text; left edge at `left.x`.
fn later_tag(ui: &Ui, left: Pos2, text: &str, size: f32, pad: f32) {
    let g = ui.painter().layout_no_wrap(text.to_string(), ui_font(size), LATER_TEXT);
    let h = if size > 10.0 { 20.0 } else { 18.0 };
    let r = Rect::from_min_size(Pos2::new(left.x, left.y - h / 2.0), Vec2::new(g.size().x + 2.0 * pad + 2.0, h));
    ui.painter().rect_stroke(r, 4, Stroke::new(1.0, LATER_RIM), StrokeKind::Inside);
    ui.painter().galley(r.center() - g.size() / 2.0, g, LATER_TEXT);
}

fn chip_width(ui: &Ui, text: &str) -> f32 {
    text_w(ui, text, semibold(10.0)) + 14.0
}

/// A CC / CX chip, its right edge at `right_center`; dim when that agent doesn't get the rule.
/// Returns its left edge.
fn agent_chip(ui: &Ui, right_center: Pos2, text: &str, opacity: f32, tip: Option<&str>) -> f32 {
    let w = chip_width(ui, text);
    let r = Rect::from_min_size(Pos2::new(right_center.x - w, right_center.y - 9.0), Vec2::new(w, 18.0));
    let p = ui.painter();
    p.rect_filled(r, 4, CHIP_BG.gamma_multiply(opacity));
    p.rect_stroke(r, 4, Stroke::new(1.0, MENU_RIM.gamma_multiply(opacity)), StrokeKind::Inside);
    p.text(r.center(), Align2::CENTER_CENTER, text, semibold(10.0), TEXT_PRIMARY.gamma_multiply(opacity));
    if let Some(tip) = tip {
        ui.interact(r, Id::NULL.with(("agent-chip", text, r.min.x as i32, r.min.y as i32)), Sense::hover()).on_hover_text(tip);
    }
    r.left()
}

/// The Tick style: 16 px box, 12.5 px label 10 px after it, padding 10,8, hover wash; faint and
/// dead when disabled.
fn tick(ui: &Ui, rect: Rect, id: Id, on: bool, enabled: bool, label: &str) -> Response {
    let resp = ui.interact(rect, id, if enabled { Sense::click() } else { Sense::hover() });
    let resp = if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp };
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 6, ROW_HOVER_BG);
    }
    let bx = Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.center().y - 8.0), Vec2::splat(16.0));
    tick_box(ui, bx, on, enabled);
    let fg = if enabled { TEXT_PRIMARY } else { LATER_TEXT };
    let g = elided(ui, label, ui_font(12.5), fg, (rect.right() - bx.right() - 14.0).max(1.0));
    ui.painter().galley(Pos2::new(bx.right() + 10.0, rect.center().y - g.size().y / 2.0), g, fg);
    focus_ring(ui, &resp, bx);
    resp
}

/// An own rule: decision pill, mono rule text (tooltip: all of it), note in NeedsInput.
fn own_row(ui: &mut Ui, id: Id, rule: &OwnRule) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::hover());
    let inner = rect.shrink2(Vec2::new(16.0, 3.0));
    let cy = inner.center().y;
    let (fg, bg) = view::decision_colors(&rule.decision);
    let bg = Color32::from_rgba_unmultiplied((bg >> 16) as u8, (bg >> 8) as u8, bg as u8, (bg >> 24) as u8);
    let pill = Rect::from_min_size(Pos2::new(inner.left(), cy - 9.0), Vec2::new(52.0, 18.0));
    let p = ui.painter();
    p.rect_filled(pill, 4, bg);
    p.text(pill.center(), Align2::CENTER_CENTER, &rule.decision, semibold(10.5), hex(fg));
    let note_w = if rule.note.is_empty() {
        0.0
    } else {
        let g = p.layout_no_wrap(rule.note.clone(), ui_font(11.0), NEEDS_INPUT);
        let nw = g.size().x;
        p.galley(Pos2::new(inner.right() - nw, cy - g.size().y / 2.0), g, NEEDS_INPUT);
        nw + 12.0
    };
    let tx = inner.left() + 64.0;
    let g = elided(ui, &rule.rule, mono(11.5), TEXT_PRIMARY, (inner.right() - note_w - tx).max(1.0));
    let tr = Rect::from_min_size(Pos2::new(tx, cy - g.size().y / 2.0), g.size());
    ui.painter().galley(tr.min, g, TEXT_PRIMARY);
    ui.interact(tr, id, Sense::hover()).on_hover_text(&rule.rule);
}

/// A dashed rounded rectangle (WPF borders draw no dashes, so the C# uses a Rectangle too).
fn dashed_round_rect(ui: &Ui, r: Rect, radius: f32, color: Color32) {
    let mut pts = Vec::new();
    let corners = [
        (Pos2::new(r.right() - radius, r.top() + radius), -90.0f32),
        (Pos2::new(r.right() - radius, r.bottom() - radius), 0.0),
        (Pos2::new(r.left() + radius, r.bottom() - radius), 90.0),
        (Pos2::new(r.left() + radius, r.top() + radius), 180.0),
    ];
    for (c, start) in corners {
        for k in 0..=6 {
            let a = (start + 15.0 * k as f32).to_radians();
            pts.push(c + Vec2::new(a.cos(), a.sin()) * radius);
        }
    }
    pts.push(pts[0]);
    ui.painter().extend(Shape::dashed_line(&pts, Stroke::new(1.0, color), 4.0, 3.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, RawInput};

    struct Harness {
        ctx: egui::Context,
        page: SettingsPage,
        s: KilnoSettings,
    }

    impl Harness {
        fn new() -> Self {
            let ctx = egui::Context::default();
            ctx.set_fonts(crate::fonts::definitions());
            let data = DataDir::new(std::env::temp_dir().join("kilno-settings-page-test"));
            let mut page = SettingsPage::new(data);
            page.refresh();
            let mut h = Self { ctx, page, s: KilnoSettings::default() };
            h.frame(vec![]);
            h
        }

        /// One pass; what the page asked for, with Changed applied as the app would.
        fn frame(&mut self, events: Vec<Event>) -> Vec<&'static str> {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 900.0))),
                events,
                ..Default::default()
            };
            let info = Info { shot_key_taken: true, newer: None, waiting: false };
            let mut outs = Vec::new();
            let (page, s) = (&mut self.page, &self.s);
            let mut full = self.ctx.run_ui(input, |ui| outs = page.show(ui, s, &info));
            full.textures_delta.clear();
            outs.into_iter()
                .map(|o| match o {
                    Out::Close => "close",
                    Out::OpenSetup => "setup",
                    Out::Update => "update",
                    Out::Changed(n) => {
                        self.s = *n;
                        "changed"
                    }
                })
                .collect()
        }

        fn click(&mut self, id: Id) -> Vec<&'static str> {
            let pos = self.ctx.read_response(id).expect("widget drawn").rect.center();
            self.frame(vec![Event::PointerMoved(pos)]);
            let press = |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Default::default() };
            self.frame(vec![press(true)]);
            let out = self.frame(vec![press(false)]);
            self.frame(vec![]);
            out
        }

        fn go(&mut self, label: &str) -> Vec<&'static str> {
            self.click(Id::NULL.with(("settings-nav", label)))
        }
    }

    #[test]
    fn every_page_draws_and_agents_opens_setup_without_leaving_the_page() {
        let mut h = Harness::new();
        h.s.use_codex = true;
        for page in ["Status bar", "Security", "Island", "General"] {
            assert!(h.go(page).is_empty());
        }
        assert_eq!(h.go("Agents"), ["setup"]);
        assert!(h.page.page == Page::General);
        assert_eq!(h.click(Id::NULL.with("settings-close")), ["close"]);
    }

    #[test]
    fn a_click_anywhere_on_a_switch_row_saves_at_once() {
        let mut h = Harness::new();
        assert_eq!(h.click(Id::NULL.with("update-check")), ["changed"]);
        assert!(!h.s.update_check);
        h.go("Island");
        assert_eq!(h.click(Id::NULL.with("shot-offer")), ["changed"]);
        assert!(h.s.screenshot_offer);
    }

    #[test]
    fn segments_ticks_and_disabled_ticks() {
        let mut h = Harness::new();
        h.go("Status bar");
        assert_eq!(h.click(Id::NULL.with("reserve").with(("seg", 2))), ["changed"]);
        assert_eq!(h.s.reserve_at, 80);
        assert_eq!(h.click(Id::NULL.with("theme").with(("seg", 3))), ["changed"]);
        assert_eq!(h.s.theme, StatusTheme::Chips);
        assert_eq!(h.click(Id::NULL.with(("field-tick", "session", "Effort"))), ["changed"]);
        assert!(!h.s.status_effort);
        // Header off: the per-session ticks are dead.
        assert_eq!(h.click(Id::NULL.with("status-header")), ["changed"]);
        assert!(h.click(Id::NULL.with(("field-tick", "session", "Model"))).is_empty());
        assert!(h.s.status_model);
    }

    #[test]
    fn security_group_switch_rule_ticks_and_the_locked_codex_switch() {
        let mut h = Harness::new();
        h.s.use_codex = true;
        h.go("Security");
        // The lone card starts open; its switch turns the group on.
        let card = Id::NULL.with(("ask-card", GROUPS[0].id));
        assert_eq!(h.click(card.with("switch-hit")), ["changed"]);
        assert_eq!(h.s.ask_groups_on, GROUPS[0].id);
        assert_eq!(h.click(Id::NULL.with(("ask-rule", "push"))), ["changed"]);
        assert_eq!(h.s.ask_off, "push");
        // D1: Also apply to Codex can't be switched here.
        assert!(h.click(Id::NULL.with("sec-codex")).is_empty());
        assert!(!h.s.also_for_codex);
        // The header closes the card: its rules are gone.
        assert!(h.click(card).is_empty());
        h.frame(vec![]);
        assert!(h.ctx.read_response(Id::NULL.with(("ask-rule", "push"))).is_none());
    }
}
