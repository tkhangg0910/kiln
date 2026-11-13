//! Keys, pastes and mouse reports as the bytes an xterm sends (xterm.js defaults, which the C#
//! pane used; Kiln only changed Ctrl+C / Ctrl+V, handled in the widget).

use alacritty_terminal::term::TermMode;
use egui::{Key, Modifiers};

/// xterm's modifier parameter: 1 + shift + 2·alt + 4·ctrl.
fn mod_param(m: Modifiers) -> u8 {
    1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8
}

fn csi_mod(m: Modifiers, tail: &str) -> Vec<u8> {
    format!("\x1b[1;{}{}", mod_param(m), tail).into_bytes()
}

fn tilde(n: u8, m: Modifiers) -> Vec<u8> {
    let p = mod_param(m);
    if p == 1 {
        format!("\x1b[{n}~").into_bytes()
    } else {
        format!("\x1b[{n};{p}~").into_bytes()
    }
}

/// Bytes for a key that is not plain text (arrows, F-keys, Enter, Ctrl+letter...). None when
/// the key produces text (sent from the text event instead) or nothing.
pub fn encode_key(key: Key, m: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let plain = !m.shift && !m.alt && !m.ctrl;
    let esc = |mut v: Vec<u8>| {
        if m.alt {
            v.insert(0, 0x1b);
        }
        v
    };
    let cursor = |c: char| -> Vec<u8> {
        if plain {
            if app_cursor {
                format!("\x1bO{c}").into_bytes()
            } else {
                format!("\x1b[{c}").into_bytes()
            }
        } else {
            csi_mod(m, &c.to_string())
        }
    };
    let ss3 = |c: char| -> Vec<u8> {
        if plain {
            format!("\x1bO{c}").into_bytes()
        } else {
            csi_mod(m, &c.to_string())
        }
    };
    Some(match key {
        Key::Enter => esc(vec![b'\r']),
        Key::Tab if m.shift => b"\x1b[Z".to_vec(),
        Key::Tab => esc(vec![b'\t']),
        Key::Backspace if m.ctrl => esc(vec![0x08]),
        Key::Backspace => esc(vec![0x7f]),
        Key::Escape => esc(vec![0x1b]),
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => tilde(2, m),
        Key::Delete => tilde(3, m),
        Key::PageUp => tilde(5, m),
        Key::PageDown => tilde(6, m),
        Key::F1 => ss3('P'),
        Key::F2 => ss3('Q'),
        Key::F3 => ss3('R'),
        Key::F4 => ss3('S'),
        Key::F5 => tilde(15, m),
        Key::F6 => tilde(17, m),
        Key::F7 => tilde(18, m),
        Key::F8 => tilde(19, m),
        Key::F9 => tilde(20, m),
        Key::F10 => tilde(21, m),
        Key::F11 => tilde(23, m),
        Key::F12 => tilde(24, m),
        Key::Space if m.ctrl => esc(vec![0]),
        _ if m.ctrl && !m.alt => return ctrl_byte(key).map(|b| vec![b]),
        _ if m.ctrl && m.alt => return ctrl_byte(key).map(|b| vec![0x1b, b]),
        _ if m.alt => return letter(key).map(|c| vec![0x1b, if m.shift { c.to_ascii_uppercase() } else { c }]),
        _ => return None,
    })
}

fn letter(key: Key) -> Option<u8> {
    let name = key.name();
    (name.len() == 1).then(|| name.as_bytes()[0].to_ascii_lowercase()).filter(|c| c.is_ascii_alphanumeric())
}

/// Ctrl+key as a C0 control byte.
fn ctrl_byte(key: Key) -> Option<u8> {
    Some(match key {
        Key::OpenBracket => 0x1b,
        Key::Backslash => 0x1c,
        Key::CloseBracket => 0x1d,
        Key::Num6 => 0x1e,
        Key::Minus => 0x1f,
        Key::Num2 => 0x00,
        _ => {
            let c = letter(key)?;
            if !c.is_ascii_lowercase() {
                return None;
            }
            c - b'a' + 1
        }
    })
}

/// A paste as xterm sends it: newlines become CR; wrapped in bracketed-paste markers when the
/// app turned that mode on (2004).
pub fn paste_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    let body = text.replace("\r\n", "\r").replace('\n', "\r");
    if mode.contains(TermMode::BRACKETED_PASTE) {
        // A pasted end marker would end the paste early and run the rest as typed keys.
        let body = strip_esc(&body);
        format!("\x1b[200~{body}\x1b[201~").into_bytes()
    } else {
        body.into_bytes()
    }
}

/// A path pasted so the agent attaches it: always bracketed, whatever the mode (TP:374).
pub fn bracketed_path(path: &str) -> Vec<u8> {
    format!("\x1b[200~{}\x1b[201~", strip_esc(path)).into_bytes()
}

/// Without ESC a pasted text cannot contain (or reassemble) the end-of-paste marker, so it can
/// never break out of the bracket and run as typed keys.
fn strip_esc(s: &str) -> String {
    s.chars().filter(|&c| c != '\x1b').collect()
}

