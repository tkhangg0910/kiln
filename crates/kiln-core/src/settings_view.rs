//! The words the Settings page shows for a given [`KilnSettings`] (SettingsPane.xaml.cs), so
//! the egui page only draws them.

use crate::security::ask_rules::{AskGroup, AskRule, GROUPS};
use crate::settings::KilnSettings;
use crate::system::updater::Version;

/// "On" / "Off" beside every switch.
pub fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

/// General › Start at login's line; a build that can't write the Run key says so.
pub fn login_text(can_write: bool) -> &'static str {
    if can_write {
        "Kiln starts hidden; the island greets you."
    } else {
        "Kiln starts hidden; the island greets you. Only the installed Kiln can change this."
    }
}

/// General › Updates: what sits under "Kiln x.y.z" with a newer release found (None: the
/// What's new link instead), and the button's label.
pub fn newer_text(newer: Version, waiting: bool) -> (String, &'static str) {
    if waiting {
        (format!("Kiln {newer} installs as soon as no session is working."), "Call off")
    } else {
        (format!("Kiln {newer} is out."), "Update")
    }
}

/// Island › the Ctrl+Alt+S switch's line.
pub fn shot_key_text(hotkey_on: bool, taken: bool) -> &'static str {
    if hotkey_on && taken {
        "Another app has Ctrl+Alt+S, so the shortcut is off. Close that app's shortcut, then switch this off and on."
    } else {
        "Take a screenshot (Win+Shift+S), then press Ctrl+Alt+S: the island asks which session it is for."
    }
}

/// Status bar › Fields: why the per-session ticks are greyed ("" when they aren't).
pub fn why_head(s: &KilnSettings) -> &'static str {
    if s.status_header {
        ""
    } else {
        "Pane header is off"
    }
}

/// Status bar › Fields: the account ticks' note.
pub fn why_foot(s: &KilnSettings) -> &'static str {
    match (s.status_header, s.status_footer) {
        (false, false) => "Both placements are off",
        (true, false) => "Shown in the pane header while the footer is off",
        _ => "",
    }
}

/// A comma list of ids, as `askOff` and `askGroupsOn` keep them, with `id` in it or not;
/// ordinal sorted (SettingsPane.Toggled).
pub fn toggled(list: &str, id: &str, listed: bool) -> String {
    let mut ids: Vec<&str> = list.split(',').filter(|x| !x.is_empty() && *x != id).collect();
    if listed {
        ids.push(id);
    }
    ids.sort_unstable();
    ids.join(",")
}

/// A card's state beside its switch: "Off", "On", or "On · 3 of 8".
pub fn group_state(s: &KilnSettings, g: &AskGroup) -> String {
    if s.is_group_off(g.id) {
        return "Off".into();
    }
    let ticked = g.rules.iter().filter(|r| !s.is_ask_off(r.id)).count();
    if ticked < g.rules.len() {
        format!("On · {ticked} of {}", g.rules.len())
    } else {
        "On".into()
    }
}

/// One rule row as shown: its line, how strong each chip is, and the chips' tooltips.
#[derive(Debug, PartialEq)]
pub struct RuleView {
    pub line: String,
    pub claude_active: bool,
    pub codex_active: bool,
    pub claude_tip: &'static str,
    pub codex_tip: &'static str,
}

pub fn rule_view(s: &KilnSettings, g: &AskGroup, r: &AskRule) -> RuleView {
    let on = !s.is_group_off(g.id);
    let yes = !s.is_ask_off(r.id);
    let codex_gets = yes && s.also_for_codex;
    let codex_only = s.use_codex && !s.use_claude;
    let line = if r.id != "api" {
        r.line.to_string()
    } else if codex_only {
        "every gh api call".to_string()
    } else if codex_gets && s.use_codex {
        format!("{} · Codex: every gh api call", r.line)
    } else {
        r.line.to_string()
    };
    RuleView {
        line,
        claude_active: on && yes,
        codex_active: on && codex_gets,
        claude_tip: if yes { "Claude: asks first" } else { "Claude: runs without asking" },
        codex_tip: if !yes {
            "Codex: runs without asking"
        } else if !codex_gets {
            "Codex: Also apply to Codex is off"
        } else if r.id == "api" {
            "Codex: asks before every gh api call"
        } else {
            "Codex: asks first"
        },
    }
}

/// The line under the cards.
pub fn ask_foot(s: &KilnSettings) -> &'static str {
    if GROUPS.iter().all(|g| s.is_group_off(g.id)) {
        "Off. Only your own rules apply."
    } else if s.use_codex && !s.use_claude && !s.also_for_codex {
        "Codex asks once Also apply to Codex is on. Your own rules still apply."
    } else {
        "Applies to sessions you start from now on. Your own rules still apply."
    }
}

