//! Kilno starts with Windows from the per-user Run key, with its window hidden, so the island is
//! there and sessions can be picked up from anywhere. Off until you turn it on (setup or
//! Settings › General): Kilno never writes the Run key by itself at start, which is what
//! Defender's `Persistence.A!ml` flags. Turning it off also leaves a marker, so a moved Kilno's
//! repair ([`LoginStartup::keep`]) does not write it back.

use std::path::{Path, PathBuf};

use crate::paths::DataDir;

/// The argument login starts Kilno with: no window, just the island (and its greeting).
pub const BACKGROUND_ARG: &str = "--background";

/// The real per-user Run key, under HKCU.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The marker in Kilno's data folder that says the user turned it off.
pub const OPT_OUT_FILE: &str = "no-login-start";

/// What the Run value holds: `"<exe>" --background`.
pub fn run_command(exe: &Path) -> String {
    format!("\"{}\" {BACKGROUND_ARG}", exe.display())
}

/// One Run-key value (HKCU) and its opt-out marker.
#[derive(Clone, Debug)]
pub struct LoginStartup {
    key_path: String,
    value_name: String,
    opt_out: PathBuf,
}

impl LoginStartup {
    pub fn new(
        key_path: impl Into<String>,
        value_name: impl Into<String>,
        opt_out: impl Into<PathBuf>,
    ) -> Self {
        Self {
            key_path: key_path.into(),
            value_name: value_name.into(),
            opt_out: opt_out.into(),
        }
    }

    /// The real Run key's `Kilno` value, with the marker in `data`.
    pub fn for_this_user(data: &DataDir) -> Self {
        Self::new(RUN_KEY, "Kilno", data.join(OPT_OUT_FILE))
    }

    /// A dev build never writes the real Run key: it would point login at target\debug instead
    /// of the installed Kilno. Any other key is fair game, so the tests can run in debug.
    pub fn can_write(&self) -> bool {
        !cfg!(debug_assertions) || self.key_path != RUN_KEY
    }
}