/// Mouse buttons and wheel as xterm numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// A mouse report for the cell (col, row), both 0-based. None when the app did not ask for
/// this kind of report.
pub fn mouse_report(
    button: Option<MouseButton>,
    action: MouseAction,
    m: Modifiers,
    col: usize,
    row: usize,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return None;
    }
    if action == MouseAction::Motion {
        let dragging = button.is_some() && mode.contains(TermMode::MOUSE_DRAG);
        if !(mode.contains(TermMode::MOUSE_MOTION) || dragging) {
            return None;
        }
    }
    let mut b: u32 = match button {
        Some(MouseButton::Left) => 0,
        Some(MouseButton::Middle) => 1,
        Some(MouseButton::Right) => 2,
        Some(MouseButton::WheelUp) => 64,
        Some(MouseButton::WheelDown) => 65,
        None => 3,
    };
    if action == MouseAction::Motion {
        b += 32;
    }
    b += 4 * m.shift as u32 + 8 * m.alt as u32 + 16 * m.ctrl as u32;
    let (x, y) = (col + 1, row + 1);
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if action == MouseAction::Release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{b};{x};{y}{end}").into_bytes());
    }
    if action == MouseAction::Release {
        b = (b & !3) | 3;
    }
    if x > 223 || y > 223 {
        return None;
    }
    Some(vec![0x1b, b'[', b'M', 32 + b as u8, 32 + x as u8, 32 + y as u8])
}

/// Keys that answer a question the agent asked: anything except an `ESC [` sequence longer
/// than two bytes (arrows, focus and mouse reports). Mirrors TerminalPane.Answers.
pub fn answers(bytes: &[u8]) -> bool {
    !(bytes.len() > 2 && bytes[0] == 0x1b && bytes[1] == b'[')
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers::NONE;

    #[test]
    fn arrows_follow_cursor_mode() {
        assert_eq!(encode_key(Key::ArrowUp, NONE, TermMode::NONE).unwrap(), b"\x1b[A");
        assert_eq!(encode_key(Key::ArrowUp, NONE, TermMode::APP_CURSOR).unwrap(), b"\x1bOA");
        assert_eq!(encode_key(Key::ArrowLeft, Modifiers::CTRL, TermMode::NONE).unwrap(), b"\x1b[1;5D");
    }

    #[test]
    fn control_keys() {
        assert_eq!(encode_key(Key::C, Modifiers::CTRL, TermMode::NONE).unwrap(), vec![3]);
        assert_eq!(encode_key(Key::Enter, NONE, TermMode::NONE).unwrap(), b"\r");
        assert_eq!(encode_key(Key::Enter, Modifiers::SHIFT, TermMode::NONE).unwrap(), b"\r");
        assert_eq!(encode_key(Key::Enter, Modifiers::ALT, TermMode::NONE).unwrap(), b"\x1b\r");
        assert_eq!(encode_key(Key::Tab, Modifiers::SHIFT, TermMode::NONE).unwrap(), b"\x1b[Z");
        assert_eq!(encode_key(Key::Backspace, NONE, TermMode::NONE).unwrap(), vec![0x7f]);
        assert_eq!(encode_key(Key::V, Modifiers::ALT, TermMode::NONE).unwrap(), b"\x1bv");
        assert_eq!(encode_key(Key::F5, NONE, TermMode::NONE).unwrap(), b"\x1b[15~");
        assert_eq!(encode_key(Key::Delete, Modifiers::SHIFT, TermMode::NONE).unwrap(), b"\x1b[3;2~");
        assert_eq!(encode_key(Key::A, NONE, TermMode::NONE), None);
    }

    #[test]
    fn pastes() {
        assert_eq!(paste_bytes("a\r\nb\nc", TermMode::NONE), b"a\rb\rc");
        assert_eq!(paste_bytes("x", TermMode::BRACKETED_PASTE), b"\x1b[200~x\x1b[201~");
        assert_eq!(paste_bytes("a\x1b[201~b", TermMode::BRACKETED_PASTE), b"\x1b[200~a[201~b\x1b[201~");
        // A marker that would reassemble after removing an inner one still cannot appear.
        let nested = paste_bytes("\x1b[20\x1b[201~1~x", TermMode::BRACKETED_PASTE);
        assert_eq!(nested.iter().filter(|&&b| b == 0x1b).count(), 2);
        assert_eq!(bracketed_path("C:\\a.png"), b"\x1b[200~C:\\a.png\x1b[201~");
    }

    #[test]
    fn mouse() {
        let sgr = TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE;
        assert_eq!(
            mouse_report(Some(MouseButton::Left), MouseAction::Press, NONE, 4, 1, sgr).unwrap(),
            b"\x1b[<0;5;2M"
        );
        assert_eq!(
            mouse_report(Some(MouseButton::Left), MouseAction::Release, NONE, 4, 1, sgr).unwrap(),
            b"\x1b[<0;5;2m"
        );
        assert_eq!(mouse_report(Some(MouseButton::Left), MouseAction::Press, NONE, 0, 0, TermMode::NONE), None);
        assert_eq!(
            mouse_report(Some(MouseButton::WheelUp), MouseAction::Press, NONE, 0, 0, TermMode::MOUSE_REPORT_CLICK).unwrap(),
            vec![0x1b, b'[', b'M', 96, 33, 33]
        );
        assert_eq!(mouse_report(None, MouseAction::Motion, NONE, 0, 0, sgr), None);
    }

    #[test]
    fn answering() {
        assert!(!answers(b"\x1b[A"));
        assert!(!answers(b"\x1b[I"));
        assert!(answers(b"\x1b"));
        assert!(answers(b"\r"));
        assert!(answers(b"1"));
    }
}
