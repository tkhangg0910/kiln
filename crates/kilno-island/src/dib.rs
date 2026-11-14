//! A clipboard DIB (CF_DIB / CF_DIBV5: a BITMAPINFOHEADER or V4/V5 header, optional colour
//! masks and palette, then the pixels) to a PNG. The alpha channel is dropped, as the C#
//! converted to Bgr32: a copied bitmap often carries an alpha of 0 that means nothing and would
//! save as a fully transparent picture.

use std::io::Write;

#[derive(Debug, PartialEq, Eq)]
pub enum DibError {
    Truncated,
    Unsupported(&'static str),
    Png(String),
}

impl std::fmt::Display for DibError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DibError::Truncated => write!(f, "the bitmap is shorter than its header says"),
            DibError::Unsupported(what) => write!(f, "unsupported bitmap: {what}"),
            DibError::Png(e) => write!(f, "png: {e}"),
        }
    }
}

impl std::error::Error for DibError {}

/// An RGB image, 3 bytes a pixel, top row first.
#[derive(Debug, PartialEq, Eq)]
pub struct Rgb {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const BI_ALPHABITFIELDS: u32 = 6;

fn u16_at(b: &[u8], at: usize) -> Result<u16, DibError> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or(DibError::Truncated)
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, DibError> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(DibError::Truncated)
}

/// A channel from `v` by its mask, scaled to 8 bits.
fn channel(v: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let bits = (mask >> shift).count_ones();
    let x = (v & mask) >> shift;
    let max = (1u64 << bits) - 1;
    ((u64::from(x) * 255 + max / 2) / max) as u8
}

/// Decodes a packed DIB to RGB.
pub fn decode(dib: &[u8]) -> Result<Rgb, DibError> {
    let header = u32_at(dib, 0)? as usize;
    if header < 40 {
        return Err(DibError::Unsupported("OS/2 core header"));
    }
    let width = u32_at(dib, 4)? as i32;
    let height = u32_at(dib, 8)? as i32;
    let bpp = u16_at(dib, 14)?;
    let compression = u32_at(dib, 16)?;
    let used = u32_at(dib, 32)? as usize;
    if width <= 0 || height == 0 || width > 1 << 15 || height.unsigned_abs() > 1 << 15 {
        return Err(DibError::Unsupported("size"));
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let top_down = height < 0;

    let mut at = header;
    let masks = match (compression, bpp) {
        (BI_BITFIELDS | BI_ALPHABITFIELDS, 16 | 32) => {
            // V4/V5 headers carry the masks inside; a plain info header has them after it.
            if header >= 52 {
                (u32_at(dib, 40)?, u32_at(dib, 44)?, u32_at(dib, 48)?)
            } else {
                let m = (u32_at(dib, at)?, u32_at(dib, at + 4)?, u32_at(dib, at + 8)?);
                at += if compression == BI_ALPHABITFIELDS { 16 } else { 12 };
                m
            }
        }
        (BI_RGB, 16) => (0x7c00, 0x03e0, 0x001f),
        (BI_RGB, 32) => (0x00ff_0000, 0x0000_ff00, 0x0000_00ff),
        (BI_RGB, 1 | 4 | 8 | 24) => (0, 0, 0),
        (BI_RGB, _) => return Err(DibError::Unsupported("bit depth")),
        _ => return Err(DibError::Unsupported("compression")),
    };
    let palette_len = if bpp <= 8 {
        if used == 0 {
            1usize << bpp
        } else {
            used
        }
    } else {
        used
    };
    let palette = at;
    at += palette_len * 4;
    if bpp <= 8 && dib.len() < at {
        return Err(DibError::Truncated);
    }

    let stride = (w * usize::from(bpp)).div_ceil(32) * 4;
    let pixels = dib.get(at..).ok_or(DibError::Truncated)?;
    if pixels.len() < stride * h {
        return Err(DibError::Truncated);
    }

    let mut out = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let src_row = if top_down { y } else { h - 1 - y };
        let row = &pixels[src_row * stride..src_row * stride + stride];
        for x in 0..w {
            let (r, g, b) = match bpp {
                24 => (row[x * 3 + 2], row[x * 3 + 1], row[x * 3]),
                32 => {
                    let v = u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]);
                    (channel(v, masks.0), channel(v, masks.1), channel(v, masks.2))
                }
                16 => {
                    let v = u32::from(u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]));
                    (channel(v, masks.0), channel(v, masks.1), channel(v, masks.2))
                }
                _ => {
                    let per = 8 / usize::from(bpp);
                    let byte = row[x / per];
                    let shift = 8 - usize::from(bpp) * (x % per + 1);
                    let idx = usize::from(byte >> shift) & ((1 << bpp) - 1);
                    if idx >= palette_len {
                        (0, 0, 0)
                    } else {
                        let p = palette + idx * 4;
                        (dib[p + 2], dib[p + 1], dib[p])
                    }
                }
            };
            out.extend_from_slice(&[r, g, b]);
        }
    }
    Ok(Rgb {
        width: w as u32,
        height: h as u32,
        data: out,
    })
}

