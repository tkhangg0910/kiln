//! The setup window's words and states, from what the agent detector found and the "Use" ticks.
//! No UI, so the tests can drive it.

use std::sync::LazyLock;

use regex::Regex;

use crate::agent::{AgentInfo, AgentKind};
use crate::settings::{KilnSettings, NewSessionAgent};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentRowState {
    Looking,
    Ready,
    SignIn,
    Missing,
    Installing,
    SigningIn,
}

/// One agent's row: its pill and the grey line under its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRowView {
    pub kind: AgentKind,
    pub state: AgentRowState,
    pub meta: String,
}

impl AgentRowView {
    pub fn new(kind: AgentKind, state: AgentRowState, meta: impl Into<String>) -> Self {
        Self {
            kind,
            state,
            meta: meta.into(),
        }
    }
}

/// Everything the setup window shows that depends on what was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupView {
    pub title: String,
    pub intro: String,
    pub claude: AgentRowView,
    pub codex: AgentRowView,
    pub show_picker: bool,
    pub options_enabled: bool,
    pub note: String,
    pub can_open: bool,
}

const LOOKING: &str = "Checking PATH and usual install folders";
const FOLLOW_ALONG: &str =
    "Follow along in the PowerShell window. Kiln checks again when it closes.";

/// While the detector runs.
pub fn checking() -> SetupView {
    SetupView {
        title: "Setting up Kiln".into(),
        intro: "Looking for Claude Code and Codex on this PC…".into(),
        claude: AgentRowView::new(AgentKind::Claude, AgentRowState::Looking, LOOKING),
        codex: AgentRowView::new(AgentKind::Codex, AgentRowState::Looking, LOOKING),
        show_picker: false,
        options_enabled: false,
        note: "Takes about a second.".into(),
        can_open: false,
    }
}

/// Install's PowerShell window is open. Nothing else can be done meanwhile; the other row stays
/// as it was.
pub fn installing(found: &SetupView, kind: AgentKind) -> SetupView {
    let meta = match kind {
        AgentKind::Claude => "Running the official installer…",
        AgentKind::Codex => "Running npm install -g @openai/codex…",
    };
    SetupView {
        title: format!("Installing {}", kind.name()),
        intro: FOLLOW_ALONG.into(),
        note: "Then sign in once, and Kiln is ready.".into(),
        ..waiting(
            found,
            AgentRowView::new(kind, AgentRowState::Installing, meta),
        )
    }
}

/// The same while Sign in's window is open.
pub fn signing_in(found: &SetupView, kind: AgentKind) -> SetupView {
    SetupView {
        title: format!("Signing in to {}", kind.name()),
        intro: FOLLOW_ALONG.into(),
        note: "Your browser may open to sign in.".into(),
        ..waiting(
            found,
            AgentRowView::new(
                kind,
                AgentRowState::SigningIn,
                "Finish in the PowerShell window",
            ),
        )
    }
}

fn waiting(found: &SetupView, row: AgentRowView) -> SetupView {
    let mut v = found.clone();
    match row.kind {
        AgentKind::Claude => v.claude = row,
        AgentKind::Codex => v.codex = row,
    }
    v.show_picker = false;
    v.options_enabled = false;
    v.can_open = false;
    v
}

pub fn state_of(a: &AgentInfo) -> AgentRowState {
    match (&a.exe, a.signed_in) {
        (None, _) => AgentRowState::Missing,
        (Some(_), true) => AgentRowState::Ready,
        (Some(_), false) => AgentRowState::SignIn,
    }
}

