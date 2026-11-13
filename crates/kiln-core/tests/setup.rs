//! Port of checks/setup.cs (the SetupScreen part; the AgentHelper commands are tested with the
//! agent helper): the setup window's states from made-up detector results.

use kiln_core::agent::{AgentInfo, AgentKind};
use kiln_core::setup::{
    checking, from, installing, short_version, signing_in, AgentRowState, AgentRowView,
};
use AgentRowState::*;

fn claude_ready() -> AgentInfo {
    AgentInfo {
        kind: AgentKind::Claude,
        exe: Some(r"C:\c.exe".into()),
        version: Some("2.1.287 (Claude Code)".into()),
        signed_in: true,
        data_folder: String::new(),
        sessions: 42,
        projects: 7,
        npm: None,
    }
}

fn codex_ready() -> AgentInfo {
    AgentInfo {
        kind: AgentKind::Codex,
        exe: Some(r"C:\x.cmd".into()),
        version: Some("codex-cli 0.160.0".into()),
        signed_in: true,
        data_folder: String::new(),
        sessions: 9,
        projects: 3,
        npm: Some(r"C:\npm.cmd".into()),
    }
}

fn claude_missing() -> AgentInfo {
    AgentInfo {
        kind: AgentKind::Claude,
        exe: None,
        version: None,
        signed_in: false,
        data_folder: String::new(),
        sessions: 0,
        projects: 0,
        npm: None,
    }
}

fn codex_missing() -> AgentInfo {
    AgentInfo {
        exe: None,
        version: None,
        signed_in: false,
        sessions: 0,
        projects: 0,
        ..codex_ready()
    }
}

fn codex_no_node() -> AgentInfo {
    AgentInfo {
        npm: None,
        ..codex_missing()
    }
}

fn codex_sign_in() -> AgentInfo {
    AgentInfo {
        signed_in: false,
        ..codex_ready()
    }
}

fn claude_sign_in() -> AgentInfo {
    AgentInfo {
        signed_in: false,
        ..claude_ready()
    }
}

#[test]
fn checking_state() {
    let v = checking();
    assert_eq!(v.title, "Setting up Kiln");
    assert_eq!(
        (v.claude.state, v.codex.state),
        (Looking, Looking),
        "both looking"
    );
    assert!(!v.can_open, "can't open");
    assert!(!v.options_enabled, "options greyed");
}

#[test]
fn both_ready() {
    let v = from(&claude_ready(), &codex_ready(), true, true);
    assert_eq!(v.title, "Kiln is ready");
    assert_eq!(
        v.intro,
        "Found both agents. Their sessions share one sidebar."
    );
    assert_eq!(v.claude.meta, "v2.1.287 · 42 sessions in 7 projects");
    assert_eq!(v.codex.meta, "v0.160.0 · 9 sessions in 3 projects");
    assert!(v.show_picker, "picker shows");
    assert!(v.can_open);
    assert_eq!(v.note, "Change any of this later in Settings.");
    assert!(
        !from(&claude_ready(), &codex_ready(), false, false).can_open,
        "both unticked"
    );
    assert!(
        from(&claude_ready(), &codex_ready(), false, true).can_open,
        "one unticked"
    );
}

#[test]
fn claude_only_and_its_mirror() {
    let v = from(&claude_ready(), &codex_missing(), true, true);
    assert_eq!(
        v.intro,
        "Claude Code is all Kiln needs. Codex is optional."
    );
    assert_eq!(v.codex.meta, "Optional. Needs Node.js.");
    assert!(!v.show_picker, "no picker");
    assert_eq!(v.note, "Add Codex any time from Settings.");
    assert_eq!(
        from(&claude_ready(), &codex_no_node(), true, true)
            .codex
            .meta,
        "Optional. Node.js not found."
    );
    assert!(
        !from(&claude_ready(), &codex_missing(), false, true).can_open,
        "claude unticked"
    );

    let v = from(&claude_missing(), &codex_ready(), true, true);
    assert_eq!(
        v.intro,
        "Codex is all Kiln needs. Claude Code is optional."
    );
    assert_eq!(v.claude.meta, "Optional.");
    assert!(v.can_open);
}

#[test]
fn sign_in() {
    let v = from(&claude_missing(), &codex_sign_in(), true, true);
    assert_eq!(v.title, "Almost there");
    assert_eq!(v.intro, "Codex is installed but not signed in.");
    assert_eq!(v.codex.state, SignIn);
    assert_eq!(
        v.codex.meta,
        "v0.160.0 · Sign in once, then Kiln can use it"
    );
    assert!(!v.can_open);
    assert_eq!(v.note, "Sign in, then Check again.");
    assert_eq!(
        from(&claude_sign_in(), &codex_sign_in(), true, true).intro,
        "Both agents are installed but not signed in."
    );
    let v = from(&claude_ready(), &codex_sign_in(), true, true);
    assert!(v.can_open, "one ready, one signed out: opens");
    assert_eq!(
        v.intro,
        "Codex is installed but not signed in. Kiln can start without it."
    );
    assert_eq!(v.note, "Sign in to Codex any time, then Check again.");
}

