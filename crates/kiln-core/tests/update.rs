//! Port of checks/update.cs: which release is newer, and where its script and page are.
//! Assumes the default update repo (no KILN_UPDATE_REPO at build time).

use kiln_core::system::updater::{
    button_text, button_tooltip, current, idle, install_command, newer, newer_from_json, release_url,
    script_url, tag_from_json, Version, CHECK_EVERY, CONFIRM_TEXT, CONFIRM_TITLE, WAITING_TEXT,
    WAITING_TOOLTIP,
};

// C#'s assembly version had four parts (1.0.0.0); release tags have three and a v.
const CURRENT: Version = Version::new(1, 0, 0);

#[test]
fn newer_versions() {
    assert_eq!(
        newer(Some("v1.0.1"), CURRENT),
        Some(Version::new(1, 0, 1)),
        "a later patch"
    );
    assert_eq!(
        newer(Some("v1.2.0"), CURRENT),
        Some(Version::new(1, 2, 0)),
        "a later minor"
    );
    assert_eq!(
        newer(Some("2.0.0"), CURRENT),
        Some(Version::new(2, 0, 0)),
        "without the v too"
    );
}

#[test]
fn not_newer() {
    assert_eq!(newer(Some("v1.0.0"), CURRENT), None, "the same release");
    assert_eq!(
        newer(Some("v1.0.0.0"), CURRENT),
        None,
        "the same release in four parts"
    );
    assert_eq!(newer(Some("v0.9.9"), CURRENT), None, "an older one");
    assert_eq!(
        newer(Some("nightly"), CURRENT),
        None,
        "a tag that is no version"
    );
    assert_eq!(newer(None, CURRENT), None, "no tag");
}

#[test]
fn versions_parse_like_dotnet() {
    assert_eq!(Version::parse("1.0.0.0"), Some(CURRENT));
    assert_eq!(Version::parse("1.0"), Some(CURRENT));
    assert_eq!(Version::parse("1"), None);
    assert_eq!(Version::parse("1.0.x"), None);
    let pkg = env!("CARGO_PKG_VERSION");
    assert_eq!(
        Version::parse(pkg).map(|v| v.to_string()).as_deref(),
        Some(pkg)
    );
}

#[test]
fn script_from_the_tag() {
    assert_eq!(
        script_url(Version::new(1, 0, 2)),
        "https://raw.githubusercontent.com/tkhangg0910/kiln/v1.0.2/scripts/install.ps1"
    );
}

#[test]
fn release_page_of_the_tag() {
    assert_eq!(
        release_url(Version::new(1, 0, 5)),
        "https://github.com/tkhangg0910/kiln/releases/tag/v1.0.5"
    );
}

#[test]
fn tag_from_github_json() {
    let body = r#"{"url":"x","tag_name":"v1.0.7","name":"Kiln 1.0.7"}"#;
    assert_eq!(tag_from_json(body).as_deref(), Some("v1.0.7"));
    assert_eq!(newer_from_json(body, CURRENT), Some(Version::new(1, 0, 7)));
    assert_eq!(tag_from_json(r#"{"message":"Not Found"}"#), None);
    assert_eq!(tag_from_json("<html>"), None);
}

#[test]
fn button_and_tooltip_texts() {
    let to = Version::new(1, 0, 7);
    assert_eq!(button_text(to), "Update to 1.0.7");
    assert_eq!(
        button_tooltip(to, Version::new(1, 0, 6)),
        "Kiln 1.0.7 is out (you have 1.0.6). Kiln closes, updates and opens again, once no session is working."
    );
    assert_eq!(WAITING_TEXT, "Update waits for running sessions");
    assert_eq!(WAITING_TOOLTIP, "Kiln updates as soon as no session is working. Press to call it off.");
    assert_eq!(CONFIRM_TITLE, "Update Kiln");
    assert_eq!(
        CONFIRM_TEXT,
        "Kiln closes to update and opens again. Open sessions close and resume from the sidebar; any that are still working are waited for."
    );
    assert_eq!(current().to_string(), env!("CARGO_PKG_VERSION"));
    assert_eq!(CHECK_EVERY.as_secs(), 12 * 3600);
}

#[test]
fn idle_when_no_pane_works_or_asks() {
    use kiln_core::pane_status::PaneStatus::*;
    assert!(idle([]));
    assert!(idle([Starting, Done, Error, Exited]));
    assert!(!idle([Done, Working]));
    assert!(!idle([NeedsInput]));
}

#[test]
fn install_command_runs_the_tagged_script_with_kiln_dir() {
    let c = install_command(Version::new(1, 0, 7), std::path::Path::new(r"C:\Apps\Kiln\"));
    assert_eq!(c.get_program(), "powershell.exe");
    let args: Vec<_> = c.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(args[..4], ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command"]);
    assert!(args[4].contains("irm 'https://raw.githubusercontent.com/tkhangg0910/kiln/v1.0.7/scripts/install.ps1' | iex"));
    let env: Vec<_> = c.get_envs().collect();
    assert_eq!(env, [(std::ffi::OsStr::new("KILN_DIR"), Some(std::ffi::OsStr::new(r"C:\Apps\Kiln\")))]);
}
