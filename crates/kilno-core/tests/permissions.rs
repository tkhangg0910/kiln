//! checks/permissions.cs: the rules each setting gives each agent. Never the real ~/.claude or
//! ~/.codex: throwaway folders.
//!
//! PORTING.md D1: the C# build wrote `<CODEX_HOME>\rules\kilno.rules` (SyncCodex). This build
//! never writes or deletes there, so the SyncCodex checks become "nothing is ever written", and
//! a kilno.rules left by the C# build is listed under Your own rules like any other file.

use kilno_core::paths::ReadOnlyDir;
use kilno_core::security::ask_rules::{
    self, all, claude_ask, codex_rules, codex_rules_file_still_active, CODEX_FILE_NAME, GROUPS,
};
use kilno_core::security::own_rules;
use kilno_core::settings::KilnoSettings;
use kilno_core::status::relay::settings_json;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn defaults() -> KilnoSettings {
    KilnoSettings::default()
}
fn on() -> KilnoSettings {
    KilnoSettings {
        ask_groups_on: "publishing".into(),
        ..defaults()
    }
}
fn off() -> KilnoSettings {
    KilnoSettings {
        ask_groups_on: String::new(),
        ..on()
    }
}
fn no_tags() -> KilnoSettings {
    KilnoSettings {
        ask_off: "tags".into(),
        ..on()
    }
}
fn codex() -> KilnoSettings {
    KilnoSettings {
        also_for_codex: true,
        ..on()
    }
}

fn claude_patterns() -> usize {
    all().map(|r| r.claude.len()).sum()
}

#[test]
fn defaults_ask_nothing() {
    assert_eq!(defaults().ask_groups_on, "", "every group off by default");
    assert_eq!(claude_ask(&defaults()).len(), 0, "nothing asks by default");
    assert_eq!(defaults().ask_off, "", "nothing unticked by default");
    assert!(!defaults().also_for_codex, "codex off by default");
    assert_eq!(all().count(), 8, "eight rules");
    let mut ids: Vec<&str> = all().map(|r| r.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 8, "ids are unique");
}

#[test]
fn claude_rules() {
    let ask = claude_ask(&on());
    assert_eq!(
        ask.len(),
        claude_patterns() * 2,
        "every pattern, for Bash and PowerShell"
    );
    for want in [
        "Bash(git push*)",
        "PowerShell(git push*)",
        "Bash(git -C * push*)",
        "Bash(gh api *-X*)",
    ] {
        assert!(ask.iter().any(|a| a == want), "{want}");
    }
    assert_eq!(
        &ask[..2],
        ["Bash(git push*)", "PowerShell(git push*)"],
        "interleaved per pattern"
    );
    assert_eq!(claude_ask(&off()).len(), 0, "switch off: no rules");

    let unticked = claude_ask(&no_tags());
    assert!(
        !unticked
            .iter()
            .any(|p| p.contains("git tag") || p.contains("* tag*")),
        "unticking tags takes its patterns away"
    );
    assert_eq!(unticked.len(), ask.len() - 4, "and only its patterns");
    assert!(
        unticked.iter().any(|a| a == "Bash(git push*)"),
        "push still asks"
    );
}

#[test]
fn groups() {
    let ask = claude_ask(&on());
    let unticked = claude_ask(&no_tags());
    assert_eq!(
        GROUPS.iter().map(|g| g.id).collect::<Vec<_>>().join(","),
        "publishing",
        "one group"
    );
    assert_eq!(GROUPS[0].icon, "\u{EA18}");
    assert!(
        all().eq(GROUPS.iter().flat_map(|g| g.rules.iter())),
        "All is every group's rules"
    );
    let group_off = KilnoSettings {
        ask_groups_on: String::new(),
        ..no_tags()
    };
    assert_eq!(group_off.ask_off, "tags", "group off keeps its ticks");
    assert_eq!(
        claude_ask(&group_off).len(),
        0,
        "group off with a rule unticked: no rules"
    );
    assert_eq!(
        claude_ask(&no_tags()),
        unticked,
        "back on, the unticked rule stays off"
    );
    assert_eq!(
        claude_ask(&KilnoSettings {
            ask_groups_on: "nope,publishing".into(),
            ..on()
        }),
        ask,
        "an unknown group on is ignored"
    );
}

