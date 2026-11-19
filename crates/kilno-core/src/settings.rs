//! What you chose in Settings, kept in `%LOCALAPPDATA%\Kilno\settings.json`. Start at login is
//! not here: it lives in the Run key (`system::login_startup`). Until `setup_done`, setup shows
//! before the window.
//!
//! The file is read key by key, so one odd value costs only that setting, and written by hand in
//! the same bytes the C# build wrote (keys in order, 2-space indent, CRLF, System.Text.Json's
//! escaping), so switching builds never churns the file.

use crate::agent::AgentKind;
use crate::paths::DataDir;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// How the status strip and footer draw their numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StatusTheme {
    #[default]
    Numbers,
    Bars,
    Rings,
    Chips,
}

impl StatusTheme {
    pub const ALL: [StatusTheme; 4] = [Self::Numbers, Self::Bars, Self::Rings, Self::Chips];

    /// The name as Settings shows it ("Numbers").
    pub fn name(self) -> &'static str {
        match self {
            Self::Numbers => "Numbers",
            Self::Bars => "Bars",
            Self::Rings => "Rings",
            Self::Chips => "Chips",
        }
    }
}

/// Which agent New session starts when both are in use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NewSessionAgent {
    #[default]
    Claude,
    Codex,
    OpenCode,
    Ask,
}

impl NewSessionAgent {
    pub const ALL: [NewSessionAgent; 4] = [Self::Claude, Self::Codex, Self::OpenCode, Self::Ask];

    pub fn name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::Ask => "Ask",
        }
    }
}

/// Every setting, with the C# record's defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KilnoSettings {
    pub status_header: bool,
    pub status_footer: bool,
    pub status_model: bool,
    pub status_branch: bool,
    pub status_context: bool,
    pub status_effort: bool,
    pub status_five_hour: bool,
    pub status_seven_day: bool,
    /// The weekly limits some models have of their own (Fable), shown with the account's.
    pub status_model_weekly: bool,
    /// One of [`RESERVE_STEPS`]; 0 is off.
    pub reserve_at: i32,
    pub reserve_five_hour: bool,
    pub reserve_seven_day: bool,
    pub reserve_model_weekly: bool,
    /// Ask Anthropic's usage endpoint for the limits with the Claude sign-in (off unless the
    /// user turns it on): the numbers `/usage` shows, not the status line's older ones.
    pub live_usage: bool,
    /// How often, in minutes, while it is on: one of [`LIVE_USAGE_STEPS`].
    pub live_usage_minutes: i32,
    pub theme: StatusTheme,
    pub setup_done: bool,
    pub use_claude: bool,
    pub use_codex: bool,
    /// OpenCode, the third agent: off until setup or Settings turns it on.
    pub use_opencode: bool,
    pub default_agent: NewSessionAgent,
    pub claude_path: Option<String>,
    pub codex_path: Option<String>,
    pub opencode_path: Option<String>,
    /// A picture drawn faintly over the whole window (Settings › General); None draws nothing.
    pub background_image: Option<String>,
    /// How strongly, in percent: one of BACKGROUND_STEPS.
    pub background_opacity: i32,
    pub screenshot_hotkey: bool,
    pub screenshot_offer: bool,
    pub update_check: bool,
    /// Ask-before groups switched on, as a sorted comma string: a group added later arrives off.
    pub ask_groups_on: String,
    /// Ask-before rules unticked, as a sorted comma string: a rule added later arrives ticked.
    pub ask_off: String,
    /// Kept so the setting survives, but this build never writes Codex's rules (PORTING.md D1).
    pub also_for_codex: bool,
}

