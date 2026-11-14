//! Ask before publishing: the rules Kilno hands the Claude sessions it starts so they stop and
//! ask before they push, tag, release, merge or change the repo on GitHub.
//!
//! The C# build also wrote `<CODEX_HOME>\rules\kilno.rules` for Codex. This build never writes
//! or deletes anything under the Codex home (PORTING.md D1): [`codex_rules`] stays as the text
//! that file held, and [`codex_rules_file_still_active`] tells Settings when one left behind by
//! the C# build is still in force.

use crate::paths::ReadOnlyDir;
use crate::settings::KilnoSettings;
use std::path::PathBuf;

/// One row of Settings › Security: Claude's ask patterns and Codex's command prefixes.
#[derive(Debug, PartialEq, Eq)]
pub struct AskRule {
    pub id: &'static str,
    pub label: &'static str,
    pub line: &'static str,
    pub claude: &'static [&'static str],
    pub codex: &'static [&'static [&'static str]],
}

/// One kind of action, a card on Settings › Security. `icon` is the card's Segoe Fluent glyph.
#[derive(Debug, PartialEq, Eq)]
pub struct AskGroup {
    pub id: &'static str,
    pub title: &'static str,
    pub line: &'static str,
    pub icon: &'static str,
    pub rules: &'static [AskRule],
}

pub static GROUPS: &[AskGroup] = &[AskGroup {
    id: "publishing",
    title: "Publishing",
    line: "Pushing, tags, releases, anything that changes GitHub.",
    icon: "\u{EA18}",
    rules: &[
        AskRule {
            id: "push",
            label: "Push code",
            line: "git push",
            claude: &["git push*", "git -C * push*"],
            codex: &[&["git", "push"]],
        },
        AskRule {
            id: "tags",
            label: "Tags",
            line: "git tag: making or moving a version tag",
            claude: &["git tag*", "git -C * tag*"],
            codex: &[&["git", "tag"]],
        },
        AskRule {
            id: "releases",
            label: "Releases",
            line: "gh release: drafting or publishing one",
            claude: &["gh release*"],
            codex: &[&["gh", "release"]],
        },
        AskRule {
            id: "merge",
            label: "Merge pull requests",
            line: "gh pr merge",
            claude: &["gh pr merge*"],
            codex: &[&["gh", "pr", "merge"]],
        },
        AskRule {
            id: "repo",
            label: "Repo settings",
            line: "Renaming, deleting, rulesets",
            claude: &[
                "gh repo edit*",
                "gh repo delete*",
                "gh repo rename*",
                "gh ruleset*",
            ],
            codex: &[
                &["gh", "repo", "edit"],
                &["gh", "repo", "delete"],
                &["gh", "repo", "rename"],
                &["gh", "ruleset"],
            ],
        },
        AskRule {
            id: "secrets",
            label: "Secrets",
            line: "gh secret",
            claude: &["gh secret*"],
            codex: &[&["gh", "secret"]],
        },
        AskRule {
            id: "workflows",
            label: "Workflows",
            line: "gh workflow: running or turning one on or off",
            claude: &["gh workflow*"],
            codex: &[&["gh", "workflow"]],
        },
        // Codex matches prefixes only, so it can't tell a gh api read from a write.
        AskRule {
            id: "api",
            label: "Other GitHub changes",
            line: "gh api calls that write",
            claude: &[
                "gh api *-X*",
                "gh api *--method*",
                "gh api *-f *",
                "gh api *-F *",
                "gh api *--field*",
                "gh api *--raw-field*",
                "gh api *--input*",
            ],
            codex: &[&["gh", "api"]],
        },
    ],
}];

/// Every group's rules, in order.
pub fn all() -> impl Iterator<Item = &'static AskRule> {
    GROUPS.iter().flat_map(|g| g.rules.iter())
}

/// The rules of every group that's on, minus the ones you unticked.
pub fn ticked(s: &KilnoSettings) -> impl Iterator<Item = &'static AskRule> + '_ {
    GROUPS
        .iter()
        .filter(|g| !s.is_group_off(g.id))
        .flat_map(|g| g.rules.iter())
        .filter(|r| !s.is_ask_off(r.id))
}

/// Claude's `permissions.ask`: each pattern for Bash, and for PowerShell in panes that run it.
pub fn claude_ask(s: &KilnoSettings) -> Vec<String> {
    ticked(s)
        .flat_map(|r| r.claude.iter())
        .flat_map(|p| [format!("Bash({p})"), format!("PowerShell({p})")])
        .collect()
}

/// The file name the C# build gave Codex.
pub const CODEX_FILE_NAME: &str = "kilno.rules";

/// What `kilno.rules` held in the C# build, or None when it should not exist: Codex is off for
/// this, the switch is off, or every rule is unticked. ASCII only, `\n` endings.
pub fn codex_rules(s: &KilnoSettings) -> Option<String> {
    if !s.also_for_codex {
        return None;
    }
    let lines: Vec<String> = ticked(s)
        .flat_map(|r| r.codex.iter())
        .map(|p| {
            let words: Vec<String> = p.iter().map(|w| format!("\"{w}\"")).collect();
            format!(
                "prefix_rule(pattern=[{}], decision=\"prompt\")",
                words.join(", ")
            )
        })
        .collect();
    if lines.is_empty() {
        return None;
    }
    Some(format!(
        "# Written by Kilno: Ask before publishing. Turn it off in Kilno, Settings > Security.\n\
         # Kilno rewrites this file when those settings change, and deletes it when they're off.\n\
         {}\n",
        lines.join("\n")
    ))
}

/// The `kilno.rules` a C# build left in the Codex home, when "Also apply to Codex" is on and the
/// file is still there: Settings › Security then says "Still active in Codex; delete this file
/// to turn it off" with an Open file link. Only looks; never writes or deletes.
pub fn codex_rules_file_still_active(
    codex_home: &ReadOnlyDir,
    s: &KilnoSettings,
) -> Option<PathBuf> {
    if !s.also_for_codex {
        return None;
    }
    let path = codex_home.join("rules").join(CODEX_FILE_NAME);
    path.is_file().then_some(path)
}