#[test]
fn settings_file_for_claude() {
    let ask = claude_ask(&on());
    let theirs = json!({"type": "command", "command": "pwsh line.ps1", "padding": 2});
    let theirs = theirs.as_object().unwrap();
    let file: Value =
        serde_json::from_str(&settings_json(r"C:\Kilno\kilno.exe", Some(theirs), &ask)).unwrap();
    let command = file["statusLine"]["command"].as_str().unwrap();
    assert!(
        command.starts_with("'C:/Kilno/kilno.exe' --statusline --pass"),
        "the file still has its statusLine relay"
    );
    assert_eq!(
        command,
        "'C:/Kilno/kilno.exe' --statusline --pass | {\npwsh line.ps1\n}"
    );
    assert_eq!(
        file["statusLine"]["padding"], 2,
        "and the rest of their statusLine"
    );
    assert_eq!(
        file["permissions"]["ask"].as_array().unwrap().len(),
        ask.len(),
        "ask rules in permissions.ask"
    );
    assert_eq!(
        file["permissions"].as_object().unwrap().len(),
        1,
        "nothing but ask"
    );
    assert_eq!(
        theirs["command"], "pwsh line.ps1",
        "their statusLine object is not changed"
    );

    let bare: Value = serde_json::from_str(&settings_json(
        r"C:\Kilno\kilno.exe",
        None,
        &claude_ask(&off()),
    ))
    .unwrap();
    assert!(
        bare.get("permissions").is_none(),
        "switch off: no permissions key"
    );
    assert_eq!(
        bare["statusLine"]["command"], "'C:/Kilno/kilno.exe' --statusline",
        "switch off: statusLine still there"
    );
}

