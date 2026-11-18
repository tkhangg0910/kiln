//! What `kilno [folder] [--background]` asks for (App.xaml.cs OnStartup / Chosen).

use crate::paths::{normalize, same_dir};
use crate::system::login_startup::BACKGROUND_ARG;
use std::path::Path;

/// The parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartArgs {
    /// Started at login: no window, only the island, until asked for.
    pub background: bool,
    /// Where to start a session; None opens on the home folder with no session.
    pub folder: Option<String>,
}

/// The first argument that is an existing folder (any position) wins. Else, unless started in
/// the background, the working directory, unless nobody chose it: a shortcut or the Start menu
/// starts Kilno in its own folder, and some launchers in System32.
pub fn parse(
    args: &[String],
    cwd: &Path,
    exe_dir: &Path,
    system_dir: &Path,
    is_dir: impl Fn(&str) -> bool,
) -> StartArgs {
    let background = args.iter().any(|a| a == BACKGROUND_ARG);
    let folder = args.iter().find(|a| is_dir(a)).cloned().or_else(|| {
        if background {
            return None;
        }
        let dir = normalize(cwd);
        let unchosen = same_dir(&dir, &normalize(exe_dir)) || same_dir(&dir, &normalize(system_dir));
        (!unchosen).then(|| cwd.to_string_lossy().into_owned())
    });
    StartArgs { background, folder }
}

impl StartArgs {
    /// What a second launch hands the running Kilno: its folder, "" for none (just show the
    /// window), nothing at all for a login start (it has nothing to add).
    pub fn handoff(&self) -> Option<String> {
        (!self.background).then(|| self.folder.clone().unwrap_or_default())
    }
}

/// A handoff message as the running Kilno reads it: UTF-8 to the end, a BOM (which the C#
/// client wrote) dropped, trimmed.
pub fn decode_handoff(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    String::from_utf8_lossy(bytes).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str], cwd: &str) -> StartArgs {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        parse(
            &args,
            Path::new(cwd),
            Path::new(r"C:\Users\me\AppData\Local\Programs\Kilno\"),
            Path::new(r"C:\Windows\system32"),
            |p| p.starts_with(r"D:\"),
        )
    }

    #[test]
    fn first_existing_folder_argument_wins_anywhere() {
        let a = run(&["--x", r"D:\work", r"D:\other"], r"C:\cwd");
        assert_eq!(a.folder.as_deref(), Some(r"D:\work"));
        assert!(!a.background);
    }

    // Windows paths compare case-insensitively and with `\`; these two lean on that.
    #[cfg(windows)]
    #[test]
    fn cwd_unless_exe_folder_or_system32() {
        assert_eq!(run(&[], r"C:\proj").folder.as_deref(), Some(r"C:\proj"));
        assert_eq!(run(&[], r"c:\users\ME\AppData\Local\Programs\Kilno").folder, None);
        assert_eq!(run(&[], r"C:\Windows\System32\").folder, None);
        assert_eq!(run(&["nope"], r"C:\Windows\System32").folder, None);
    }

    #[cfg(windows)]
    #[test]
    fn handoff_is_the_folder_or_empty_and_nothing_from_login() {
        assert_eq!(run(&[], r"C:\proj").handoff().as_deref(), Some(r"C:\proj"));
        assert_eq!(run(&[], r"C:\Windows\System32").handoff().as_deref(), Some(""));
        assert_eq!(run(&["--background", r"D:\w"], r"C:\proj").handoff(), None);
    }

    #[test]
    fn decode_strips_the_bom_and_trims() {
        assert_eq!(decode_handoff(b"\xEF\xBB\xBF C:\\x \r\n"), r"C:\x");
        assert_eq!(decode_handoff(b""), "");
        assert_eq!(decode_handoff("D:\\Đồ".as_bytes()), "D:\\Đồ");
    }

    #[test]
    fn background_ignores_cwd_but_not_a_folder_argument() {
        let a = run(&["--background"], r"C:\proj");
        assert!(a.background);
        assert_eq!(a.folder, None);
        assert_eq!(run(&["--background", r"D:\w"], r"C:\proj").folder.as_deref(), Some(r"D:\w"));
    }
}