/// Your own rules' closed header: "Claude 12 · Codex 3", or "None". `agents` are the shown
/// agents' names ("Claude Code", "Codex") with their rule counts.
pub fn own_counts(agents: &[(&str, usize)]) -> String {
    if agents.iter().all(|(_, n)| *n == 0) {
        return "None".into();
    }
    agents
        .iter()
        .map(|(name, n)| format!("{} {n}", name.split(' ').next().unwrap_or(name)))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A decision pill's text and fill colours (RGB, ARGB): allow green, deny and forbidden red,
/// anything else (ask, prompt) blue.
pub fn decision_colors(decision: &str) -> (u32, u32) {
    match decision {
        "allow" => (0x9FD3B1, 0x246BBF8A),
        "deny" | "forbidden" => (0xEC9C99, 0x29D2605F),
        _ => (0x9CCBEF, 0x295AA9E6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group() -> &'static AskGroup {
        &GROUPS[0]
    }

    fn rule(id: &str) -> &'static AskRule {
        group().rules.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn toggled_keeps_a_sorted_distinct_list() {
        assert_eq!(toggled("", "push", true), "push");
        assert_eq!(toggled("tags,push", "api", true), "api,push,tags");
        assert_eq!(toggled("api,push", "push", false), "api");
        assert_eq!(toggled("push", "push", true), "push");
    }

    #[test]
    fn group_state_counts_ticked_rules() {
        let mut s = KilnSettings::default();
        assert_eq!(group_state(&s, group()), "Off");
        s.ask_groups_on = "publishing".into();
        assert_eq!(group_state(&s, group()), "On");
        s.ask_off = "api,push".into();
        assert_eq!(group_state(&s, group()), format!("On · {} of {}", group().rules.len() - 2, group().rules.len()));
    }

    #[test]
    fn field_notes() {
        let mut s = KilnSettings::default();
        assert_eq!((why_head(&s), why_foot(&s)), ("", ""));
        s.status_footer = false;
        assert_eq!(why_foot(&s), "Shown in the pane header while the footer is off");
        s.status_header = false;
        assert_eq!((why_head(&s), why_foot(&s)), ("Pane header is off", "Both placements are off"));
        s.status_footer = true;
        assert_eq!(why_foot(&s), "");
    }

    #[test]
    fn api_line_and_tips() {
        let mut s = KilnSettings { ask_groups_on: "publishing".into(), use_codex: true, ..Default::default() };
        let v = rule_view(&s, group(), rule("api"));
        assert_eq!(v.line, "gh api calls that write");
        assert_eq!(v.codex_tip, "Codex: Also apply to Codex is off");
        assert!(v.claude_active && !v.codex_active);
        s.also_for_codex = true;
        let v = rule_view(&s, group(), rule("api"));
        assert_eq!(v.line, "gh api calls that write · Codex: every gh api call");
        assert_eq!(v.codex_tip, "Codex: asks before every gh api call");
        s.use_claude = false;
        assert_eq!(rule_view(&s, group(), rule("api")).line, "every gh api call");
        s.ask_off = "push".into();
        let v = rule_view(&s, group(), rule("push"));
        assert_eq!((v.claude_tip, v.codex_tip), ("Claude: runs without asking", "Codex: runs without asking"));
        s.ask_groups_on = String::new();
        assert!(!rule_view(&s, group(), rule("tags")).claude_active);
    }

    #[test]
    fn foot_texts() {
        let mut s = KilnSettings::default();
        assert_eq!(ask_foot(&s), "Off. Only your own rules apply.");
        s.ask_groups_on = "publishing".into();
        assert_eq!(ask_foot(&s), "Applies to sessions you start from now on. Your own rules still apply.");
        s.use_claude = false;
        s.use_codex = true;
        assert_eq!(ask_foot(&s), "Codex asks once Also apply to Codex is on. Your own rules still apply.");
    }

    #[test]
    fn counts_and_texts() {
        assert_eq!(own_counts(&[("Claude Code", 0), ("Codex", 0)]), "None");
        assert_eq!(own_counts(&[("Claude Code", 12), ("Codex", 3)]), "Claude 12 · Codex 3");
        assert_eq!(on_off(true), "On");
        assert!(login_text(false).ends_with("Only the installed Kiln can change this."));
        let v = Version::new(1, 2, 3);
        assert_eq!(newer_text(v, false), ("Kiln 1.2.3 is out.".into(), "Update"));
        assert_eq!(newer_text(v, true).1, "Call off");
        assert!(shot_key_text(true, true).starts_with("Another app has Ctrl+Alt+S"));
        assert!(shot_key_text(false, true).starts_with("Take a screenshot"));
        assert_eq!(decision_colors("forbidden").0, 0xEC9C99);
    }
}
