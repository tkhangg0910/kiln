//! Port of checks/mascot.cs: renders the held mascot (idle and peek at 256 px, idle at 44 px)
//! on the island's dark background, for comparing by eye with the design. Also writes the app
//! icon and the README logo from the same drawing: `kilno.ico` (256, 64, 48, 32, 24, 16 px,
//! PNG entries) for crates/kilno-app/res/, and a transparent 256 px `logo.png` for docs/assets/.
//!
//! `cargo run -p kilno-island --example mascot_render [out-dir]` (default %TEMP%\kilno-mascot).

use kilno_island::mascot::{self, MascotMove};
use std::io::Write;
use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

/// PNG of `pm`. tiny-skia is premultiplied; PNG wants straight alpha.
fn png(pm: &Pixmap) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::with_capacity(pm.data().len());
    for p in pm.pixels() {
        let c = p.demultiply();
        data.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, pm.width(), pm.height());
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(&data).map_err(std::io::Error::other)?;
    w.finish().map_err(std::io::Error::other)?;
    Ok(out)
}

/// An .ico whose entries are PNGs (Windows Vista and later read these at every size).
fn ico(sizes: &[u32]) -> std::io::Result<Vec<u8>> {
    let pngs = sizes
        .iter()
        .map(|&s| png(&mascot::render(MascotMove::Idle, None, s)))
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    out.write_all(&[0, 0, 1, 0])?; // reserved, type 1 = icon
    out.write_all(&(sizes.len() as u16).to_le_bytes())?;
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, data) in sizes.iter().zip(&pngs) {
        let dim = if s >= 256 { 0 } else { s as u8 }; // 0 means 256
        out.write_all(&[dim, dim, 0, 0])?; // no palette, reserved
        out.write_all(&1u16.to_le_bytes())?; // colour planes
        out.write_all(&32u16.to_le_bytes())?; // bits per pixel
        out.write_all(&(data.len() as u32).to_le_bytes())?;
        out.write_all(&offset.to_le_bytes())?;
        offset += data.len() as u32;
    }
    for data in &pngs {
        out.write_all(data)?;
    }
    Ok(out)
}

fn main() -> std::io::Result<()> {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("kilno-mascot"));
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
        std::fs::write(&path, png(&pm)?)?;
        println!("{}", path.display());
    }
    let logo = out.join("logo.png");
    std::fs::write(&logo, png(&mascot::render(MascotMove::Idle, None, 256))?)?;
    println!("{}", logo.display());
    let icon = out.join("kilno.ico");
    std::fs::write(&icon, ico(&[256, 64, 48, 32, 24, 16])?)?;
    println!("{}", icon.display());
    Ok(())
}
