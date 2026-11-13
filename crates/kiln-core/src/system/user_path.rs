//! Kiln's folder on the user's PATH (HKCU\Environment), so `kiln` typed in Explorer's address
//! bar or a shell finds it. Edited in the registry rather than with setx, which cuts PATH at 1024
//! characters, and every other entry is kept as written, %VARS% unexpanded.

use std::path::PathBuf;

use crate::paths::Env;

/// The real per-user environment key, under HKCU.
pub const ENVIRONMENT_KEY: &str = "Environment";

/// The folder kiln.exe runs from.
pub fn kiln_folder() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(PathBuf::from)
}

/// The entries of a PATH value: split on `;`, empty and blank ones dropped (so a trailing `;`
/// disappears when it is written back).
pub fn entries(value: &str) -> impl Iterator<Item = &str> {
    value.split(';').filter(|e| !e.trim().is_empty())
}

/// One folder however it's written: quoted, with a trailing slash, or through a %VAR%.
pub fn same(entry: &str, folder: &str, env: &dyn Env) -> bool {
    norm(&expand(entry, env)).to_lowercase() == norm(folder).to_lowercase()
}

fn norm(path: &str) -> &str {
    path.trim().trim_matches('"').trim_end_matches(['\\', '/'])
}

/// `%NAME%` replaced by its value; unknown names are left as written, like
/// `Environment.ExpandEnvironmentVariables`.
fn expand(s: &str, env: &dyn Env) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(len) = after.find('%') else { break };
        out.push_str(&rest[..start]);
        let name = &after[..len];
        match env.var(name) {
            Some(v) if !name.is_empty() => {
                out.push_str(&v);
                rest = &after[len + 1..];
            }
            // Not a variable: keep its first % and look again from the second one.
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The PATH value under one HKCU key.
#[derive(Clone, Debug)]
pub struct UserPath {
    key_path: String,
}

impl UserPath {
    pub fn new(key_path: impl Into<String>) -> Self {
        Self {
            key_path: key_path.into(),
        }
    }

    pub fn for_this_user() -> Self {
        Self::new(ENVIRONMENT_KEY)
    }

    /// As login startup: a dev build never writes the real PATH, or `kiln` would start
    /// target\debug. Any other key is fair game, so the tests can run in debug.
    pub fn can_write(&self) -> bool {
        !cfg!(debug_assertions) || self.key_path != ENVIRONMENT_KEY
    }

    /// This is the real environment key: after writing it, the app broadcasts WM_SETTINGCHANGE
    /// "Environment" so Explorer and new shells see the change without signing out.
    pub fn is_real(&self) -> bool {
        self.key_path == ENVIRONMENT_KEY
    }
}

#[cfg(windows)]
mod reg {
    use std::io;

    use winreg::enums::{RegType, HKEY_CURRENT_USER};
    use winreg::{RegKey, RegValue};

    use super::{entries, same, UserPath};
    use crate::paths::Env;

    /// A raw string value: UTF-16 bytes, trailing NULs dropped.
    fn decode(bytes: &[u8]) -> String {
        let units: Vec<u16> = (0..bytes.len() / 2)
            .map(|i| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]))
            .collect();
        let end = units.iter().rposition(|u| *u != 0).map_or(0, |i| i + 1);
        String::from_utf16_lossy(&units[..end])
    }

    fn encode(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    impl UserPath {
        /// The raw value, %VARS% and all, and its type: REG_EXPAND_SZ unless it was plain REG_SZ
        /// (also when there is none yet).
        pub fn read(&self) -> (String, RegType) {
            let raw = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(&self.key_path)
                .and_then(|k| k.get_raw_value("Path"));
            match raw {
                Ok(v) if matches!(v.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) => {
                    let kind = if v.vtype == RegType::REG_SZ {
                        RegType::REG_SZ
                    } else {
                        RegType::REG_EXPAND_SZ
                    };
                    (decode(&v.bytes), kind)
                }
                _ => (String::new(), RegType::REG_EXPAND_SZ),
            }
        }

        pub fn contains(&self, folder: &str, env: &dyn Env) -> bool {
            entries(&self.read().0).any(|e| same(e, folder, env))
        }

        /// Adds the folder at the end, or removes every entry that names it. Nothing is written
        /// when it is already as asked. Returns whether the real environment key was written, so
        /// the caller broadcasts the change.
        pub fn set(&self, on: bool, folder: &str, env: &dyn Env) -> io::Result<bool> {
            if !self.can_write() {
                return Ok(false);
            }
            let (value, kind) = self.read();
            let mut list: Vec<&str> = entries(&value).collect();
            let has = list.iter().any(|e| same(e, folder, env));
            if on == has {
                return Ok(false);
            }
            if on {
                list.push(folder);
            } else {
                list.retain(|e| !same(e, folder, env));
            }
            let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&self.key_path)?;
            let joined = list.join(";");
            key.set_raw_value(
                "Path",
                &RegValue {
                    bytes: encode(&joined).into(),
                    vtype: kind,
                },
            )?;
            Ok(self.is_real())
        }
    }
}
