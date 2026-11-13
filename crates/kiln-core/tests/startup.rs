//! Port of checks/startup.cs. Never the real Run key: a throwaway key (HKCU\Software\KilnTest)
//! and marker, both removed at the end.
#![cfg(windows)]

use std::path::Path;

use kiln_core::paths::DataDir;
use kiln_core::system::login_startup::LoginStartup;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const TEST_KEY: &str = r"Software\KilnTest";

fn value() -> Option<String> {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(TEST_KEY)
        .ok()?
        .get_value("Kiln")
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
    let marker = std::env::temp_dir().join("kiln-check-no-login-start");
    let _cleanup = Cleanup(marker.clone());
    let s = LoginStartup::new(TEST_KEY, "Kiln", &marker);
    let (a, b, c) = (
        Path::new(r"C:\a\kiln.exe"),
        Path::new(r"C:\b\kiln.exe"),
        Path::new(r"C:\c\kiln.exe"),
    );

    assert!(!s.is_on(), "off before anything is written");

    s.keep(a).unwrap();
    assert!(s.is_on(), "on by default");
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\a\kiln.exe" --background"#),
        "the running exe, hidden"
    );

    s.keep(b).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\b\kiln.exe" --background"#),
        "follows the exe"
    );

    s.set(false, b).unwrap();
    assert_eq!(value(), None, "turned off removes the entry");
    s.keep(b).unwrap();
    assert!(!s.is_on(), "and the next start keeps it off");

    s.set(true, b).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\b\kiln.exe" --background"#),
        "turned on again"
    );
    s.keep(c).unwrap();
    assert_eq!(
        value().as_deref(),
        Some(r#""C:\c\kiln.exe" --background"#),
        "next start keeps it on"
    );

    assert!(s.can_write(), "a debug build may still write a test key");
}

#[test]
fn only_a_release_build_writes_the_real_run_key() {
    let real = LoginStartup::for_this_user(&DataDir::new(r"C:\nowhere\Kiln"));
    assert_eq!(real.can_write(), !cfg!(debug_assertions));
}