/// What the window says once the detector is done.
pub fn from(claude: &AgentInfo, codex: &AgentInfo, use_claude: bool, use_codex: bool) -> SetupView {
    use AgentRowState::{Ready, SignIn};
    let cs = state_of(claude);
    let xs = state_of(codex);
    let any_ready = cs == Ready || xs == Ready;
    let can_open = (cs == Ready && use_claude) || (xs == Ready && use_codex);

    let (title, intro, note) = if cs == Ready && xs == Ready {
        (
            "Kiln is ready".to_string(),
            "Found both agents. Their sessions share one sidebar.".to_string(),
            "Change any of this later in Settings.".to_string(),
        )
    } else if any_ready {
        let (ready, other, other_state) = if cs == Ready {
            (claude, codex, xs)
        } else {
            (codex, claude, cs)
        };
        let other_name = other.kind.name();
        if other_state == SignIn {
            (
                "Kiln is ready".to_string(),
                format!("{other_name} is installed but not signed in. Kiln can start without it."),
                format!("Sign in to {other_name} any time, then Check again."),
            )
        } else {
            (
                "Kiln is ready".to_string(),
                format!(
                    "{} is all Kiln needs. {other_name} is optional.",
                    ready.kind.name()
                ),
                format!("Add {other_name} any time from Settings."),
            )
        }
    } else if cs == SignIn || xs == SignIn {
        let intro = if cs == xs {
            "Both agents are installed but not signed in.".to_string()
        } else {
            let kind = if cs == SignIn {
                AgentKind::Claude
            } else {
                AgentKind::Codex
            };
            format!("{} is installed but not signed in.", kind.name())
        };
        (
            "Almost there".to_string(),
            intro,
            "Sign in, then Check again.".to_string(),
        )
    } else {
        (
            "Install one agent to start".to_string(),
            "Kiln runs your coding agent in its panes. Either one works.".to_string(),
            "Install installs it for you, in a window you can watch.".to_string(),
        )
    };

    SetupView {
        title,
        intro,
        claude: row(claude, cs, any_ready),
        codex: row(codex, xs, any_ready),
        show_picker: cs == Ready && xs == Ready,
        options_enabled: can_open,
        note,
        can_open,
    }
}

fn row(a: &AgentInfo, state: AgentRowState, any_ready: bool) -> AgentRowView {
    let version = short_version(a.version.as_deref());
    let meta = match state {
        AgentRowState::Ready => format!("{version} · {}", count(a.sessions, a.projects)),
        AgentRowState::SignIn => format!("{version} · Sign in once, then Kiln can use it"),
        // Missing: optional when the other one works, else who makes it. Codex installs through npm.
        _ if a.kind == AgentKind::Claude => {
            (if any_ready { "Optional." } else { "Anthropic" }).to_string()
        }
        _ => match (any_ready, a.npm.is_some()) {
            (true, false) => "Optional. Node.js not found.",
            (true, true) => "Optional. Needs Node.js.",
            (false, false) => "OpenAI · Node.js not found",
            (false, true) => "OpenAI · needs Node.js",
        }
        .to_string(),
    };
    AgentRowView::new(a.kind, state, meta)
}

static VERSION_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+(\.\d+)+").expect("version regex"));

/// "2.1.288 (Claude Code)" and "codex-cli 0.160.0" both become "v2.1.288" style; a version with
/// no number stays as it is, and none is "".
pub fn short_version(version: Option<&str>) -> String {
    match version {
        None => String::new(),
        Some(v) => match VERSION_NUMBER.find(v) {
            Some(m) => format!("v{}", m.as_str()),
            None => v.to_string(),
        },
    }
}

// ------------------------------------------------------------------ the window around the view

/// The mascot's move for a view (SetupWindow.Show): peeking while it looks, walking while an
/// install or sign-in runs, a jump once Kiln can open, else at ease.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SetupMove {
    Peek,
    Walk,
    Jump,
    Idle,
}

pub fn mascot_move(view: &SetupView) -> SetupMove {
    let busy = |s: AgentRowState| matches!(s, AgentRowState::Installing | AgentRowState::SigningIn);
    if view.claude.state == AgentRowState::Looking {
        SetupMove::Peek
    } else if busy(view.claude.state) || busy(view.codex.state) {
        SetupMove::Walk
    } else if view.can_open {
        SetupMove::Jump
    } else {
        SetupMove::Idle
    }
}

/// The pill's text and colour (0xRRGGBB); its background is the colour at alpha 0x1F.
pub fn pill(state: AgentRowState) -> (&'static str, u32) {
    match state {
        AgentRowState::Looking => ("Looking…", 0xD4A24C),
        AgentRowState::Ready => ("Ready", 0x6BBF8A),
        AgentRowState::SignIn => ("Not signed in", 0xD4A24C),
        AgentRowState::Installing => ("Installing…", 0xD4A24C),
        AgentRowState::SigningIn => ("Signing in…", 0xD4A24C),
        AgentRowState::Missing => ("Not installed", 0x8A929F),
    }
}

/// The 36x36 badge: "CC" / "CX", in its agent's colours only once the agent is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Badge {
    pub text: &'static str,
    pub bg: u32,
    pub border: u32,
    pub fg: u32,
}

