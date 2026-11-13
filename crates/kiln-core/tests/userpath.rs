//! Port of checks/userpath.cs. Never the real HKCU\Environment: a throwaway key
//! (HKCU\Software\KilnTestPath), removed at the end.
#![cfg(windows)]

use kiln_core::paths::MapEnv;
use kiln_core::system::user_path::UserPath;
use winreg::enums::{RegType, HKEY_CURRENT_USER};
use winreg::{RegKey, RegValue};

const TEST_KEY: &str = r"Software\KilnTestPath";

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// The value as stored, %VARS% unexpanded, and its type.
fn raw() -> Option<(String, RegType)> {
    let v = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(TEST_KEY)
        .ok()?
        .get_raw_value("Path")
        .ok()?;
    let b = &v.bytes;
    let units: Vec<u16> = (0..b.len() / 2)
        .map(|i| u16::from_le_bytes([b[2 * i], b[2 * i + 1]]))
        .collect();
    let s = String::from_utf16(&units).unwrap();
    Some((s.trim_end_matches('\0').to_string(), v.vtype))
}

fn write(value: &str, vtype: RegType) {
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(TEST_KEY)
        .unwrap();
    key.set_raw_value(
        "Path",
        &RegValue {
            bytes: utf16(value).into(),
            vtype,
        },
    )
    .unwrap();
}

struct Cleanup;

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(TEST_KEY);
    }
}

#[test]
fn user_path_keeps_every_other_entry_as_written() {
    let _cleanup = Cleanup;
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(TEST_KEY);
    let env = MapEnv::default().with("USERPROFILE", r"C:\Users\tester");
    let p = UserPath::new(TEST_KEY);

    assert!(!p.contains(r"C:\kiln", &env), "no PATH: not on it");
    assert!(
        !p.set(true, r"C:\kiln", &env).unwrap(),
        "a test key needs no broadcast"
    );
    assert_eq!(
        raw(),
        Some((r"C:\kiln".into(), RegType::REG_EXPAND_SZ)),
        "added to an empty PATH"
    );

    write(
        r"%USERPROFILE%\.local\bin;C:\tools;",
        RegType::REG_EXPAND_SZ,
    );
    p.set(true, r"C:\kiln", &env).unwrap();
    assert_eq!(
        raw(),
        Some((
            r"%USERPROFILE%\.local\bin;C:\tools;C:\kiln".into(),
            RegType::REG_EXPAND_SZ
        )),
        "added at the end, the rest as written"
    );
    p.set(true, r"C:\kiln", &env).unwrap();
    assert_eq!(
        raw().unwrap().0,
        r"%USERPROFILE%\.local\bin;C:\tools;C:\kiln",
        "added twice is once"
    );
    assert!(p.contains(r"C:\kiln", &env), "on it");
    assert!(
        p.contains(r"C:\Kiln\", &env),
        "on it with a trailing slash too"
    );

    p.set(false, r"C:\kiln\", &env).unwrap();
    assert_eq!(
        raw(),
        Some((
            r"%USERPROFILE%\.local\bin;C:\tools".into(),
            RegType::REG_EXPAND_SZ
        )),
        "removed, only ours"
    );
    p.set(false, r"C:\kiln", &env).unwrap();
    assert_eq!(
        raw().unwrap().0,
        r"%USERPROFILE%\.local\bin;C:\tools",
        "removed when not there"
    );

    assert!(
        p.contains(r"C:\Users\tester\.local\bin", &env),
        "an entry written with a %VAR% counts"
    );

    write(r#""C:\Quoted Folder";C:\x"#, RegType::REG_SZ);
    assert!(
        p.contains(r"C:\Quoted Folder", &env),
        "a quoted entry counts"
    );
    p.set(true, r"C:\kiln", &env).unwrap();
    assert_eq!(
        raw(),
        Some((
            r#""C:\Quoted Folder";C:\x;C:\kiln"#.into(),
            RegType::REG_SZ
        )),
        "a plain value stays plain"
    );

    assert!(p.can_write(), "a debug build may still write a test key");
}

#[test]
fn only_a_release_build_writes_the_real_path() {
    assert_eq!(
        UserPath::for_this_user().can_write(),
        !cfg!(debug_assertions)
    );
}
