//! Screen-sprite effects of `GameData/Render/flare.dat` and the lens-flare layout
//! `flarepos.dat` (`docs/re/lighting.md`, "Sun and lens flare").
//!
//! `flare.dat` is a raw array of 0x8c-byte records, copied as-is by the loader
//! `FlareDb_Load` 0x4522b0 and looked up by tag (`FlareDb_Find` 0x452260); a record
//! is drawn by `Flare_DrawSprite` 0x451710. `flarepos.dat` holds 14 floats, the
//! position of each sun sprite on the line from the screen centre through the sun
//! (`SunFlare_Init` 0x460780 falls back to the same table built in when the file is
//! missing).

use crate::{f32le, u32le, Result};

/// Size of one `flare.dat` record.
pub const FLARE_RECORD_SIZE: usize = 0x8c;

/// Sun sprite positions of `SunFlare_Init` 0x460780 (and `flarepos.dat`):
/// sprite `i` sits at `centre + (sun − centre)·t[i]`.
pub const DEFAULT_FLARE_POSITIONS: [f32; 14] =
    [1.0, 0.75, -0.75, 0.5, -0.5, 0.25, -0.25, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0];

/// Record flag bit 2: the sprite turns by `(x + y)/1440` turns at screen point (x, y).
pub const FLARE_FLAG_ROTATE: u32 = 4;

/// One `flare.dat` record.
#[derive(Debug, Clone, PartialEq)]
pub struct FlareRecord {
    /// `+0x00` tag, read as text (`suna` is the sun, `sunb`..`sunn` the flare sprites).
    pub tag: String,
    /// `+0x04`, `+0x08` texture names (`common.fsh`), `None` for `null`.
    pub textures: [Option<String>; 2],
    /// `+0x1c`, `+0x20` ARGB colour per texture pass.
    pub colors: [u32; 2],
    /// `+0x34`, `+0x38` sprite size per texture pass: half extent `size·0.1` pixels
    /// of a 640×480 screen.
    pub sizes: [f32; 2],
    /// `+0x4c` flags (bit 2: rotate with the screen position).
    pub flags: u32,
    /// `+0x50` blink phase, `+0x54` on time, `+0x58` off time (ticks); on = off = 0
    /// means always shown.
    pub blink: [u32; 3],
}

/// A FourCC stored little-endian, as text (`b"anus"` → `"suna"`).
fn fourcc(data: &[u8], offset: usize) -> Result<String> {
    let value = u32le(data, offset)?;
    Ok(value.to_be_bytes().iter().map(|&b| b as char).collect::<String>().trim_end().to_string())
}

/// Parse `flare.dat`.
pub fn parse_flare_dat(data: &[u8]) -> Result<Vec<FlareRecord>> {
    if data.len() % FLARE_RECORD_SIZE != 0 {
        return Err(format!("flare.dat: {} bytes is not a multiple of 0x8c", data.len()));
    }
    (0..data.len() / FLARE_RECORD_SIZE)
        .map(|i| {
            let r = &data[i * FLARE_RECORD_SIZE..(i + 1) * FLARE_RECORD_SIZE];
            let texture = |o| fourcc(r, o).map(|t| (t != "null").then_some(t));
            Ok(FlareRecord {
                tag: fourcc(r, 0)?,
                textures: [texture(4)?, texture(8)?],
                colors: [u32le(r, 0x1c)?, u32le(r, 0x20)?],
                sizes: [f32le(r, 0x34)?, f32le(r, 0x38)?],
                flags: u32le(r, 0x4c)?,
                blink: [u32le(r, 0x50)?, u32le(r, 0x54)?, u32le(r, 0x58)?],
            })
        })
        .collect()
}