pub fn badge(kind: AgentKind, state: AgentRowState) -> Badge {
    let claude = kind == AgentKind::Claude;
    let lit = matches!(state, AgentRowState::Ready | AgentRowState::SignIn | AgentRowState::SigningIn);
    let text = if claude { "CC" } else { "CX" };
    match (lit, claude) {
        (false, _) => Badge { text, bg: 0x1B1F26, border: 0x2F3641, fg: 0x8A929F },
        (true, true) => Badge { text, bg: 0x2A221F, border: 0x4A3329, fg: 0xE39A7E },
        (true, false) => Badge { text, bg: 0x1C2530, border: 0x2B3D52, fg: 0x7DBBEB },
    }
}

/// What a row shows besides its texts (AgentRow.Show).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowParts {
    /// The Use tick: only for a ready agent.
    pub use_tick: bool,
    /// The amber Sign in button: only for a signed-out one.
    pub sign_in: bool,
    /// The fix line (command, Install, Locate…): only for a missing one.
    pub fix: bool,
    /// Codex with no npm gets "Get Node.js first" in place of Install.
    pub get_node: bool,
}

pub fn row_parts(row: &AgentRowView, found: Option<&AgentInfo>) -> RowParts {
    RowParts {
        use_tick: row.state == AgentRowState::Ready,
        sign_in: row.state == AgentRowState::SignIn,
        fix: row.state == AgentRowState::Missing,
        get_node: row.kind == AgentKind::Codex && found.is_some_and(|f| f.npm.is_none()),
    }
}

/// The install command the fix line shows (what Install runs, spelled the short way).
pub fn install_shown(kind: AgentKind) -> &'static str {
    match kind {
        AgentKind::Claude => "irm https://claude.ai/install.ps1 | iex",
        AgentKind::Codex => "npm install -g @openai/codex",
    }
}

pub const NODE_URL: &str = "https://nodejs.org";

/// Added to an option a dev build may not write (the Run key, the user PATH).
pub const INSTALLED_ONLY: &str = " Only the installed Kiln can change this.";

pub const LOGIN_TITLE: &str = "Start Kiln when I sign in";
pub const LOGIN_TEXT: &str = "Starts hidden; the island is ready at the top of the screen.";
pub const PATH_TITLE: &str = "Add kiln to my PATH";

/// Locate…'s file dialog title.
pub fn locate_title(kind: AgentKind) -> String {
    format!("Where is {}?", kind.name())
}

/// The note when the picked program did not answer `--version`.
pub fn locate_failed(kind: AgentKind) -> String {
    let file = match kind {
        AgentKind::Claude => "claude.exe",
        AgentKind::Codex => "codex.cmd",
    };
    format!("That program didn't run. Pick {}'s {file}.", kind.name())
}

/// The PATH tick starts ticked until setup was done once (or when it can't be written); after
/// that it says whether the folder is on PATH now.
pub fn path_tick(setup_done: bool, can_write: bool, on_path: impl FnOnce() -> bool) -> bool {
    if setup_done || !can_write {
        on_path()
    } else {
        true
    }
}

/// Open Kiln's "New sessions use": the picker's choice when it shows, else the one agent
/// there is: Claude if it is ready and used, else Codex.
pub fn default_agent_on_open(
    picker_shown: bool,
    picked: NewSessionAgent,
    claude: Option<&AgentInfo>,
    use_claude: bool,
) -> NewSessionAgent {
    if picker_shown {
        return picked;
    }
    if claude.is_some_and(|c| state_of(c) == AgentRowState::Ready) && use_claude {
        NewSessionAgent::Claude
    } else {
        NewSessionAgent::Codex
    }
}

/// NeedsSetup (docs/SETUP.md D6): never set up, or none of the agents in use is where it was.
/// `locate` only finds, it doesn't run anything.
pub fn needs_setup(
    settings: &KilnSettings,
    locate: impl Fn(AgentKind, Option<&str>) -> Option<String>,
) -> bool {
    !settings.setup_done
        || !(settings.use_claude && locate(AgentKind::Claude, settings.claude_path.as_deref()).is_some()
            || settings.use_codex && locate(AgentKind::Codex, settings.codex_path.as_deref()).is_some())
}

/// What Open Kiln hands back: the settings with the choices made here.
pub fn done_settings(
    settings: &KilnSettings,
    use_claude: bool,
    use_codex: bool,
    default_agent: NewSessionAgent,
) -> KilnSettings {
    KilnSettings {
        setup_done: true,
        use_claude,
        use_codex,
        default_agent,
        ..settings.clone()
    }
}

fn count(sessions: u32, projects: u32) -> String {
    if sessions == 0 {
        return "No sessions yet".into();
    }
    let s = if sessions == 1 { "session" } else { "sessions" };
    let p = if projects == 1 { "project" } else { "projects" };
    format!("{sessions} {s} in {projects} {p}")
}