/// Escaped as the C# build's System.Text.Json did (' as \u0027); the hooks that report a
/// turn (UserPromptSubmit, Stop, Notification) follow the status line.
#[test]
fn settings_file_bytes_match_csharp() {
    let got = settings_json(
        r"C:\Users\x\AppData\Local\Programs\Kilno\kilno.exe",
        None,
        &[],
    );
    let exe = r"\u0027C:/Users/x/AppData/Local/Programs/Kilno/kilno.exe\u0027";
    let hook = |e: &str| format!(r#"[{{"hooks":[{{"type":"command","command":"{exe} --hook {e}"}}]}}]"#);
    assert_eq!(
        got,
        format!(
            r#"{{"statusLine":{{"type":"command","command":"{exe} --statusline"}},"hooks":{{"UserPromptSubmit":{},"Stop":{},"Notification":{}}}}}"#,
            hook("working"),
            hook("done"),
            hook("ask")
        )
    );
}

#[test]
fn settings_lists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"statusHeader":false,"useCodex":true}"#).unwrap();
    let old = KilnoSettings::load(&path);
    assert_eq!(old.ask_groups_on, "", "old file: every group off");
    assert_eq!(old.ask_off, "", "old file: all ticked");
    assert!(!old.also_for_codex, "old file: codex off");

    let saved = KilnoSettings {
        ask_off: "push,tags".into(),
        also_for_codex: true,
        ..on()
    };
    saved.save(&path);
    assert_eq!(
        KilnoSettings::load(&path),
        saved,
        "saved settings load back"
    );
    let root: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        root["askOff"].as_array().unwrap().len(),
        2,
        "askOff is an array in the file"
    );
    assert_eq!(
        root["askGroupsOn"].as_array().unwrap().len(),
        1,
        "askGroupsOn is an array in the file"
    );

    std::fs::write(
        &path,
        r#"{"askOff":["tags",3,"",null,"push","tags","a,b"]}"#,
    )
    .unwrap();
    assert_eq!(
        KilnoSettings::load(&path).ask_off,
        "push,tags",
        "odd askOff entries are skipped"
    );
    std::fs::write(&path, r#"{"askOff":"tags"}"#).unwrap();
    assert_eq!(
        KilnoSettings::load(&path).ask_off,
        "",
        "askOff that is not an array is ignored"
    );
    std::fs::write(&path, r#"{"askGroupsOn":["nope",3,"publishing"]}"#).unwrap();
    assert_eq!(
        claude_ask(&KilnoSettings::load(&path)),
        claude_ask(&on()),
        "an unknown group in the file is ignored"
    );
}

#[test]
fn codex_rules_text() {
    assert_eq!(codex_rules(&on()), None, "codex off: no file");
    let rules = codex_rules(&codex()).unwrap();
    let lines: Vec<&str> = rules.split('\n').filter(|l| !l.is_empty()).collect();
    assert!(
        lines[0].starts_with("# Written by Kilno"),
        "starts with a comment saying Kilno wrote it"
    );
    let codex_prefixes: usize = all().map(|r| r.codex.len()).sum();
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.starts_with("prefix_rule("))
            .count(),
        codex_prefixes,
        "a prompt rule per prefix"
    );
    assert_eq!(codex_prefixes, 11);
    assert!(
        lines.contains(&r#"prefix_rule(pattern=["git", "push"], decision="prompt")"#),
        "push as Codex reads it"
    );
    assert!(
        lines.contains(&r#"prefix_rule(pattern=["gh", "api"], decision="prompt")"#),
        "every gh api call"
    );
    assert!(
        rules.ends_with(")\n") && rules.is_ascii(),
        "ASCII, newline at the end"
    );
    assert_eq!(
        codex_rules(&KilnoSettings {
            ask_groups_on: String::new(),
            ..codex()
        }),
        None,
        "codex, switch off: no file"
    );
    assert!(
        !codex_rules(&KilnoSettings {
            ask_off: "tags".into(),
            ..codex()
        })
        .unwrap()
        .contains("\"tag\""),
        "codex, tags unticked"
    );
    let every: Vec<&str> = all().map(|r| r.id).collect();
    assert_eq!(
        codex_rules(&KilnoSettings {
            ask_off: every.join(","),
            ..codex()
        }),
        None,
        "codex, all unticked: no file"
    );
}

/// Every entry under `root`, with size and modified time for files (a folder's own time lags
/// behind its last change on NTFS, so only its presence counts).
fn snapshot(root: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let m = e.metadata().unwrap();
            if m.is_dir() {
                stack.push(e.path());
                out.push((e.path(), 0, std::time::UNIX_EPOCH));
            } else {
                out.push((e.path(), m.len(), m.modified().unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// D1, replacing the SyncCodex checks: nothing in the settings or rules code ever writes under
/// a Codex home, whatever the settings, and a Codex home that doesn't exist is not created.
#[test]
fn codex_home_is_never_written() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("codex");
    let mine = home.join("rules").join("default.rules");
    std::fs::create_dir_all(mine.parent().unwrap()).unwrap();
    std::fs::write(
        &mine,
        "prefix_rule(pattern=[\"git\", \"reset\"], decision=\"prompt\")\n",
    )
    .unwrap();
    let missing = dir.path().join("no-codex");
    let before = snapshot(&home);

    let data = dir.path().join("data");
    for s in [
        on(),
        codex(),
        KilnoSettings {
            ask_off: "tags".into(),
            ..codex()
        },
        KilnoSettings {
            ask_groups_on: String::new(),
            ..codex()
        },
    ] {
        s.save(&data.join("settings.json"));
        let s = KilnoSettings::load(&data.join("settings.json"));
        let _ = (claude_ask(&s), codex_rules(&s));
        let _ = own_rules::codex(&home, &s);
        let _ = own_rules::codex(&missing, &s);
        let _ = codex_rules_file_still_active(&ReadOnlyDir::new(&home), &s);
        let _ = codex_rules_file_still_active(&ReadOnlyDir::new(&missing), &s);
    }
    assert_eq!(
        snapshot(&home),
        before,
        "nothing written, rewritten or deleted under CODEX_HOME"
    );
    assert!(
        !home.join("rules").join(CODEX_FILE_NAME).exists(),
        "codex on: still no kilno.rules"
    );
    assert!(
        !missing.exists(),
        "a Codex home that doesn't exist is not created"
    );
}

/// STA-140: a kilno.rules left by the C# build is reported while Also apply to Codex is on.
#[test]
fn codex_rules_file_left_behind() {
    let dir = tempfile::tempdir().unwrap();
    let home = ReadOnlyDir::new(dir.path());
    assert_eq!(
        codex_rules_file_still_active(&home, &codex()),
        None,
        "no file, nothing to say"
    );
    let file = dir.path().join("rules").join(ask_rules::CODEX_FILE_NAME);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, codex_rules(&codex()).unwrap()).unwrap();
    assert_eq!(
        codex_rules_file_still_active(&home, &codex()),
        Some(file.clone()),
        "still active in Codex"
    );
    assert_eq!(
        codex_rules_file_still_active(&home, &on()),
        None,
        "only while Also apply to Codex is on"
    );
    assert!(file.exists(), "and never deleted");
}

#[test]
fn own_rules_claude() {
    let dir = tempfile::tempdir().unwrap();
    let claude_file = dir.path().join("claude-settings.json");
    assert_eq!(
        own_rules::claude(&claude_file, &on()).problem.as_deref(),
        Some("No settings file yet."),
        "claude: a missing file is one line"
    );
    std::fs::write(&claude_file, r#"{"permissions":{"allow":["Bash(npm test*)","Bash(git push*)"],"ask":["Bash(git commit*)"],"deny":["Read(./.env)"]},"model":"x"}"#).unwrap();
    let own = own_rules::claude(&claude_file, &on());
    assert_eq!(
        own.rules
            .iter()
            .map(|r| r.decision.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        "allow allow ask deny",
        "claude: allow, ask and deny read"
    );
    assert_eq!(
        own.rules[3].rule, "Read(./.env)",
        "claude: the rule as written"
    );
    assert_eq!(
        own.rules[0].note, "",
        "claude: no note on an unrelated rule"
    );
    assert_eq!(
        own.rules[1].note, "Push code still asks first",
        "claude: allow of a ticked rule still asks"
    );
    let push_off = KilnoSettings {
        ask_off: "push".into(),
        ..no_tags()
    };
    assert_eq!(
        own_rules::claude(&claude_file, &push_off).rules[1].note,
        "Push code runs without asking",
        "claude: unticked, it runs without asking"
    );
    assert_eq!(
        own_rules::claude(&claude_file, &off()).rules[1].note,
        "Push code runs without asking",
        "claude: switch off, it runs without asking"
    );
    assert_eq!(
        own.file.as_deref(),
        Some(claude_file.as_path()),
        "claude: Open file is the file"
    );
    std::fs::write(&claude_file, r#"{"permissions":{"allow":["Bash("#).unwrap();
    assert_eq!(
        own_rules::claude(&claude_file, &on()).problem.as_deref(),
        Some("Couldn't read this file."),
        "claude: a broken file is one line"
    );
    std::fs::write(&claude_file, r#"{"permissions":{"allow":"Bash(x)"}}"#).unwrap();
    assert_eq!(
        own_rules::claude(&claude_file, &on()).rules.len(),
        0,
        "claude: a list that isn't one is skipped"
    );
}

#[test]
fn own_rules_codex() {
    let dir = tempfile::tempdir().unwrap();
    let cx_home = dir.path().join("codex-own");
    assert_eq!(
        own_rules::codex(&cx_home, &codex()).problem.as_deref(),
        Some("No rules files yet."),
        "codex: no rules folder is one line"
    );
    let rules_dir = cx_home.join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join("default.rules"),
        "# mine\nprefix_rule(pattern=[\"git\", \"reset\"], decision=\"prompt\")\nprefix_rule(\n  pattern = [\"git\",\"push\"],\n  decision = \"allow\",\n)\nprefix_rule(pattern=[\"npm\", \"test\"])\n",
    )
    .unwrap();

    let cx = own_rules::codex(&cx_home, &codex());
    assert_eq!(cx.rules.len(), 3, "codex: the user's rules");
    assert_eq!(
        cx.rules
            .iter()
            .map(|r| r.decision.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        "prompt allow allow",
        "codex: decisions read"
    );
    assert_eq!(
        cx.rules[1].rule, r#"["git", "push"]"#,
        "codex: a rule over several lines"
    );
    assert_eq!(
        cx.rules[2].rule, r#"["npm", "test"]"#,
        "codex: no decision is Codex's allow"
    );
    assert_eq!(
        cx.rules[1].note, "Push code still asks first",
        "codex: allow of a ticked rule still asks"
    );
    assert_eq!(
        own_rules::codex(&cx_home, &on()).rules[1].note,
        "Push code runs without asking",
        "codex: with Codex off it runs without asking"
    );
    assert_eq!(
        file_name(&cx.file),
        "default.rules",
        "codex: Open file is the user's file"
    );

    // A kilno.rules the C# build left: listed like any other file (D1; the C# build hid it),
    // sorted first, while Open file still opens the user's own.
    let kilno = codex_rules(&codex()).unwrap();
    std::fs::write(rules_dir.join(CODEX_FILE_NAME), &kilno).unwrap();
    let cx = own_rules::codex(&cx_home, &codex());
    assert_eq!(
        cx.rules.len(),
        3 + 11,
        "codex: Kilno's old file is listed too"
    );
    assert!(
        cx.rules[3..].iter().all(|r| r.decision == "prompt"),
        "its prompt rules last (default < kilno)"
    );
    assert_eq!(cx.rules[1].note, "Push code still asks first");
    assert_eq!(
        file_name(&cx.file),
        "default.rules",
        "codex: Open file is still the user's file"
    );
}

fn file_name(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
