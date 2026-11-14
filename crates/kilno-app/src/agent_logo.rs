//! The agents' marks where "CC" / "CX" used to be: Claude's spark for Claude Code, OpenAI's
//! mark for Codex. White-on-transparent PNGs (rendered at 128 px from Simple Icons' SVGs, see
//! res/agents/README.md), tinted with the agent's colour where they are drawn.

use egui::{Color32, ColorImage, Context, Painter, Pos2, Rect, TextureFilter, TextureHandle, TextureOptions};
use kilno_core::agent::AgentKind;
use std::sync::OnceLock;

const CLAUDE: &[u8] = include_bytes!("../res/agents/claude.png");
const CODEX: &[u8] = include_bytes!("../res/agents/codex.png");

/// The agent a "CC" / "CX" tag ([`AgentKind::tag`]) names.
pub fn kind_of(tag: &str) -> AgentKind {
    if tag == AgentKind::Codex.tag() {
        AgentKind::Codex
    } else {
        AgentKind::Claude
    }
}

/// `kind`'s mark filling `rect` (square), in `tint`.
pub fn paint(p: &Painter, rect: Rect, kind: AgentKind, tint: Color32) {
    let [claude, codex] = textures(p.ctx());
    let tex = if kind == AgentKind::Codex { codex } else { claude };
    p.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
}

fn textures(ctx: &Context) -> &'static [TextureHandle; 2] {
    static T: OnceLock<[TextureHandle; 2]> = OnceLock::new();
    T.get_or_init(|| [load(ctx, "agent-claude", CLAUDE), load(ctx, "agent-codex", CODEX)])
}

fn load(ctx: &Context, name: &str, png: &[u8]) -> TextureHandle {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).expect("embedded PNG").to_rgba8();
    let image = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    // Drawn at 12-36 px from 128: mipmaps keep the small ones from shimmering.
    ctx.load_texture(name, image, TextureOptions { mipmap_mode: Some(TextureFilter::Linear), ..TextureOptions::LINEAR })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_marks_decode_with_transparency() {
        for png in [CLAUDE, CODEX] {
            let img = image::load_from_memory(png).unwrap().to_rgba8();
            assert_eq!((img.width(), img.height()), (128, 128));
            assert!(img.pixels().any(|p| p[3] == 0) && img.pixels().any(|p| p[3] == 255));
        }
        assert_eq!(kind_of("CX"), AgentKind::Codex);
        assert_eq!(kind_of("CC"), AgentKind::Claude);
    }
}
