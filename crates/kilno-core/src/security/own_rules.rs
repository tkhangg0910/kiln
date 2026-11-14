//! The rules already in the agents' own settings, as Settings › Security lists them. Only ever
//! read, never the credential files, and a file that's missing or broken is one line, never an
//! error.

use super::ask_rules::{self, AskRule};
use crate::settings::KilnoSettings;
use regex::Regex;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One of the user's own rules: allow, ask or deny for Claude; allow, prompt or forbidden for
/// Codex. `note` says how it meets Kilno's ask rules ("" when it doesn't).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnRule {
    pub decision: String,
    pub rule: String,
    pub note: String,
}

/// An agent's own rules. `file` is what Open file opens; `problem` replaces the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnRules {
    pub file: Option<PathBuf>,
    pub rules: Vec<OwnRule>,
    pub problem: Option<String>,
}

impl OwnRules {
    fn problem(file: Option<PathBuf>, text: &str) -> Self {
        Self {
            file,
            rules: Vec::new(),
            problem: Some(text.to_string()),
        }
    }
}

/// `permissions.allow`, `ask` and `deny` from Claude's settings.json, in that order.
pub fn claude(settings_json: &Path, s: &KilnoSettings) -> OwnRules {
    if !settings_json.exists() {
        return OwnRules::problem(None, "No settings file yet.");
    }
    let root = match std::fs::read(settings_json).ok().and_then(|b| {
        serde_json::from_str::<Value>(String::from_utf8_lossy(&b).trim_start_matches('\u{feff}'))
            .ok()
    }) {
        Some(v) => v,
        None => {
            return OwnRules::problem(
                Some(settings_json.to_path_buf()),
                "Couldn't read this file.",
            )
        }
    };
    let mut rules = Vec::new();
    if let Some(p) = root.get("permissions").and_then(Value::as_object) {
        for decision in ["allow", "ask", "deny"] {
            let Some(list) = p.get(decision).and_then(Value::as_array) else {
                continue;
            };
            for rule in list
                .iter()
                .filter_map(Value::as_str)
                .filter(|r| !r.is_empty())
            {
                let note = if decision == "allow" {
                    claude_note(rule, s)
                } else {
                    String::new()
                };
                rules.push(OwnRule {
                    decision: decision.to_string(),
                    rule: rule.to_string(),
                    note,
                });
            }
        }
    }
    OwnRules {
        file: Some(settings_json.to_path_buf()),
        rules,
        problem: None,
    }
}

/// `prefix_rule` lines from every rules file in `<codex_home>\rules`, sorted by name ignoring
/// case. Unlike the C# build, a `kilno.rules` that build left behind is listed like any other
/// file (PORTING.md D1), so you can see it is still in force. Open file opens your first own
/// file, or that one when it's all there is.
pub fn codex(codex_home: &Path, s: &KilnoSettings) -> OwnRules {
    let dir = codex_home.join("rules");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_file() && has_ext(p, "rules"))
                .collect()
        })
        .unwrap_or_default();
    if files.is_empty() {
        return OwnRules::problem(None, "No rules files yet.");
    }
    files.sort_by_key(|p| file_name(p).to_uppercase());
    let open = files
        .iter()
        .find(|p| !file_name(p).eq_ignore_ascii_case(ask_rules::CODEX_FILE_NAME))
        .unwrap_or(&files[0])
        .clone();

    let mut rules = Vec::new();
    for file in &files {
        let Ok(bytes) = std::fs::read(file) else {
            return OwnRules::problem(Some(open), "Couldn't read these files.");
        };
        let text = String::from_utf8_lossy(&bytes);
        for m in re().prefix_rule.captures_iter(&text) {
            let inner = &m[1];
            let Some(pattern) = re().pattern.captures(inner) else {
                continue;
            };
            // Codex's own default when a rule names no decision.
            let decision = re()
                .decision
                .captures(inner)
                .map_or("allow".to_string(), |d| d[1].to_string());
            let words: Vec<&str> = re()
                .word
                .captures_iter(&pattern[1])
                .map(|w| w.get(1).map_or("", |g| g.as_str()))
                .collect();
            let shown = format!(
                "[{}]",
                words
                    .iter()
                    .map(|w| format!("\"{w}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let note = if decision == "allow" {
                codex_note(&words, s)
            } else {
                String::new()
            };
            rules.push(OwnRule {
                decision,
                rule: shown,
                note,
            });
        }
    }
    OwnRules {
        file: Some(open),
        rules,
        problem: None,
    }
}

// Claude: ask wins over allow, so an allow of a ticked rule's pattern still asks.
fn claude_note(rule: &str, s: &KilnoSettings) -> String {
    ask_rules::all()
        .find(|r| {
            r.claude
                .iter()
                .any(|p| rule == format!("Bash({p})") || rule == format!("PowerShell({p})"))
        })
        .map_or_else(String::new, |hit| note(hit, is_ticked(hit, s)))
}

// Codex: the strictest matching decision wins, so prompt beats allow, but only with Codex on.
fn codex_note(words: &[&str], s: &KilnoSettings) -> String {
    ask_rules::all()
        .find(|r| r.codex.contains(&words))
        .map_or_else(String::new, |hit| {
            note(hit, s.also_for_codex && is_ticked(hit, s))
        })
}

fn is_ticked(rule: &AskRule, s: &KilnoSettings) -> bool {
    ask_rules::ticked(s).any(|r| r.id == rule.id)
}

fn note(rule: &AskRule, asks: bool) -> String {
    if asks {
        format!("{} still asks first", rule.label)
    } else {
        format!("{} runs without asking", rule.label)
    }
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
}

struct Patterns {
    prefix_rule: Regex,
    pattern: Regex,
    decision: Regex,
    word: Regex,
}

fn re() -> &'static Patterns {
    static RE: OnceLock<Patterns> = OnceLock::new();
    RE.get_or_init(|| Patterns {
        prefix_rule: Regex::new(r"(?s)prefix_rule\s*\(([^)]*)\)").expect("valid regex"),
        pattern: Regex::new(r"pattern\s*=\s*\[([^\]]*)\]").expect("valid regex"),
        decision: Regex::new(r#"decision\s*=\s*"(\w+)""#).expect("valid regex"),
        word: Regex::new(r#""([^"]*)""#).expect("valid regex"),
    })
}
