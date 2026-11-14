//! checks/settings.cs: never the real settings file, a throwaway folder.

use kilno_core::agent::AgentKind;
use kilno_core::settings::{KilnoSettings, NewSessionAgent, StatusTheme};
use std::path::{Path, PathBuf};

fn defaults() -> KilnoSettings {
    KilnoSettings::default()
}

fn file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    (dir, path)
}

fn load_text(path: &Path, text: &str) -> KilnoSettings {
    std::fs::write(path, text).unwrap();
    KilnoSettings::load(path)
}

#[test]
fn defaults_and_round_trip() {
    let (dir, path) = file();
    assert_eq!(
        KilnoSettings::load(&path),
        defaults(),
        "no file is the defaults"
    );
    let d = defaults();
    assert!(
        d.status_header
            && d.status_footer
            && d.status_model
            && d.status_branch
            && d.status_context
            && d.status_effort
            && d.status_five_hour
            && d.status_seven_day,
        "every field shows by default"
    );
    let changed = KilnoSettings {
        status_header: false,
        status_effort: false,
        status_seven_day: false,
        ..defaults()
    };
    changed.save(&path);
    assert_eq!(
        KilnoSettings::load(&path),
        changed,
        "saved settings load back"
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "no temp file left behind"
    );
}

#[test]
fn lenient_keys() {
    let (_dir, path) = file();
    assert_eq!(
        load_text(
            &path,
            r#"{"statusHeader":false,"someLaterSetting":{"x":1}}"#
        ),
        KilnoSettings {
            status_header: false,
            ..defaults()
        },
        "unknown keys are ignored"
    );
    assert_eq!(
        load_text(&path, r#"{"statusModel":false}"#),
        KilnoSettings {
            status_model: false,
            ..defaults()
        },
        "missing keys fall back to defaults"
    );
    assert_eq!(
        load_text(&path, r#"{"statusModel":"no","statusBranch":false}"#),
        KilnoSettings {
            status_branch: false,
            ..defaults()
        },
        "a key of the wrong type falls back, the others still count"
    );
}

#[test]
fn screenshots() {
    let (_dir, path) = file();
    assert!(
        defaults().screenshot_hotkey,
        "screenshot hotkey is on by default"
    );
    assert!(
        !defaults().screenshot_offer,
        "offering every screenshot is off by default"
    );
    let shots = KilnoSettings {
        screenshot_hotkey: false,
        screenshot_offer: true,
        ..defaults()
    };
    shots.save(&path);
    assert_eq!(
        KilnoSettings::load(&path),
        shots,
        "screenshot settings load back"
    );
}

#[test]
fn update_check() {
    let (_dir, path) = file();
    assert!(defaults().update_check, "update checks are on by default");
    assert!(
        load_text(&path, r#"{"statusHeader":false}"#).update_check,
        "no updateCheck key is on"
    );
    assert_eq!(
        load_text(&path, r#"{"updateCheck":false}"#),
        KilnoSettings {
            update_check: false,
            ..defaults()
        },
        "updateCheck off is kept"
    );
    KilnoSettings {
        update_check: false,
        ..defaults()
    }
    .save(&path);
    assert!(
        !KilnoSettings::load(&path).update_check,
        "updateCheck off loads back"
    );
}

#[test]
fn reserve() {
    let (_dir, path) = file();
    assert_eq!(defaults().reserve_at, 0, "reserve is off by default");
    let reserve = KilnoSettings {
        reserve_at: 80,
        reserve_seven_day: false,
        ..defaults()
    };
    reserve.save(&path);
    assert_eq!(KilnoSettings::load(&path), reserve, "a reserve loads back");
    assert_eq!(
        load_text(&path, r#"{"reserveAt":55}"#).reserve_at,
        0,
        "a threshold not offered is off"
    );
    assert_eq!(
        load_text(&path, r#"{"reserveAt":"90"}"#).reserve_at,
        0,
        "a threshold of the wrong type is off"
    );
}

#[test]
fn theme() {
    let (_dir, path) = file();
    assert_eq!(defaults().theme, StatusTheme::Numbers, "numbers by default");
    KilnoSettings {
        theme: StatusTheme::Rings,
        ..defaults()
    }
    .save(&path);
    assert_eq!(
        KilnoSettings::load(&path).theme,
        StatusTheme::Rings,
        "a theme loads back"
    );
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("\"theme\": \"rings\""),
        "saved by its name"
    );
    assert_eq!(
        load_text(&path, r#"{"theme":"voxel"}"#).theme,
        StatusTheme::Numbers,
        "a theme not offered is numbers"
    );
    assert_eq!(
        load_text(&path, r#"{"theme":"2"}"#).theme,
        StatusTheme::Numbers,
        "a number is not a theme"
    );
}

#[test]
fn setup() {
    let (_dir, path) = file();
    assert!(!defaults().setup_done, "setup not done by default");
    assert!(
        defaults().use_claude && defaults().use_codex,
        "both agents in use by default"
    );
    assert_eq!(
        defaults().default_agent,
        NewSessionAgent::Claude,
        "claude starts new sessions by default"
    );
    let setup = KilnoSettings {
        setup_done: true,
        use_codex: false,
        default_agent: NewSessionAgent::Ask,
        claude_path: Some(r"C:\tools\claude.exe".into()),
        codex_path: None,
        ..defaults()
    };
    setup.save(&path);
    assert_eq!(KilnoSettings::load(&path), setup, "setup choices load back");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("codexPath"), "no path saved is no key");
    assert_eq!(
        defaults().new_session_kind(),
        Some(AgentKind::Claude),
        "new session: claude by default"
    );
    assert_eq!(
        KilnoSettings {
            default_agent: NewSessionAgent::Codex,
            ..defaults()
        }
        .new_session_kind(),
        Some(AgentKind::Codex),
        "new session: codex when chosen"
    );
    assert_eq!(
        KilnoSettings {
            default_agent: NewSessionAgent::Ask,
            ..defaults()
        }
        .new_session_kind(),
        None,
        "new session: ask each time"
    );
    assert_eq!(
        setup.new_session_kind(),
        Some(AgentKind::Claude),
        "new session: never asks with one agent in use"
    );
    assert_eq!(
        KilnoSettings {
            use_claude: false,
            ..defaults()
        }
        .new_session_kind(),
        Some(AgentKind::Codex),
        "new session: never an agent not in use"
    );
    assert!(
        text.contains("\"defaultAgent\": \"ask\""),
        "an agent saved by its name"
    );
    let odd = load_text(
        &path,
        r#"{"defaultAgent":"gemini","claudePath":"","codexPath":3}"#,
    );
    assert_eq!(
        odd.default_agent,
        NewSessionAgent::Claude,
        "an agent not offered is claude"
    );
    assert_eq!(odd.claude_path, None, "an empty path is none");
    assert_eq!(odd.codex_path, None, "a path of the wrong type is none");
    let with_status = KilnoSettings {
        status_header: false,
        ..setup
    };
    with_status.save(&path);
    assert_eq!(
        KilnoSettings::load(&path),
        with_status,
        "a status option saved later keeps the setup choices"
    );
}

#[test]
fn broken_files() {
    let (_dir, path) = file();
    assert_eq!(
        load_text(&path, r#"{"statusModel":fal"#),
        defaults(),
        "a half-written file is the defaults"
    );
    assert_eq!(
        load_text(&path, "[true]"),
        defaults(),
        "not an object is the defaults"
    );
    assert_eq!(
        load_text(&path, ""),
        defaults(),
        "an empty file is the defaults"
    );
}

/// Held open without sharing, the way another program might: saving must not panic.
#[cfg(windows)]
#[test]
fn locked_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_dir, path) = file();
    std::fs::write(&path, "{}").unwrap();
    let _held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&path)
        .unwrap();
    KilnoSettings {
        status_header: false,
        ..defaults()
    }
    .save(&path);
    assert_eq!(
        KilnoSettings::load(&path),
        defaults(),
        "a file that cannot be read is the defaults"
    );
}

/// A file as the C# build's Utf8JsonWriter wrote it loads and saves back byte for byte.
#[test]
fn round_trips_a_csharp_file() {
    let (_dir, path) = file();
    let csharp = "{\r\n  \"statusHeader\": false,\r\n  \"statusFooter\": true,\r\n  \"statusModel\": true,\r\n  \"statusBranch\": true,\r\n  \"statusContext\": true,\r\n  \"statusEffort\": true,\r\n  \"statusFiveHour\": true,\r\n  \"statusSevenDay\": false,\r\n  \"reserveAt\": 80,\r\n  \"reserveFiveHour\": true,\r\n  \"reserveSevenDay\": true,\r\n  \"theme\": \"chips\",\r\n  \"setupDone\": true,\r\n  \"useClaude\": true,\r\n  \"useCodex\": true,\r\n  \"defaultAgent\": \"ask\",\r\n  \"claudePath\": \"C:\\\\Users\\\\O\\u0027Brien\\\\Ng\\u01B0\\u1EDDi\\\\claude.exe\",\r\n  \"screenshotHotkey\": true,\r\n  \"screenshotOffer\": false,\r\n  \"updateCheck\": true,\r\n  \"askGroupsOn\": [\r\n    \"publishing\"\r\n  ],\r\n  \"askOff\": [],\r\n  \"alsoForCodex\": true\r\n}";
    std::fs::write(&path, csharp).unwrap();
    let s = KilnoSettings::load(&path);
    assert_eq!(
        s.claude_path.as_deref(),
        Some("C:\\Users\\O'Brien\\Ng\u{01B0}\u{1EDD}i\\claude.exe")
    );
    assert_eq!(
        (
            s.theme,
            s.reserve_at,
            s.ask_groups_on.as_str(),
            s.also_for_codex
        ),
        (StatusTheme::Chips, 80, "publishing", true)
    );
    s.save(&path);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), csharp);
}
