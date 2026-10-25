//! Localized string table (`FEData/Locale/*.loc`, magic `LOCH`); layouts refer to its ids
//! with `stringID=`/`helptext=`.
//!
//! - `LOCH` header: 0x04 u32 size (0x2c), 0x08 u32 version (1), 0x0c u32 language count,
//!   0x10 u32[count] absolute offsets of one `LOCL` chunk per language
//!   (English, French, German, Italian, Spanish, Swedish, Portuguese in `festrings.loc`).
//! - `LOCI` chunk at 0x2c: u32 size, u32 count, then (after 8 bytes) `count` pairs
//!   `u16 string id, u16 slot`, sorted by id.
//! - `LOCL` chunk: u32 size, u32 language, u32 count, u32[count] slot offsets relative to
//!   the chunk start; each string is NUL-terminated Windows-1252.
//!
//! Evidence: `docs/formats/lay.md` (strings) in the workspace root.
use crate::{bytes, u16le, u32le, Result};

/// `(string id, text)` for one language, in id order.
pub fn parse_loc(input: &[u8], language: usize) -> Result<Vec<(u16, String)>> {
    let d = crate::decompress(input)?;
    if bytes(&d, 0, 4)? != b"LOCH" {
        return Err("offset 0x0: expected LOCH string table".into());
    }
    let languages = u32le(&d, 0x0c)? as usize;
    if language >= languages || languages > 32 {
        return Err(format!("language {language} not in table of {languages}"));
    }
    let index = u32le(&d, 0x04)? as usize;
    if bytes(&d, index, 4)? != b"LOCI" {
        return Err(format!("offset 0x{index:x}: expected LOCI"));
    }
    let count = u32le(&d, index + 8)? as usize;
    let chunk = u32le(&d, 0x10 + language * 4)? as usize;
    if bytes(&d, chunk, 4)? != b"LOCL" {
        return Err(format!("offset 0x{chunk:x}: expected LOCL"));
    }
    if u32le(&d, chunk + 12)? as usize != count {
        return Err("LOCL and LOCI counts differ".into());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let id = u16le(&d, index + 16 + i * 4)?;
        let slot = u16le(&d, index + 18 + i * 4)? as usize;
        if slot >= count {
            return Err(format!("string {id}: slot {slot} out of range"));
        }
        let start = chunk + u32le(&d, chunk + 16 + slot * 4)? as usize;
        let tail = d.get(start..).ok_or("string offset out of range")?;
        let end = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or("unterminated string")?;
        out.push((id, tail[..end].iter().map(|&b| cp1252(b)).collect()));
    }
    Ok(out)
}

fn cp1252(b: u8) -> char {
    const HIGH: [u16; 32] = [
        0x20ac, 0x81, 0x201a, 0x0192, 0x201e, 0x2026, 0x2020, 0x2021, 0x02c6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x8d, 0x017d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013,
        0x2014, 0x02dc, 0x2122, 0x0161, 0x203a, 0x0153, 0x9d, 0x017e, 0x0178,
    ];
    match b {
        0x80..=0x9f => char::from_u32(HIGH[(b - 0x80) as usize] as u32).unwrap_or('?'),
        _ => b as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_strings() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/FEData/Locale/festrings.loc");
        let Ok(d) = std::fs::read(&p) else {
            eprintln!("skip: festrings.loc not installed");
            return;
        };
        let en = parse_loc(&d, 0).unwrap();
        assert_eq!(en.len(), 2846);
        let get = |t: &[(u16, String)], id| t.iter().find(|(i, _)| *i == id).unwrap().1.clone();
        assert_eq!(get(&en, 12400), "main menu");
        assert_eq!(get(&en, 12404), "evolution...");
        let fr = parse_loc(&d, 1).unwrap();
        assert_eq!(get(&fr, 12404), "évolution…");
        assert!(parse_loc(&d, 7).is_err());
    }
}
