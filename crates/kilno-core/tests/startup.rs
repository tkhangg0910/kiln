//! Port of checks/startup.cs. Never the real Run key: a throwaway key (HKCU\Software\KilnoTest)
//! and marker, both removed at the end.
#![cfg(windows)]

use std::path::Path;

use kilno_core::paths::DataDir;
use kilno_core::system::login_startup::LoginStartup;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const TEST_KEY: &str = r"Software\KilnoTest";

fn value() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(TEST_KEY)
        .ok()?
        .get_value("Kilno")
        .ok()
}

/// Removes the key and the marker even when an assertion fails.
struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(TEST_KEY);
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn login_startup_follows_the_exe_and_the_opt_out() {
    let marker = std::env::temp_dir().join("kilno-check-no-login-start");
    let _cleanup = Cleanup(marker.clone());
    let s = LoginStartup::new(TEST_KEY, "Kilno", &marker);
    let (a, b, c) = (
        Path::new(r"C:\a\kilno.exe"),
        Path::new(r"C:\b\kilno.exe"),
        Path::new(r"C:\c\kilno.exe"),
    );

    assert!(!s.is_on(), "off before anything is written");

    s.keep(a).unwrap();
    assert!(!s.is_on(), "a start never turns it on: only your switch does (Defender's Persistence.A!ml)");

    s.set(true, a).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\a\kilno.exe" --background"#),
        "turned on: the running exe, hidden"
    );

    s.keep(b).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\b\kilno.exe" --background"#),
        "the Kilno it named is gone (moved): follows the exe"
    );

    // A Kilno that is still there (the installed one) is not taken over by another copy.
    let here = std::env::current_exe().unwrap();
    s.set(true, &here).unwrap();
    s.keep(c).unwrap();
    assert_eq!(
        value(),
        Some(kilno_core::system::login_startup::run_command(&here)),
        "a test build leaves it alone"
    );
    s.set(true, b).unwrap();

    s.set(false, b).unwrap();
    assert_eq!(value(), None, "turned off removes the entry");
    s.keep(b).unwrap();
    assert!(!s.is_on(), "and the next start keeps it off");

    s.set(true, b).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\b\kilno.exe" --background"#),
        "turned on again"
    );
    s.keep(c).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\c\kilno.exe" --background"#),
        "next start keeps it on"
    );

    assert!(s.can_write(), "a debug build may still write a test key");
}

#[test]
fn only_a_release_build_writes_the_real_run_key() {
    let real = LoginStartup::for_this_user(&DataDir::new(r"C:\nowhere\Kilno"));
    assert_eq!(real.can_write(), !cfg!(debug_assertions));
}

#[test]
fn repair_only_when_the_named_kilno_is_gone() {
    use kilno_core::system::login_startup::{exe_of, needs_repair};
    let exe = Path::new(r"C:\new\kilno.exe");
    let gone = |_: &Path| false;
    let there = |_: &Path| true;
    assert_eq!(exe_of(r#""C:\x y\kilno.exe" --background"#).unwrap(), Path::new(r"C:\x y\kilno.exe"));
    assert_eq!(exe_of(r"C:\k.exe --background").unwrap(), Path::new(r"C:\k.exe"));
    assert!(!needs_repair(r#""C:\new\kilno.exe" --background"#, exe, gone), "already right");
    assert!(needs_repair(r#""C:\old\kilno.exe" --background"#, exe, gone), "moved: follow");
    assert!(!needs_repair(r#""C:\old\kilno.exe" --background"#, exe, there), "another Kilno still there");
    assert!(needs_repair(r#""C:\NEW\kilno.exe""#, exe, there), "this exe without --background");
    assert!(!needs_repair("", exe, gone), "nothing to read");
}