/// Parse `flarepos.dat`: 14 floats; a short file keeps the built-in table
/// for the rest (the original copies 0x38 bytes from the file).
pub fn parse_flare_positions(data: &[u8]) -> [f32; 14] {
    let mut out = DEFAULT_FLARE_POSITIONS;
    for (i, value) in out.iter_mut().enumerate() {
        if let Ok(v) = f32le(data, i * 4) {
            *value = v;
        }
    }
    out
}

/// Sun and lens-flare data of `GameData/Render`.
#[derive(Debug, Clone, PartialEq)]
pub struct FlareData {
    pub records: Vec<FlareRecord>,
    pub positions: [f32; 14],
}

impl FlareData {
    pub fn find(&self, tag: &str) -> Option<&FlareRecord> {
        self.records.iter().find(|r| r.tag == tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(tag: &[u8; 4], textures: [&[u8; 4]; 2], colors: [u32; 2], sizes: [f32; 2]) -> Vec<u8> {
        let mut r = vec![0u8; FLARE_RECORD_SIZE];
        let mut put = |o: usize, b: &[u8]| r[o..o + b.len()].copy_from_slice(b);
        // Stored little-endian: the text reads backwards in the file.
        let rev = |t: &[u8; 4]| [t[3], t[2], t[1], t[0]];
        put(0, &rev(tag));
        put(4, &rev(textures[0]));
        put(8, &rev(textures[1]));
        put(0x1c, &colors[0].to_le_bytes());
        put(0x20, &colors[1].to_le_bytes());
        put(0x34, &sizes[0].to_le_bytes());
        put(0x38, &sizes[1].to_le_bytes());
        put(0x4c, &FLARE_FLAG_ROTATE.to_le_bytes());
        r
    }

    #[test]
    fn records_read_tags_textures_colours_and_sizes() {
        let mut data = record(b"suna", [b"SUNA", b"BLUR"], [0xffff_ffff, 0x33ff_3301], [205.0, 600.0]);
        data.extend(record(b"sunb", [b"lens", b"null"], [0x18ff_351e, 0xffff_ffff], [600.0, 100.0]));
        let flare = FlareData { records: parse_flare_dat(&data).unwrap(), positions: DEFAULT_FLARE_POSITIONS };
        let sun = flare.find("suna").unwrap();
        assert_eq!(sun.textures, [Some("SUNA".into()), Some("BLUR".into())]);
        assert_eq!(sun.colors, [0xffff_ffff, 0x33ff_3301]);
        assert_eq!(sun.sizes, [205.0, 600.0]);
        assert_eq!(sun.flags, FLARE_FLAG_ROTATE);
        assert_eq!(flare.find("sunb").unwrap().textures, [Some("lens".into()), None]);
        assert!(parse_flare_dat(&data[1..]).is_err());
    }

    #[test]
    fn positions_default_to_the_built_in_table() {
        assert_eq!(parse_flare_positions(&[]), DEFAULT_FLARE_POSITIONS);
        let mut data = vec![0u8; 8];
        data[4..8].copy_from_slice(&0.6f32.to_le_bytes());
        let p = parse_flare_positions(&data);
        assert_eq!(&p[..3], &[0.0, 0.6, -0.75]);
    }

    #[test]
    fn local_flare_dat_has_the_sun_and_its_flare() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../local/game/GameData/Render");
        let (Ok(data), Ok(pos)) = (std::fs::read(root.join("flare.dat")), std::fs::read(root.join("flarepos.dat")))
        else {
            return;
        };
        let flare = FlareData { records: parse_flare_dat(&data).unwrap(), positions: parse_flare_positions(&pos) };
        assert_eq!(flare.records.len(), 39);
        assert_eq!(flare.positions, DEFAULT_FLARE_POSITIONS);
        let sun = flare.find("suna").unwrap();
        assert_eq!(sun.textures, [Some("SUNA".into()), Some("BLUR".into())]);
        // Flare sprites sunb..suni; sunj..sunn are absent (skipped by the draw).
        assert!(flare.find("suni").is_some() && flare.find("sunj").is_none());
    }
}
