//! The window's title bar (Settings › General › Title bar): its colour, backdrop and title.
//! The bar stays Windows' own, so dragging, snapping and resizing are untouched; only the colours
//! DWM lets an app set change (Windows 11). Elsewhere only the title applies.

use kilno_core::settings::{hex_color, KilnoSettings, TITLE_BACKDROPS};

/// The colours offered in Settings, by name; a hex in settings.json shows as Custom.
pub const PRESETS: [(&str, &str); 6] = [
    ("Dark", ""),
    ("Picture", "picture"),
    ("Clay", "#6E3B2C"),
    ("Slate", "#2B3A4A"),
    ("Forest", "#24382C"),
    ("Plum", "#3A2740"),
];

pub const DEFAULT_TITLE: &str = "K I L N O";

/// What the bar should look like.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Look {
    /// None: Windows' dark caption.
    pub color: Option<[u8; 3]>,
    pub text: [u8; 3],
    /// DWMSBT_*: 1 none, 2 mica, 3 acrylic, 4 tabbed.
    pub backdrop: u32,
}

pub fn look(s: &KilnoSettings, picture_average: Option<[u8; 3]>) -> Look {
    let color = match s.title_color.as_str() {
        "picture" => picture_average,
        c => hex_color(c),
    };
    let backdrop = match TITLE_BACKDROPS.iter().position(|b| *b == s.title_backdrop) {
        Some(0) | None => 3,
        Some(1) => 2,
        Some(2) => 4,
        Some(_) => 1,
    };
    Look { color, text: color.map_or([255; 3], readable_on), backdrop }
}

/// Black or white, whichever reads better on `bg` (relative luminance, sRGB weights).
fn readable_on([r, g, b]: [u8; 3]) -> [u8; 3] {
    let y = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    if y > 150.0 { [0; 3] } else { [255; 3] }
}

/// The window's title: the session shown when asked for, else Kilno's.
pub fn title(s: &KilnoSettings, session: Option<&str>) -> String {
    match session.filter(|n| s.title_session && !n.trim().is_empty()) {
        Some(name) => format!("{name} · Kilno"),
        None => DEFAULT_TITLE.to_string(),
    }
}

/// What was last applied, so DWM and the title are touched only on a change.
#[derive(Default)]
pub struct TitleBar {
    look: Option<Look>,
    title: Option<String>,
}

impl TitleBar {
    #[cfg_attr(not(windows), allow(unused_variables))]
    pub fn apply(&mut self, ctx: &egui::Context, hwnd: isize, look: Look, title: String) {
        if self.look != Some(look) {
            self.look = Some(look);
            #[cfg(windows)]
            if hwnd != 0 {
                crate::win::caption_colors(hwnd, look.color, look.text, look.backdrop);
            }
        }
        if self.title.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = Some(title);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(color: &str, backdrop: &str, session: bool) -> KilnoSettings {
        KilnoSettings { title_color: color.into(), title_backdrop: backdrop.into(), title_session: session, ..Default::default() }
    }

    #[test]
    fn colours_backdrops_and_readable_text() {
        let d = look(&KilnoSettings::default(), Some([200, 200, 200]));
        assert_eq!(d, Look { color: None, text: [255; 3], backdrop: 3 }, "default: dark caption, acrylic");
        let p = look(&with("picture", "mica", false), Some([230, 220, 210]));
        assert_eq!((p.color, p.text, p.backdrop), (Some([230, 220, 210]), [0; 3], 2), "a light picture: black text");
        assert_eq!(look(&with("picture", "none", false), None).color, None, "no picture: stays dark");
        let h = look(&with("#2B3A4A", "tabbed", false), None);
        assert_eq!((h.color, h.text, h.backdrop), (Some([0x2B, 0x3A, 0x4A]), [255; 3], 4));
    }

    #[test]
    fn title_names_the_session_only_when_asked() {
        assert_eq!(title(&with("", "acrylic", true), Some("fix login")), "fix login · Kilno");
        assert_eq!(title(&with("", "acrylic", false), Some("fix login")), DEFAULT_TITLE);
        assert_eq!(title(&with("", "acrylic", true), None), DEFAULT_TITLE);
        assert_eq!(title(&with("", "acrylic", true), Some("  ")), DEFAULT_TITLE);
    }
}