impl Default for KilnoSettings {
    fn default() -> Self {
        Self {
            status_header: true,
            status_footer: true,
            status_model: true,
            status_branch: true,
            status_context: true,
            status_effort: true,
            status_five_hour: true,
            status_seven_day: true,
            status_model_weekly: true,
            reserve_at: 0,
            reserve_five_hour: true,
            reserve_seven_day: true,
            reserve_model_weekly: true,
            live_usage: false,
            live_usage_minutes: 2,
            theme: StatusTheme::Numbers,
            setup_done: false,
            use_claude: true,
            use_codex: true,
            use_opencode: false,
            default_agent: NewSessionAgent::Claude,
            claude_path: None,
            codex_path: None,
            opencode_path: None,
            background_image: None,
            background_opacity: 15,
            screenshot_hotkey: true,
            screenshot_offer: false,
            update_check: true,
            ask_groups_on: String::new(),
            ask_off: String::new(),
            also_for_codex: false,
        }
    }
}

/// The reserve thresholds Settings offers, in percent. 0 is off.
pub const RESERVE_STEPS: [i32; 4] = [0, 70, 80, 90];

/// The live usage intervals offered, in minutes.
pub const LIVE_USAGE_STEPS: [i32; 3] = [1, 2, 5];

/// The background picture's strengths offered, in percent.
pub const BACKGROUND_STEPS: [i32; 5] = [5, 10, 15, 20, 30];

/// `<data dir>\settings.json`.
pub fn default_path(data: &DataDir) -> PathBuf {
    data.join("settings.json")
}

impl KilnoSettings {
    pub fn is_group_off(&self, id: &str) -> bool {
        !self.ask_groups_on.split(',').any(|g| g == id)
    }

    pub fn is_ask_off(&self, id: &str) -> bool {
        self.ask_off.split(',').any(|r| r == id)
    }

    /// Who starts a New session; None asks. Never an agent whose Use is off, so with one agent
    /// in use it never asks.
    pub fn new_session_kind(&self) -> Option<AgentKind> {
        let used = self.used_agents();
        if used.len() <= 1 {
            // None in use reads as Claude, as before Use ticks existed.
            return Some(used.first().copied().unwrap_or(AgentKind::Claude));
        }
        let pick = match self.default_agent {
            NewSessionAgent::Claude => Some(AgentKind::Claude),
            NewSessionAgent::Codex => Some(AgentKind::Codex),
            NewSessionAgent::OpenCode => Some(AgentKind::OpenCode),
            NewSessionAgent::Ask => None,
        };
        // The default switched off: ask between the ones that are on.
        pick.filter(|k| used.contains(k))
    }

    /// The agents whose Use is on, in sidebar order.
    pub fn used_agents(&self) -> Vec<AgentKind> {
        [(AgentKind::Claude, self.use_claude), (AgentKind::Codex, self.use_codex), (AgentKind::OpenCode, self.use_opencode)]
            .into_iter()
            .filter_map(|(k, on)| on.then_some(k))
            .collect()
    }

    /// Whether `kind` is in use.
    pub fn uses(&self, kind: AgentKind) -> bool {
        match kind {
            AgentKind::Claude => self.use_claude,
            AgentKind::Codex => self.use_codex,
            AgentKind::OpenCode => self.use_opencode,
        }
    }

    /// The binary picked for `kind` with Locate…, if any.
    pub fn agent_path(&self, kind: AgentKind) -> Option<&str> {
        match kind {
            AgentKind::Claude => self.claude_path.as_deref(),
            AgentKind::Codex => self.codex_path.as_deref(),
            AgentKind::OpenCode => self.opencode_path.as_deref(),
        }
    }