#[test]
fn none_installed() {
    let v = from(&claude_missing(), &codex_no_node(), true, true);
    assert_eq!(v.title, "Install one agent to start");
    assert_eq!(
        v.intro,
        "Kiln runs your coding agent in its panes. Either one works."
    );
    assert_eq!(v.claude.meta, "Anthropic");
    assert_eq!(v.codex.meta, "OpenAI · Node.js not found");
    assert_eq!(
        from(&claude_missing(), &codex_missing(), true, true)
            .codex
            .meta,
        "OpenAI · needs Node.js"
    );
    assert!(!v.can_open);
    assert!(!v.options_enabled);
    assert_eq!(
        v.note,
        "Install installs it for you, in a window you can watch."
    );
}

#[test]
fn installing_changes_only_that_row() {
    let v = installing(
        &from(&claude_missing(), &codex_missing(), true, true),
        AgentKind::Claude,
    );
    assert_eq!(v.title, "Installing Claude Code");
    assert_eq!(
        v.claude,
        AgentRowView::new(
            AgentKind::Claude,
            Installing,
            "Running the official installer…"
        )
    );
    assert_eq!(v.codex.state, Missing, "codex row as it was");
    assert!(!v.can_open);
    assert!(!v.options_enabled);
    assert_eq!(
        v.intro,
        "Follow along in the PowerShell window. Kiln checks again when it closes."
    );
    assert_eq!(v.note, "Then sign in once, and Kiln is ready.");

    let v = installing(
        &from(&claude_ready(), &codex_missing(), true, true),
        AgentKind::Codex,
    );
    assert!(
        !v.can_open,
        "beside a ready claude: still can't open meanwhile"
    );
    assert_eq!(v.codex.meta, "Running npm install -g @openai/codex…");
}

#[test]
fn signing_in_state() {
    let v = signing_in(
        &from(&claude_missing(), &codex_sign_in(), true, true),
        AgentKind::Codex,
    );
    assert_eq!(v.title, "Signing in to Codex");
    assert_eq!(v.codex.state, SigningIn);
    assert_eq!(v.codex.meta, "Finish in the PowerShell window");
    assert_eq!(v.note, "Your browser may open to sign in.");
}

#[test]
fn small_things() {
    let one = AgentInfo {
        sessions: 1,
        projects: 1,
        ..claude_ready()
    };
    assert_eq!(
        from(&one, &codex_missing(), true, true).claude.meta,
        "v2.1.287 · 1 session in 1 project"
    );
    let none = AgentInfo {
        sessions: 0,
        projects: 0,
        ..claude_ready()
    };
    assert_eq!(
        from(&none, &codex_missing(), true, true).claude.meta,
        "v2.1.287 · No sessions yet"
    );
    assert_eq!(short_version(Some("dev build")), "dev build");
    assert_eq!(short_version(None), "");
}

// ------------------------------------------------------------------ the window around the view

use kiln_core::settings::{KilnSettings, NewSessionAgent};
use kiln_core::setup::{
    badge, default_agent_on_open, done_settings, install_shown, locate_failed, locate_title,
    mascot_move, needs_setup, path_tick, pill, row_parts, SetupMove,
};

#[test]
fn mascot_moves_with_the_view() {
    assert_eq!(mascot_move(&checking()), SetupMove::Peek);
    let ready = from(&claude_ready(), &codex_missing(), true, true);
    assert_eq!(mascot_move(&ready), SetupMove::Jump);
    assert_eq!(mascot_move(&installing(&ready, AgentKind::Codex)), SetupMove::Walk);
    assert_eq!(mascot_move(&signing_in(&ready, AgentKind::Claude)), SetupMove::Walk);
    let none = from(&claude_missing(), &codex_missing(), true, true);
    assert_eq!(mascot_move(&none), SetupMove::Idle);
    // Ready but not used: Kiln can't open, so no jump.
    assert_eq!(mascot_move(&from(&claude_ready(), &codex_missing(), false, true)), SetupMove::Idle);
}

