//! Front-end bitmap font (`FEData/Art/*.ffn`, magic `FNTF`).
//!
//! Layout (from the shipped Univers fonts; all little-endian):
//! - 0x00 `FNTF`, 0x04 u32 file size, 0x08 u16 version (0x65), 0x0a u16 glyph count,
//!   0x13 u8 line height in pixels, 0x1c u32 offset of the glyph atlas.
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
    pub line_height: u8,
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
    let line_height = bytes(&d, 0x13, 1)?[0];
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
    fn installed_fonts_parse() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/FEData/Art");
        let Ok(d) = std::fs::read(dir.join("Univers14b.ffn")) else {
            eprintln!("skip: FEData/Art not installed");
            return;
        };
        let f = parse_ffn(&d).unwrap();
        assert_eq!(f.glyphs.len(), 224);
        assert_eq!(f.line_height, 0x17);
        assert_eq!((f.atlas_width, f.atlas_height), (256, 197));
        let hash = f.glyph(b'#' as u16).unwrap();
        assert_eq!(
            (hash.width, hash.height, hash.x, hash.y, hash.advance),
            (12, 15, 2, 95, 12)
        );
        // Row 96 of the atlas: "+@*" at x 6..8 (high nibble first), blank at x 5.
        assert!(f.alpha[96 * 256 + 7] > 0xc0 && f.alpha[96 * 256 + 5] == 0);
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".ffn") {
                let font = parse_ffn(&std::fs::read(entry.path()).unwrap())
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
                assert!(font.glyph(b'A' as u16).is_some(), "{name}");
            }
        }
    }
}
