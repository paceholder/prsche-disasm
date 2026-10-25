//! Front-end bitmap font (`FEData/Art/*.ffn`, magic `FNTF`).
//!
//! Layout (from the shipped Univers fonts; all little-endian):
//! - 0x00 `FNTF`, 0x04 u32 file size, 0x08 u16 version (0x64 or 0x65), 0x0a u16 glyph count,
//!   0x12/0x13 signed height/gap bytes (their sum is the line step), 0x1c u32 atlas offset.
//! - 0x20: glyph records of 11 bytes: u16 code (Latin-1), u8 width, u8 height,
//!   u16 atlas x, u16 atlas y, u8 advance, i8 x offset, i8 y offset (from the line top).
//! - atlas: one FSH shape record (code 0x7a: 4-bit alpha, high nibble = left pixel)
//!   with the usual 16-byte header (u16 width at +4, u16 height at +6).
//!
//! Evidence: `docs/formats/ffn.md` in the workspace root.
use crate::{bytes, u16le, u32le, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub code: u16,
    pub width: u8,
    pub height: u8,
    pub x: u16,
    pub y: u16,
    pub advance: u8,
    pub x_offset: i8,
    pub y_offset: i8,
}

#[derive(Debug, Clone)]
pub struct Font {
    /// Signed line step: FONT loader 0x53C70A..0x53C71A adds header +0x12/+0x13;
    /// newline handling 0x53DB6B..0x53DB77 reads that sum, scaled, from font+0x14.
    pub line_height: i32,
    pub glyphs: Vec<Glyph>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// One alpha byte (0..255) per atlas pixel.
    pub alpha: Vec<u8>,
}

impl Font {
    pub fn glyph(&self, code: u16) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.code == code)
    }
}

pub fn parse_ffn(input: &[u8]) -> Result<Font> {
    let d = crate::decompress(input)?;
    if bytes(&d, 0, 4)? != b"FNTF" {
        return Err("offset 0x0: expected FNTF font".into());
    }
    let count = u16le(&d, 0x0a)? as usize;
    let metrics = bytes(&d, 0x12, 2)?;
    let line_height = metrics[0] as i8 as i32 + metrics[1] as i8 as i32;
    let atlas = u32le(&d, 0x1c)? as usize;
    let mut glyphs = Vec::with_capacity(count);
    for i in 0..count {
        let r = bytes(&d, 0x20 + i * 11, 11)?;
        glyphs.push(Glyph {
            code: u16::from_le_bytes([r[0], r[1]]),
            width: r[2],
            height: r[3],
            x: u16::from_le_bytes([r[4], r[5]]),
            y: u16::from_le_bytes([r[6], r[7]]),
            advance: r[8],
            x_offset: r[9] as i8,
            y_offset: r[10] as i8,
        });
    }
    let code = bytes(&d, atlas, 1)?[0];
    if code != 0x7a {
        return Err(format!(
            "offset 0x{atlas:x}: atlas format 0x{code:02x}, expected 0x7a"
        ));
    }
    let w = u16le(&d, atlas + 4)? as usize;
    let h = u16le(&d, atlas + 6)? as usize;
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err("invalid font atlas size".into());
    }
    let packed = bytes(&d, atlas + 16, (w * h).div_ceil(2))?;
    let mut alpha = vec![0u8; w * h];
    for (i, a) in alpha.iter_mut().enumerate() {
        let b = packed[i / 2];
        let nibble = if i % 2 == 0 { b >> 4 } else { b & 15 };
        *a = nibble * 17;
    }
    Ok(Font {
        line_height,
        glyphs,
        atlas_width: w as u32,
        atlas_height: h as u32,
        alpha,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_step_adds_signed_header_metrics() {
        // One two-pixel glyph; glyph height 1 is independent of line spacing.
        let atlas = 0x20 + 11;
        let mut data = vec![0u8; atlas + 17];
        data[..4].copy_from_slice(b"FNTF");
        let size = data.len() as u32;
        data[4..8].copy_from_slice(&size.to_le_bytes());
        data[0x0a..0x0c].copy_from_slice(&1u16.to_le_bytes());
        data[0x14..0x18].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1c..0x20].copy_from_slice(&(atlas as u32).to_le_bytes());
        data[0x20..0x2b].copy_from_slice(&[b'A', 0, 2, 1, 0, 0, 0, 0, 2, 0, 0]);
        data[atlas] = 0x7a;
        data[atlas + 4..atlas + 6].copy_from_slice(&2u16.to_le_bytes());
        data[atlas + 6..atlas + 8].copy_from_slice(&1u16.to_le_bytes());
        data[atlas + 16] = 0xf1;
        for version in [0x64u16, 0x65] {
            data[8..10].copy_from_slice(&version.to_le_bytes());
            for (height, gap, step) in [(21i8, 0i8, 21), (0, 23, 23), (23, -2, 21), (-3, 2, -1)] {
                data[0x12] = height as u8;
                data[0x13] = gap as u8;
                let font = parse_ffn(&data).unwrap();
                assert_eq!(font.line_height, step);
                assert_eq!(font.glyph(b'A' as u16).unwrap().height, 1);
                assert_eq!(font.alpha, [255, 17]);
            }
        }
    }

    #[test]
    fn installed_fonts_parse() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/FEData/Art");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!("skip: FEData/Art not installed");
            return;
        };
        // Different installed font revisions have different glyph rectangles,
        // advances and atlas heights; do not require one particular artwork.
        let mut count = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".ffn") {
                let font = parse_ffn(&std::fs::read(entry.path()).unwrap())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                count += 1;
                assert!(font.line_height > 0, "{name}");
                assert_eq!(font.alpha.len(), (font.atlas_width * font.atlas_height) as usize);
                assert!(font.alpha.iter().any(|&a| a > 0), "{name}");
                assert!(font.glyph(b'A' as u16).is_some(), "{name}");
            }
        }
        assert!(count > 0, "no FFN fonts in {}", dir.display());
    }
}