#[test]
fn pills_and_badges() {
    assert_eq!(pill(Looking), ("Looking…", 0xD4A24C));
    assert_eq!(pill(Ready), ("Ready", 0x6BBF8A));
    assert_eq!(pill(SignIn), ("Not signed in", 0xD4A24C));
    assert_eq!(pill(Installing), ("Installing…", 0xD4A24C));
    assert_eq!(pill(SigningIn), ("Signing in…", 0xD4A24C));
    assert_eq!(pill(Missing), ("Not installed", 0x8A929F));

    let b = badge(AgentKind::Claude, Ready);
    assert_eq!((b.text, b.bg, b.border, b.fg), ("CC", 0x2A221F, 0x4A3329, 0xE39A7E));
    let b = badge(AgentKind::Codex, SigningIn);
    assert_eq!((b.text, b.bg, b.border, b.fg), ("CX", 0x1C2530, 0x2B3D52, 0x7DBBEB));
    for s in [Looking, Missing, Installing] {
        let b = badge(AgentKind::Codex, s);
        assert_eq!((b.bg, b.border, b.fg), (0x1B1F26, 0x2F3641, 0x8A929F), "{s:?} is unlit");
    }
}

#[test]
fn row_parts_by_state() {
    let v = from(&claude_ready(), &codex_no_node(), true, true);
    let c = row_parts(&v.claude, Some(&claude_ready()));
    assert!(c.use_tick && !c.sign_in && !c.fix && !c.get_node);
    let x = row_parts(&v.codex, Some(&codex_no_node()));
    assert!(!x.use_tick && !x.sign_in && x.fix && x.get_node);
    let x = row_parts(&v.codex, Some(&codex_missing()));
    assert!(x.fix && !x.get_node, "npm found: Install");
    let v = from(&claude_sign_in(), &codex_missing(), true, true);
    let c = row_parts(&v.claude, Some(&claude_sign_in()));
    assert!(!c.use_tick && c.sign_in && !c.fix);
    // While looking nothing is known yet.
    let l = row_parts(&checking().codex, None);
    assert!(!l.use_tick && !l.sign_in && !l.fix && !l.get_node);
    assert_eq!(install_shown(AgentKind::Claude), "irm https://claude.ai/install.ps1 | iex");
    assert_eq!(install_shown(AgentKind::Codex), "npm install -g @openai/codex");
}

#[test]
fn locate_texts() {
    assert_eq!(locate_title(AgentKind::Claude), "Where is Claude Code?");
    assert_eq!(locate_failed(AgentKind::Claude), "That program didn't run. Pick Claude Code's claude.exe.");
    assert_eq!(locate_failed(AgentKind::Codex), "That program didn't run. Pick Codex's codex.cmd.");
}

#[test]
fn path_tick_is_ticked_until_setup_was_done() {
    assert!(path_tick(false, true, || false));
    assert!(!path_tick(true, true, || false));
    assert!(path_tick(true, true, || true));
    assert!(!path_tick(false, false, || false), "a dev build shows what is there");
}

#[test]
fn default_agent_on_open_rules() {
    use NewSessionAgent::*;
    assert_eq!(default_agent_on_open(true, Ask, Some(&claude_ready()), true), Ask);
    assert_eq!(default_agent_on_open(false, Ask, Some(&claude_ready()), true), Claude);
    assert_eq!(default_agent_on_open(false, Claude, Some(&claude_ready()), false), Codex);
    assert_eq!(default_agent_on_open(false, Claude, Some(&claude_missing()), true), Codex);
    assert_eq!(default_agent_on_open(false, Claude, None, true), Codex);
}

#[test]
fn needs_setup_until_done_and_an_agent_in_use_is_there() {
    let found = |k: AgentKind, _: Option<&str>| (k == AgentKind::Codex).then(|| "codex".to_string());
    let mut s = KilnSettings { setup_done: false, ..Default::default() };
    assert!(needs_setup(&s, found));
    s.setup_done = true;
    s.use_claude = true;
    s.use_codex = false;
    assert!(needs_setup(&s, found), "Claude in use but gone");
    s.use_codex = true;
    assert!(!needs_setup(&s, found));
    s.use_claude = false;
    s.use_codex = false;
    assert!(needs_setup(&s, found), "nothing in use");
    // The saved path is what locate is asked with.
    s.use_claude = true;
    s.claude_path = Some(r"C:\c.exe".into());
    assert!(!needs_setup(&s, |k, p| (k == AgentKind::Claude && p == Some(r"C:\c.exe")).then(String::new)));
}

#[test]
fn done_keeps_everything_else() {
    let s = KilnSettings { claude_path: Some("x".into()), reserve_at: 80, ..Default::default() };
    let d = done_settings(&s, false, true, NewSessionAgent::Codex);
    assert!(d.setup_done && !d.use_claude && d.use_codex);
    assert_eq!(d.default_agent, NewSessionAgent::Codex);
    assert_eq!(d.claude_path.as_deref(), Some("x"));
    assert_eq!(d.reserve_at, 80);
}