    /// A missing, unreadable or broken file is the defaults: settings are never a reason for
    /// Kilno not to start.
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        let text = String::from_utf8_lossy(&bytes);
        match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
            Ok(Value::Object(root)) => Self::from_map(&root),
            _ => Self::default(),
        }
    }

    /// Key by key; a key of the wrong type falls back to its default, unknown keys are ignored.
    pub fn from_map(root: &Map<String, Value>) -> Self {
        let d = Self::default();
        let b = |name: &str, fallback: bool| {
            root.get(name).and_then(Value::as_bool).unwrap_or(fallback)
        };
        let s = |name: &str| match root.get(name) {
            Some(Value::String(t)) if !t.is_empty() => Some(t.clone()),
            _ => None,
        };
        // A list of ids, kept as a sorted comma string; anything that isn't a plain id is skipped.
        let l = |name: &str, fallback: &str| match root.get(name) {
            Some(Value::Array(items)) => {
                let mut ids: Vec<&str> = items
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|id| !id.is_empty() && !id.contains(','))
                    .collect();
                ids.sort_unstable();
                ids.dedup();
                ids.join(",")
            }
            _ => fallback.to_string(),
        };
        // By name only, ignoring case: a number is not a theme.
        let theme = s("theme")
            .and_then(|t| {
                StatusTheme::ALL
                    .into_iter()
                    .find(|v| v.name().eq_ignore_ascii_case(&t))
            })
            .unwrap_or(d.theme);
        let default_agent = s("defaultAgent")
            .and_then(|t| {
                NewSessionAgent::ALL
                    .into_iter()
                    .find(|v| v.name().eq_ignore_ascii_case(&t))
            })
            .unwrap_or(d.default_agent);
        let live_usage_minutes = root
            .get("liveUsageMinutes")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .filter(|n| LIVE_USAGE_STEPS.contains(n))
            .unwrap_or(d.live_usage_minutes);
        let background_opacity = root
            .get("backgroundOpacity")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .filter(|n| BACKGROUND_STEPS.contains(n))
            .unwrap_or(d.background_opacity);
        let reserve_at = root
            .get("reserveAt")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .filter(|n| RESERVE_STEPS.contains(n))
            .unwrap_or(d.reserve_at);

        Self {
            status_header: b("statusHeader", d.status_header),
            status_footer: b("statusFooter", d.status_footer),
            status_model: b("statusModel", d.status_model),
            status_branch: b("statusBranch", d.status_branch),
            status_context: b("statusContext", d.status_context),
            status_effort: b("statusEffort", d.status_effort),
            status_five_hour: b("statusFiveHour", d.status_five_hour),
            status_seven_day: b("statusSevenDay", d.status_seven_day),
            status_model_weekly: b("statusModelWeekly", d.status_model_weekly),
            reserve_at,
            reserve_five_hour: b("reserveFiveHour", d.reserve_five_hour),
            reserve_seven_day: b("reserveSevenDay", d.reserve_seven_day),
            reserve_model_weekly: b("reserveModelWeekly", d.reserve_model_weekly),
            live_usage: b("liveUsage", d.live_usage),
            live_usage_minutes,
            theme,
            setup_done: b("setupDone", d.setup_done),
            use_claude: b("useClaude", d.use_claude),
            use_codex: b("useCodex", d.use_codex),
            use_opencode: b("useOpenCode", d.use_opencode),
            default_agent,
            claude_path: s("claudePath"),
            codex_path: s("codexPath"),
            opencode_path: s("openCodePath"),
            background_image: s("backgroundImage"),
            background_opacity,
            screenshot_hotkey: b("screenshotHotkey", d.screenshot_hotkey),
            screenshot_offer: b("screenshotOffer", d.screenshot_offer),
            update_check: b("updateCheck", d.update_check),
            ask_groups_on: l("askGroupsOn", &d.ask_groups_on),
            ask_off: l("askOff", &d.ask_off),
            also_for_codex: b("alsoForCodex", d.also_for_codex),
        }
    }

    /// The file's bytes, exactly as the C# build's Utf8JsonWriter (Indented) wrote them.
    pub fn to_json(&self) -> String {
        let mut w = Pretty::default();
        w.bool("statusHeader", self.status_header);
        w.bool("statusFooter", self.status_footer);
        w.bool("statusModel", self.status_model);
        w.bool("statusBranch", self.status_branch);
        w.bool("statusContext", self.status_context);
        w.bool("statusEffort", self.status_effort);
        w.bool("statusFiveHour", self.status_five_hour);
        w.bool("statusSevenDay", self.status_seven_day);
        // Keys Clayo never had are written only when off, so a Clayo file saves back unchanged.
        if !self.status_model_weekly {
            w.bool("statusModelWeekly", false);
        }
        w.raw("reserveAt", &self.reserve_at.to_string());
        w.bool("reserveFiveHour", self.reserve_five_hour);
        w.bool("reserveSevenDay", self.reserve_seven_day);
        if !self.reserve_model_weekly {
            w.bool("reserveModelWeekly", false);
        }
        if self.live_usage {
            w.bool("liveUsage", true);
        }
        if self.live_usage_minutes != Self::default().live_usage_minutes {
            w.raw("liveUsageMinutes", &self.live_usage_minutes.to_string());
        }
        w.str("theme", &self.theme.name().to_lowercase());
        w.bool("setupDone", self.setup_done);
        w.bool("useClaude", self.use_claude);
        w.bool("useCodex", self.use_codex);
        w.str("defaultAgent", &self.default_agent.name().to_lowercase());
        if let Some(p) = &self.claude_path {
            w.str("claudePath", p);
        }
        if let Some(p) = &self.codex_path {
            w.str("codexPath", p);
        }
        // OpenCode keys are written only when set, so a Clayo file saves back unchanged.
        if self.use_opencode {
            w.bool("useOpenCode", true);
        }
        if let Some(p) = &self.opencode_path {
            w.str("openCodePath", p);
        }
        // Kilno-only keys, written only when set, so a Clayo file saves back unchanged.
        if let Some(p) = &self.background_image {
            w.str("backgroundImage", p);
        }
        if self.background_opacity != Self::default().background_opacity {
            w.raw("backgroundOpacity", &self.background_opacity.to_string());
        }
        w.bool("screenshotHotkey", self.screenshot_hotkey);
        w.bool("screenshotOffer", self.screenshot_offer);
        w.bool("updateCheck", self.update_check);
        w.list("askGroupsOn", &self.ask_groups_on);
        w.list("askOff", &self.ask_off);
        w.bool("alsoForCodex", self.also_for_codex);
        w.finish()
    }

    /// Written beside the file then swapped in, so a crash mid-write loses nothing. Best effort:
    /// errors are swallowed.
    pub fn save(&self, path: &Path) {
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let result = (|| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&tmp, self.to_json())?;
            std::fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Utf8JsonWriter's indented layout for a flat object: CRLF, two spaces, `"key": value`, empty
/// arrays as `[]`, no trailing newline.
#[derive(Default)]
struct Pretty {
    out: String,
}

impl Pretty {
    fn key(&mut self, name: &str) {
        self.out.push_str(if self.out.is_empty() {
            "{\r\n  "
        } else {
            ",\r\n  "
        });
        push_json_string(&mut self.out, name);
        self.out.push_str(": ");
    }
    fn raw(&mut self, name: &str, value: &str) {
        self.key(name);
        self.out.push_str(value);
    }
    fn bool(&mut self, name: &str, value: bool) {
        self.raw(name, if value { "true" } else { "false" });
    }
    fn str(&mut self, name: &str, value: &str) {
        self.key(name);
        push_json_string(&mut self.out, value);
    }
    fn list(&mut self, name: &str, comma: &str) {
        self.key(name);
        let ids: Vec<&str> = comma.split(',').filter(|s| !s.is_empty()).collect();
        if ids.is_empty() {
            self.out.push_str("[]");
            return;
        }
        self.out.push('[');
        for (i, id) in ids.iter().enumerate() {
            self.out
                .push_str(if i == 0 { "\r\n    " } else { ",\r\n    " });
            push_json_string(&mut self.out, id);
        }
        self.out.push_str("\r\n  ]");
    }
    fn finish(mut self) -> String {
        self.out.push_str("\r\n}");
        self.out
    }
}

/// A JSON string as System.Text.Json's default encoder writes it: ASCII only, with `"`, `'`,
/// `&`, `+`, `<`, `>` and `` ` `` (HTML-sensitive) and control characters as `\uXXXX`.
pub(crate) fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' if !matches!(c, '"' | '\'' | '&' | '+' | '<' | '>' | '`') => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
        }
    }
    out.push('"');
}
