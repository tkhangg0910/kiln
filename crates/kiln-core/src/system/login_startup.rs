//! Kiln starts with Windows from the per-user Run key, with its window hidden, so the island is
//! there and sessions can be picked up from anywhere. On by default; turning it off leaves a
//! marker, because otherwise the next start would just write it back.

use std::path::{Path, PathBuf};

use crate::paths::DataDir;

/// The argument login starts Kiln with: no window, just the island (and its greeting).
pub const BACKGROUND_ARG: &str = "--background";

/// The real per-user Run key, under HKCU.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The marker in Kiln's data folder that says the user turned it off.
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

    /// The real Run key's `Kiln` value, with the marker in `data`.
    pub fn for_this_user(data: &DataDir) -> Self {
        Self::new(RUN_KEY, "Kiln", data.join(OPT_OUT_FILE))
    }

    /// A dev build never writes the real Run key: it would point login at target\debug instead
    /// of the installed Kiln. Any other key is fair game, so the tests can run in debug.
    pub fn can_write(&self) -> bool {
        !cfg!(debug_assertions) || self.key_path != RUN_KEY
    }
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
        /// time means a Kiln that moved is the one that starts.
        pub fn keep(&self, exe: &Path) -> io::Result<()> {
            if self.can_write() && !self.opt_out.exists() {
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
