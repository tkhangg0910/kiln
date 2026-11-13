//! Port of checks/mascot.cs: renders the held mascot (idle and peek at 256 px, idle at 44 px)
//! on the island's dark background, for comparing by eye with the design.
//!
//! `cargo run -p kiln-island --example mascot_render [out-dir]` (default %TEMP%\kiln-mascot).

use kiln_island::mascot::{self, MascotMove};
use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

fn write(pm: &Pixmap, path: &PathBuf) -> std::io::Result<()> {
    let mut enc = png::Encoder::new(std::fs::File::create(path)?, pm.width(), pm.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    // Opaque background, so premultiplied is the colour.
    w.write_image_data(pm.data()).map_err(std::io::Error::other)?;
    w.finish().map_err(std::io::Error::other)
}

fn main() -> std::io::Result<()> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("kiln-mascot"));
    std::fs::create_dir_all(&out)?;
    for (name, mv, size) in [
        ("idle.png", MascotMove::Idle, 256),
        ("peek.png", MascotMove::Peek, 256),
        ("idle-44.png", MascotMove::Idle, 44),
    ] {
        let figure = mascot::render(mv, None, size);
        let mut pm = Pixmap::new(size, size).expect("size");
        pm.fill(Color::from_rgba8(0x0f, 0x11, 0x14, 255));
        pm.draw_pixmap(0, 0, figure.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        let path = out.join(name);
        write(&pm, &path)?;
        println!("{}", path.display());
    }
    Ok(())
}