/// The exe a Run value starts: the quoted path at its front (or up to the first space).
pub fn exe_of(command: &str) -> Option<PathBuf> {
    let c = command.trim();
    let path = match c.strip_prefix('"') {
        Some(rest) => &rest[..rest.find('"')?],
        None => c.split_whitespace().next()?,
    };
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Whether a Run value `current` should be pointed at `exe`: only when it names another exe
/// that no longer `exists` (Kilno was moved). Already right, or naming a Kilno still there, it
/// stays as it is.
pub fn needs_repair(current: &str, exe: &Path, exists: impl Fn(&Path) -> bool) -> bool {
    if current == run_command(exe) {
        return false;
    }
    let Some(named) = exe_of(current) else {
        return false; // not ours to read: left alone
    };
    let same = named.as_os_str().eq_ignore_ascii_case(exe.as_os_str());
    // This exe with other arguments: put `--background` back. Another exe: only if it is gone.
    same || !exists(&named)
}

#[cfg(windows)]
mod reg {
    use std::io;
    use std::path::Path;

    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    use super::{run_command, LoginStartup};

    fn not_found_ok(r: io::Result<()>) -> io::Result<()> {
        match r {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    impl LoginStartup {
        /// The value is there, whatever it holds.
        pub fn is_on(&self) -> bool {
            RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(&self.key_path)
                .is_ok_and(|k| k.get_raw_value(&self.value_name).is_ok())
        }

        /// At every start: unless turned off, make sure login launches this exe. Rewriting it each
        /// time means a Kilno that moved is the one that starts.
        ///
        /// Only a repair now: the value is never created here, only by your switch (setup or
        /// Settings), and only rewritten when the Kilno it names is gone (you moved it). An exe
        /// that writes a Run key on every launch is what Defender's `Persistence.A!ml` flags, and a
        /// second copy (a test build) must not take login over from the installed one.
        pub fn keep(&self, exe: &Path) -> io::Result<()> {
            if !self.can_write() || self.opt_out.exists() {
                return Ok(());
            }
            let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(&self.key_path) else {
                return Ok(());
            };
            let Ok(current) = key.get_value::<String, _>(&self.value_name) else {
                return Ok(());
            };
            if super::needs_repair(&current, exe, |p| p.exists()) {
                self.write(exe)?;
            }
            Ok(())
        }

        pub fn set(&self, on: bool, exe: &Path) -> io::Result<()> {
            if !self.can_write() {
                return Ok(());
            }
            if on {
                not_found_ok(std::fs::remove_file(&self.opt_out))?;
                return self.write(exe);
            }
            if let Some(dir) = self.opt_out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&self.opt_out, "")?;
            match RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey_with_flags(&self.key_path, winreg::enums::KEY_SET_VALUE)
            {
                Ok(key) => not_found_ok(key.delete_value(&self.value_name)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        }

        /// Only writes when the value differs.
        fn write(&self, exe: &Path) -> io::Result<()> {
            let command = run_command(exe);
            let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&self.key_path)?;
            if key.get_value::<String, _>(&self.value_name).ok().as_deref()
                != Some(command.as_str())
            {
                key.set_value(&self.value_name, &command)?;
            }
            Ok(())
        }
    }
}

/// Elsewhere (Linux desktops, per the XDG autostart spec): a `kilno.desktop` file in
/// `$XDG_CONFIG_HOME/autostart` (else `~/.config/autostart`), its `Exec=` the same command the
/// Run value holds. The same rules: only your switch creates it; a start only repairs it.
#[cfg(unix)]
mod desktop {
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{run_command, LoginStartup, RUN_KEY};

    fn not_found_ok(r: io::Result<()>) -> io::Result<()> {
        match r {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    impl LoginStartup {
        /// The real autostart folder for [`LoginStartup::for_this_user`]; for any other key (the
        /// tests), that key taken as a folder.
        fn desktop_file(&self) -> PathBuf {
            let dir = if self.key_path == RUN_KEY {
                let env = crate::paths::OsEnv;
                crate::paths::Env::var(&env, "XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| crate::paths::home(&env).join(".config"))
                    .join("autostart")
            } else {
                PathBuf::from(&self.key_path)
            };
            dir.join(format!("{}.desktop", self.value_name.to_lowercase()))
        }

        fn exec(&self) -> Option<String> {
            let text = std::fs::read_to_string(self.desktop_file()).ok()?;
            text.lines().find_map(|l| l.strip_prefix("Exec=")).map(str::to_string)
        }

        pub fn is_on(&self) -> bool {
            self.desktop_file().is_file()
        }

        /// At every start: only a repair (see the Windows one), never a new entry.
        pub fn keep(&self, exe: &Path) -> io::Result<()> {
            if !self.can_write() || self.opt_out.exists() {
                return Ok(());
            }
            match self.exec() {
                Some(current) if super::needs_repair(&current, exe, |p| p.exists()) => self.write(exe),
                _ => Ok(()),
            }
        }

        pub fn set(&self, on: bool, exe: &Path) -> io::Result<()> {
            if !self.can_write() {
                return Ok(());
            }
            if on {
                not_found_ok(std::fs::remove_file(&self.opt_out))?;
                return self.write(exe);
            }
            if let Some(dir) = self.opt_out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&self.opt_out, "")?;
            not_found_ok(std::fs::remove_file(self.desktop_file()))
        }

        fn write(&self, exe: &Path) -> io::Result<()> {
            let file = self.desktop_file();
            let text = format!(
                "[Desktop Entry]\nType=Application\nName=Kilno\nComment=Starts hidden; the island is ready.\nExec={}\nIcon=kilno\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                run_command(exe)
            );
            if std::fs::read_to_string(&file).ok().as_deref() == Some(text.as_str()) {
                return Ok(());
            }
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(file, text)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn desktop_entry_only_from_the_switch() {
            let dir = tempfile::tempdir().unwrap();
            let auto = dir.path().join("autostart");
            let s = LoginStartup::new(auto.to_string_lossy(), "Kilno", dir.path().join("no-login-start"));
            let exe = dir.path().join("kilno");
            s.keep(&exe).unwrap();
            assert!(!s.is_on(), "a start never adds it");
            s.set(true, &exe).unwrap();
            assert!(s.is_on());
            let text = std::fs::read_to_string(auto.join("kilno.desktop")).unwrap();
            assert!(text.contains(&format!("Exec=\"{}\" --background", exe.display())));
            s.set(false, &exe).unwrap();
            assert!(!s.is_on(), "off removes it");
        }
    }
}