/// Writes `img` as an 8-bit RGB PNG.
pub fn write_png(img: &Rgb, out: impl Write) -> Result<(), DibError> {
    let mut enc = png::Encoder::new(out, img.width, img.height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| DibError::Png(e.to_string()))?;
    w.write_image_data(&img.data).map_err(|e| DibError::Png(e.to_string()))?;
    w.finish().map_err(|e| DibError::Png(e.to_string()))
}

/// A DIB straight to PNG bytes.
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, DibError> {
    let img = decode(dib)?;
    let mut buf = Vec::new();
    write_png(&img, &mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(size: u32, w: i32, h: i32, bpp: u16, compression: u32, used: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&bpp.to_le_bytes());
        v.extend_from_slice(&compression.to_le_bytes());
        v.extend_from_slice(&[0; 12]); // size image, ppm x/y
        v.extend_from_slice(&used.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.resize(size as usize, 0);
        v
    }

    #[test]
    fn bottom_up_32_bit_with_zero_alpha_is_opaque_rgb() {
        // 2x2, bottom row first: blue, green / red, white. Alpha 0 everywhere.
        let mut d = header(40, 2, 2, 32, BI_RGB, 0);
        for bgra in [[255, 0, 0, 0], [0, 255, 0, 0], [0, 0, 255, 0], [255, 255, 255, 0]] {
            d.extend_from_slice(&bgra);
        }
        let img = decode(&d).expect("decodes");
        assert_eq!((img.width, img.height), (2, 2));
        assert_eq!(img.data, vec![255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0], "red white / blue green");

        let png = dib_to_png(&d).expect("png");
        let dec = png::Decoder::new(std::io::Cursor::new(png));
        let mut r = dec.read_info().expect("info");
        let mut buf = vec![0; r.output_buffer_size().expect("size")];
        let info = r.next_frame(&mut buf).expect("frame");
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(&buf[..info.buffer_size()], &img.data[..]);
    }

    #[test]
    fn top_down_24_bit_rows_are_padded() {
        // 1x2 top-down: each row 3 bytes + 1 padding.
        let mut d = header(40, 1, -2, 24, BI_RGB, 0);
        d.extend_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
        let img = decode(&d).expect("decodes");
        assert_eq!(img.data, vec![3, 2, 1, 6, 5, 4]);
    }

    #[test]
    fn bitfields_after_an_info_header_and_inside_a_v5_header() {
        let mut d = header(40, 1, 1, 32, BI_BITFIELDS, 0);
        for m in [0x0000_ff00u32, 0x00ff_0000, 0xff00_0000] {
            d.extend_from_slice(&m.to_le_bytes());
        }
        d.extend_from_slice(&[0, 10, 20, 30]);
        assert_eq!(decode(&d).expect("decodes").data, vec![10, 20, 30]);

        let mut v5 = header(124, 1, 1, 32, BI_BITFIELDS, 0);
        v5[40..44].copy_from_slice(&0x00ff_0000u32.to_le_bytes());
        v5[44..48].copy_from_slice(&0x0000_ff00u32.to_le_bytes());
        v5[48..52].copy_from_slice(&0x0000_00ffu32.to_le_bytes());
        v5.extend_from_slice(&[7, 8, 9, 0]);
        assert_eq!(decode(&v5).expect("decodes").data, vec![9, 8, 7]);
    }

    #[test]
    fn sixteen_bit_565_and_paletted() {
        let mut d = header(40, 1, 1, 16, BI_BITFIELDS, 0);
        for m in [0xf800u32, 0x07e0, 0x001f] {
            d.extend_from_slice(&m.to_le_bytes());
        }
        d.extend_from_slice(&0xf800u16.to_le_bytes());
        d.extend_from_slice(&[0, 0]);
        assert_eq!(decode(&d).expect("decodes").data, vec![255, 0, 0]);

        let mut p = header(40, 3, 1, 8, BI_RGB, 2);
        p.extend_from_slice(&[0, 0, 0, 0, 10, 20, 30, 0]);
        p.extend_from_slice(&[1, 0, 1, 0]);
        assert_eq!(decode(&p).expect("decodes").data, vec![30, 20, 10, 0, 0, 0, 30, 20, 10]);
    }

    #[test]
    fn short_or_odd_bitmaps_are_refused() {
        assert_eq!(decode(&[1, 2]), Err(DibError::Truncated));
        let d = header(40, 4, 4, 32, BI_RGB, 0);
        assert_eq!(decode(&d), Err(DibError::Truncated));
        let jpeg = header(40, 1, 1, 0, 4, 0);
        assert!(matches!(decode(&jpeg), Err(DibError::Unsupported(_))));
    }
}
