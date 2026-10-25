//! Engine-sound tables shipped next to each engine bank: `CRDl` curve files
//! (`.ect`, `.elt`, `.cct`, `.clt`) and the per-car `.spl` mix parameters.
//!
//! Format notes and evidence: `docs/formats/engine-sound-curves.md` in the research workspace.
//! Loader: original `0x4acc10`; per-frame use: `0x4ac780` and `0x4ac290`.
use crate::{bytes, u32le, Result};

pub const CRD_MAGIC: &[u8; 4] = b"CRDl";
pub const CRD_LAYERS: usize = 8;
/// Curve length: x = RPM bin, 0..=511 (`0x4ac780` clamps to 0x1ff).
pub const CRD_BINS: usize = 512;
const SLOTS_AT: usize = 0x20;
const VOLUME_PTRS_AT: usize = 0x128;
const PITCH_PTRS_AT: usize = 0x148;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrdLayer {
    /// Bank slot (patch id) this layer plays.
    pub slot: u8,
    /// Volume 0..127 per RPM bin; the only curve the game reads.
    pub volume: Vec<u8>,
    /// Second 512-byte curve, rising roughly x/4. Relocated but never read by the game.
    pub aux: Vec<u8>,
}

impl CrdLayer {
    /// Bins the game keeps: from the first nonzero entry up to (excluding) the next zero
    /// (`0x4acc10`). Outside this range the layer is silent.
    pub fn active_range(&self) -> std::ops::Range<usize> {
        let start = self.volume.iter().position(|&v| v != 0).unwrap_or(CRD_BINS);
        let end = self.volume[start..]
            .iter()
            .position(|&v| v == 0)
            .map_or(CRD_BINS, |n| start + n);
        start..end
    }
    /// Curve value as the game sees it (0 outside `active_range`).
    pub fn volume_at(&self, bin: usize) -> u8 {
        if self.active_range().contains(&bin) {
            self.volume[bin]
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrdTable {
    /// Byte 4; the game requires 2.
    pub version: u8,
    /// Bytes 0x08..0x20 (eight little-endian u16/u32 values); not read by the game.
    pub header: Vec<u8>,
    /// Layers in file order; unused layers (slot byte 0xFF) are `None`.
    pub layers: Vec<Option<CrdLayer>>,
}

/// Parses a `CRDl` table: 0x20-byte header, 8 slot bytes at 0x20, then two arrays of eight
/// u32 offsets at 0x128 (volume curves) and 0x148 (aux curves), each relative to the
/// position of the offset itself.
pub fn parse_crd(d: &[u8]) -> Result<CrdTable> {
    if bytes(d, 0, 4)? != CRD_MAGIC {
        return Err("not a CRDl table".into());
    }
    let version = bytes(d, 4, 1)?[0];
    if version != 2 {
        return Err(format!("CRDl version {version}, expected 2"));
    }
    let header = bytes(d, 8, 0x18)?.to_vec();
    let slots = bytes(d, SLOTS_AT, CRD_LAYERS)?;
    let curve = |table: usize, i: usize| -> Result<Vec<u8>> {
        let at = table + 4 * i;
        let off = at + u32le(d, at)? as usize;
        Ok(bytes(d, off, CRD_BINS)?.to_vec())
    };
    let layers = (0..CRD_LAYERS)
        .map(|i| {
            // The game tests the slot as a signed char (`>= 0`).
            if slots[i] >= 0x80 {
                return Ok(None);
            }
            Ok(Some(CrdLayer {
                slot: slots[i],
                volume: curve(VOLUME_PTRS_AT, i)?,
                aux: curve(PITCH_PTRS_AT, i)?,
            }))
        })
        .collect::<Result<_>>()?;
    Ok(CrdTable {
        version,
        header,
        layers,
    })
}

/// The four tables of an engine archive, in the order the game indexes its 32 voices
/// (`0x4acc10`: voices 0..7 `.ect`, 8..15 `.elt`, 16..23 `.cct`, 24..31 `.clt`).
pub const ENGINE_TABLE_EXTS: [&str; 4] = ["ect", "elt", "cct", "clt"];

/// Per-car `.spl`: 13 little-endian i32 copied over the engine-audio defaults (`0x4acc10`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineSpl(pub [i32; 13]);

impl EngineSpl {
    /// Built-in values used when no `.spl` is loaded (`0x4acc10`).
    pub const DEFAULT: Self = Self([
        0x32, 0, 0x46, 0x14, 0x40, 0x80, 0x80, 0xaa, 0x50, 0x68, 0x19, 0xdc, 0x1e,
    ]);
    /// Master volume for view mode 0, 1, 2+ (fields 2, 0, 7).
    pub fn master_volume(&self, mode: u32) -> i32 {
        self.0[match mode {
            0 => 2,
            1 => 0,
            _ => 7,
        }]
    }
    /// Bank-group crossfade position (0..128) for view mode 0, 1, 2+ (fields 3, 1, 4).
    pub fn group_fade(&self, mode: u32) -> i32 {
        self.0[match mode {
            0 => 3,
            1 => 1,
            _ => 4,
        }]
    }
    /// Pitch scale (field 8): pitch = scale * (rpm16 + 0x3333) >> 10, 0x1000 = 1.0.
    pub fn pitch_scale(&self) -> i32 {
        self.0[8]
    }
    /// RPM-to-bin scale (field 9): bin = min(scale * rpm16 >> 14, 511).
    pub fn rpm_bin_scale(&self) -> i32 {
        self.0[9]
    }
}

pub fn parse_spl(d: &[u8]) -> Result<EngineSpl> {
    if d.len() != 52 {
        return Err(format!("engine .spl has {} bytes, expected 52", d.len()));
    }
    let mut f = [0i32; 13];
    for (i, v) in f.iter_mut().enumerate() {
        *v = u32le(d, 4 * i)? as i32;
    }
    Ok(EngineSpl(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crd(slots: [u8; 8], curves: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let mut d = vec![0u8; 0x168];
        d[..4].copy_from_slice(b"CRDl");
        d[4] = 2;
        d[SLOTS_AT..SLOTS_AT + 8].copy_from_slice(&slots);
        for (i, (v, a)) in curves.iter().enumerate() {
            let vol_at = 0x168 + 0x400 * i;
            let aux_at = vol_at + 0x200;
            let pv = VOLUME_PTRS_AT + 4 * i;
            let pa = PITCH_PTRS_AT + 4 * i;
            d[pv..pv + 4].copy_from_slice(&((vol_at - pv) as u32).to_le_bytes());
            d[pa..pa + 4].copy_from_slice(&((aux_at - pa) as u32).to_le_bytes());
            d.extend_from_slice(v);
            d.extend_from_slice(a);
        }
        d
    }

    #[test]
    fn parses_layers_and_active_range() {
        let mut v = vec![0u8; CRD_BINS];
        v[10..20].fill(5);
        v[30] = 9; // after the first zero run: dropped by the game
        let d = crd(
            [3, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            &[(v, vec![1; CRD_BINS])],
        );
        let t = parse_crd(&d).unwrap();
        let l = t.layers[0].as_ref().unwrap();
        assert_eq!(l.slot, 3);
        assert_eq!(l.active_range(), 10..20);
        assert_eq!((l.volume_at(15), l.volume_at(30)), (5, 0));
        assert!(t.layers[1..].iter().all(Option::is_none));
    }

    #[test]
    fn rejects_bad_tables() {
        assert!(parse_crd(b"CRDx").is_err());
        let mut d = crd([0xff; 8], &[]);
        d[4] = 1;
        assert!(parse_crd(&d).is_err());
        let d = crd([0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], &[]);
        assert!(parse_crd(&d).is_err());
        assert!(parse_spl(&[0; 51]).is_err());
    }

    /// All engine archives: tables parse, every layer's slot exists in the bank, and every
    /// per-car `.spl` parses.
    #[test]
    fn local_engine_tables_parse() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/Sounds");
        let Ok(read) = std::fs::read_dir(&dir) else {
            return;
        };
        let (mut archives, mut spls) = (0, 0);
        // Low-detail `*16` banks may omit slots below 0x40; the mixer mutes those (`0x4ac290`).
        for entry in read {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
            if name.ends_with(".spl") && name != "game.spl" && name != "replay.spl" {
                parse_spl(&std::fs::read(&path).unwrap()).unwrap();
                spls += 1;
            }
            if !(name.starts_with("sd") && name.ends_with(".viv")) {
                continue;
            }
            let viv = std::fs::read(&path).unwrap();
            let files = crate::parse_viv(&viv).unwrap();
            let stem = name.trim_end_matches(".viv");
            let bank = crate::viv_file(&viv, &files, &format!("{stem}.bnk")).unwrap();
            let bank = crate::parse_bnk(bank).unwrap();
            for ext in ENGINE_TABLE_EXTS {
                let d = crate::viv_file(&viv, &files, &format!("{stem}.{ext}")).unwrap();
                let t = parse_crd(d).unwrap_or_else(|e| panic!("{name} {ext}: {e}"));
                for l in t.layers.iter().flatten() {
                    let optional = name.ends_with("16.viv") && l.slot < 0x40;
                    assert!(
                        optional || bank.patch(l.slot as usize).is_some(),
                        "{name} {ext} slot {}",
                        l.slot
                    );
                    assert!(l.volume.iter().all(|&v| v <= 127));
                }
            }
            archives += 1;
        }
        assert_eq!((archives, spls), (40, 84));
    }

    #[test]
    fn spl_maps_fields() {
        let mut d = Vec::new();
        for v in EngineSpl::DEFAULT.0 {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let s = parse_spl(&d).unwrap();
        assert_eq!(s, EngineSpl::DEFAULT);
        assert_eq!(
            (s.master_volume(0), s.master_volume(1), s.master_volume(3)),
            (70, 50, 170)
        );
        assert_eq!(
            (s.group_fade(0), s.group_fade(1), s.group_fade(2)),
            (20, 0, 64)
        );
        assert_eq!((s.pitch_scale(), s.rpm_bin_scale()), (80, 104));
    }
}
