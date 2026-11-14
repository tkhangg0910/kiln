//! Colours of the C# terminal (Assets/terminal.html): Kilno's background/foreground/cursor and
//! black/bright black, xterm.js's Tango defaults for the other ANSI colours, the xterm 6x6x6
//! cube and grey ramp for 16-255. Truecolor passes through.

use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

pub const BACKGROUND: Rgb = rgb(0x14171c);
pub const FOREGROUND: Rgb = rgb(0xc8cdd6);
pub const CURSOR: Rgb = rgb(0xd4a24c);
pub const CURSOR_ACCENT: Rgb = rgb(0x14171c);
pub const SELECTION: Rgb = rgb(0x2f3947);
pub const SCROLLBAR: Rgb = rgb(0x333a45);
pub const SCROLLBAR_HOVER: Rgb = rgb(0x464f5d);

const fn rgb(v: u32) -> Rgb {
    Rgb { r: (v >> 16) as u8, g: (v >> 8) as u8, b: v as u8 }
}

const ANSI: [Rgb; 16] = [
    rgb(0x14171c), // black: Kilno's background
    rgb(0xcc0000),
    rgb(0x4e9a06),
    rgb(0xc4a000),
    rgb(0x3465a4),
    rgb(0x75507b),
    rgb(0x06989a),
    rgb(0xd3d7cf),
    rgb(0x5a6472), // bright black: Kilno's
    rgb(0xef2929),
    rgb(0x8ae234),
    rgb(0xfce94f),
    rgb(0x729fcf),
    rgb(0xad7fa8),
    rgb(0x34e2e2),
    rgb(0xeeeeec),
];

/// The colour for palette index `i` (0-255) or a named slot (256+), before any OSC override.
pub fn default_rgb(i: usize) -> Rgb {
    match i {
        0..=15 => ANSI[i],
        16..=231 => {
            let n = i - 16;
            let step = |v: usize| if v == 0 { 0 } else { (v * 40 + 55) as u8 };
            Rgb { r: step(n / 36), g: step((n / 6) % 6), b: step(n % 6) }
        }
        232..=255 => {
            let v = (8 + (i - 232) * 10) as u8;
            Rgb { r: v, g: v, b: v }
        }
        x if x == NamedColor::Foreground as usize || x == NamedColor::BrightForeground as usize => FOREGROUND,
        x if x == NamedColor::Background as usize => BACKGROUND,
        x if x == NamedColor::Cursor as usize => CURSOR,
        x if x == NamedColor::DimForeground as usize => dim(FOREGROUND),
        x if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&x) => {
            dim(ANSI[x - NamedColor::DimBlack as usize])
        }
        _ => FOREGROUND,
    }
}

fn dim(c: Rgb) -> Rgb {
    Rgb { r: (c.r as u16 * 2 / 3) as u8, g: (c.g as u16 * 2 / 3) as u8, b: (c.b as u16 * 2 / 3) as u8 }
}

/// Resolve a cell colour, honouring colours the app changed with OSC 4/10/11.
pub fn resolve(c: Color, overrides: &Colors) -> Rgb {
    let idx = match c {
        Color::Spec(rgb) => return rgb,
        Color::Indexed(i) => i as usize,
        Color::Named(n) => n as usize,
    };
    overrides[idx].unwrap_or_else(|| default_rgb(idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_and_ramp() {
        assert_eq!(default_rgb(16), Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(default_rgb(231), Rgb { r: 255, g: 255, b: 255 });
        assert_eq!(default_rgb(196), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(default_rgb(232), Rgb { r: 8, g: 8, b: 8 });
        assert_eq!(default_rgb(255), Rgb { r: 238, g: 238, b: 238 });
        assert_eq!(default_rgb(NamedColor::Background as usize), BACKGROUND);
    }

    #[test]
    fn truecolor_passes() {
        let c = Colors::default();
        assert_eq!(resolve(Color::Spec(Rgb { r: 1, g: 2, b: 3 }), &c), Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(resolve(Color::Named(NamedColor::Foreground), &c), FOREGROUND);
    }
}
