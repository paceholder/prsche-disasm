//! EA sound bank (`.bnk`, magic `BNKl`) and its sample codecs.
//!
//! Format notes and evidence: `docs/formats/bnk.md` in the research workspace.
//! Tag reader: original `0x599760` (iterator) and `0x59bd90` (signed big-endian value).
//! Sample header defaults (22050 Hz, mono, codec 10): original `0x590f70`.
//! Patch defaults (root note 60): original `0x599100`; pitch: `0x590420`.
use crate::{bytes, u16le, u32le, Result};
use std::collections::BTreeMap;

pub const BNK_MAGIC: &[u8; 4] = b"BNKl";
/// Note the engine plays every patch at (`0x561960` default params).
pub const DEFAULT_NOTE: i32 = 60;

/// Codec id from tag `0xA0` (default 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BnkCodec {
    /// 8: signed 16-bit little-endian PCM (2 bytes per sample per channel).
    PcmS16,
    /// 9: signed 8-bit PCM (used by the low-detail `*16.viv` banks).
    PcmS8,
    /// 10: EA-XA 4-bit ADPCM, 15-byte frames of 28 samples per channel.
    EaXa,
    Other(i32),
}

impl BnkCodec {
    fn from_id(id: i32) -> Self {
        match id {
            8 => Self::PcmS16,
            9 => Self::PcmS8,
            10 => Self::EaXa,
            other => Self::Other(other),
        }
    }
}

/// One `PT` header: patch attributes (tags < 0x80) and the sample description.
#[derive(Debug, Clone, PartialEq)]
pub struct BnkPatch {
    /// Every tag with a value of 4 bytes or fewer; later duplicates overwrite earlier ones.
    pub tags: BTreeMap<u8, i32>,
    pub codec: BnkCodec,
    pub channels: u8,
    pub sample_rate: u32,
    pub num_samples: u32,
    /// Tag `0x86`, in samples.
    pub loop_start: Option<u32>,
    /// Tag `0x87`, in samples (observed values are 1..4 below `num_samples`).
    pub loop_end: Option<u32>,
    /// Tag `0x88`: absolute offset of the sample data in the bank file.
    pub data_offset: usize,
}

impl BnkPatch {
    fn tag(&self, t: u8) -> Option<i32> {
        self.tags.get(&t).copied()
    }
    /// Tag `0x07`, default 60.
    pub fn root_note(&self) -> i32 {
        self.tag(0x07).unwrap_or(60)
    }
    /// Tag `0x10`, signed cents, default 0.
    pub fn fine_tune_cents(&self) -> i32 {
        self.tag(0x10).unwrap_or(0)
    }
    /// Base detune when played at `DEFAULT_NOTE`: `(note - root) * 100 + fine` (`0x590420`).
    pub fn detune_cents(&self) -> i32 {
        (DEFAULT_NOTE - self.root_note()) * 100 + self.fine_tune_cents()
    }
    /// Playback rate at pitch multiplier 1.0 (0x1000 in the original).
    pub fn base_rate_hz(&self) -> f64 {
        self.sample_rate as f64 * 2f64.powf(self.detune_cents() as f64 / 1200.0)
    }
    pub fn is_looped(&self) -> bool {
        self.loop_end.is_some()
    }
    /// Encoded byte length of the sample data.
    pub fn data_len(&self) -> Option<usize> {
        let n = self.num_samples as usize;
        let ch = self.channels as usize;
        match self.codec {
            BnkCodec::PcmS16 => Some(n * ch * 2),
            BnkCodec::PcmS8 => Some(n * ch),
            BnkCodec::EaXa => Some(n.div_ceil(28) * 15 * ch),
            BnkCodec::Other(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bnk {
    pub version: u16,
    pub header_size: u32,
    pub data_size: u32,
    /// Slot index = patch id used by the game (`0x565e70` second argument).
    pub slots: Vec<Option<BnkPatch>>,
}

impl Bnk {
    pub fn patch(&self, slot: usize) -> Option<&BnkPatch> {
        self.slots.get(slot).and_then(Option::as_ref)
    }
}

/// Reads tag/value pairs the way `0x599760` does. Returns `None` at the `0xFF` terminator.
fn next_tag(d: &[u8], p: &mut usize) -> Result<Option<(u8, Option<i32>)>> {
    loop {
        let t = bytes(d, *p, 1)?[0];
        match t {
            0xfc => *p += 1,
            0xff => return Ok(None),
            0xfd | 0xfe => {
                *p += 1;
                return Ok(Some((t, None)));
            }
            _ => {
                let mut len = bytes(d, *p + 1, 1)?[0] as usize;
                *p += 2;
                if len == 0xff {
                    len = u32::from_be_bytes(bytes(d, *p, 4)?.try_into().unwrap()) as usize;
                    *p += 4;
                }
                let raw = bytes(d, *p, len)?;
                *p += len;
                let value = (len <= 4).then(|| {
                    let v = raw.iter().fold(0u32, |a, &b| (a << 8) | b as u32);
                    match len {
                        1..=3 => {
                            let s = 32 - 8 * len as u32;
                            ((v << s) as i32) >> s
                        }
                        _ => v as i32,
                    }
                });
                return Ok(Some((t, value)));
            }
        }
    }
}

fn parse_patch(d: &[u8], at: usize) -> Result<BnkPatch> {
    if bytes(d, at, 2)? != b"PT" {
        return Err(format!("offset 0x{at:x}: missing PT header"));
    }
    let mut p = at
        + if bytes(d, at + 3, 1)?[0] & 2 != 0 {
            8
        } else {
            4
        };
    let mut tags = BTreeMap::new();
    while let Some((t, v)) = next_tag(d, &mut p)? {
        if let Some(v) = v {
            tags.insert(t, v);
        }
    }
    let get = |t: u8| tags.get(&t).copied();
    let nonneg = |t: u8| -> Result<Option<u32>> {
        get(t)
            .map(|v| u32::try_from(v).map_err(|_| format!("offset 0x{at:x}: tag 0x{t:x} < 0")))
            .transpose()
    };
    let patch = BnkPatch {
        codec: BnkCodec::from_id(get(0xa0).unwrap_or(10)),
        channels: match get(0x82).unwrap_or(1) {
            c @ 1..=2 => c as u8,
            c => return Err(format!("offset 0x{at:x}: {c} channels")),
        },
        sample_rate: nonneg(0x84)?.unwrap_or(22050),
        num_samples: nonneg(0x85)?.ok_or_else(|| format!("offset 0x{at:x}: no sample count"))?,
        loop_start: nonneg(0x86)?,
        loop_end: nonneg(0x87)?,
        data_offset: nonneg(0x88)?.ok_or_else(|| format!("offset 0x{at:x}: no data offset"))?
            as usize,
        tags,
    };
    if let Some(len) = patch.data_len() {
        bytes(d, patch.data_offset, len)?;
    }
    Ok(patch)
}

/// Parses a `BNKl` bank: 0x14-byte header, then one u32 per slot holding the offset of
/// its `PT` header relative to the slot's own position (0 = empty slot).
pub fn parse_bnk(d: &[u8]) -> Result<Bnk> {
    if bytes(d, 0, 4)? != BNK_MAGIC {
        return Err("not a BNKl sound bank".into());
    }
    let version = u16le(d, 4)?;
    let count = u16le(d, 6)? as usize;
    let header_size = u32le(d, 8)?;
    let data_size = u32le(d, 12)?;
    let slots = (0..count)
        .map(|i| {
            let at = 0x14 + 4 * i;
            match u32le(d, at)? as usize {
                0 => Ok(None),
                rel => parse_patch(d, at + rel)
                    .map(Some)
                    .map_err(|e| format!("slot {i}: {e}")),
            }
        })
        .collect::<Result<_>>()?;
    Ok(Bnk {
        version,
        header_size,
        data_size,
        slots,
    })
}

/// EA-XA predictor pairs (x/256), selected by the high nibble of each frame byte.
const EA_XA_COEFS: [(i32, i32); 4] = [(0, 0), (240, 0), (460, -208), (392, -220)];

/// Decodes one EA-XA channel: frames of 1 header byte (predictor << 4 | shift - 8)
/// plus 14 bytes of nibbles, high nibble first. `stride` is the distance between frames.
fn decode_ea_xa_channel(
    d: &[u8],
    start: usize,
    stride: usize,
    n: usize,
    out: &mut Vec<i16>,
) -> Result<()> {
    let (mut h1, mut h2) = (0i32, 0i32);
    let mut produced = 0;
    let mut p = start;
    while produced < n {
        let f = bytes(d, p, 15)?;
        let pred = (f[0] >> 4) as usize;
        let &(c1, c2) = EA_XA_COEFS
            .get(pred)
            .ok_or_else(|| format!("offset 0x{p:x}: EA-XA predictor {pred}"))?;
        let shift = (f[0] & 0x0f) as u32 + 8;
        for i in 0..28 {
            if produced == n {
                break;
            }
            let b = f[1 + i / 2];
            let nib = if i % 2 == 0 { b >> 4 } else { b & 0x0f };
            let s = ((((nib as i32) << 28) >> shift) + c1 * h1 + c2 * h2 + 128) >> 8;
            let s = s.clamp(-32768, 32767);
            h2 = h1;
            h1 = s;
            out.push(s as i16);
            produced += 1;
        }
        p += stride;
    }
    Ok(())
}

/// Decodes a patch's sample to interleaved signed 16-bit PCM.
pub fn decode_patch(bank: &[u8], patch: &BnkPatch) -> Result<Vec<i16>> {
    let n = patch.num_samples as usize;
    let ch = patch.channels as usize;
    let len = patch
        .data_len()
        .ok_or_else(|| format!("unsupported codec {:?}", patch.codec))?;
    let data = bytes(bank, patch.data_offset, len)?;
    Ok(match patch.codec {
        BnkCodec::PcmS16 => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect(),
        BnkCodec::PcmS8 => data.iter().map(|&b| ((b as i8) as i16) << 8).collect(),
        BnkCodec::EaXa => {
            // Stereo: per 28-sample block, one 15-byte frame for each channel in turn.
            let mut planes = Vec::with_capacity(ch);
            for c in 0..ch {
                let mut plane = Vec::with_capacity(n);
                decode_ea_xa_channel(bank, patch.data_offset + 15 * c, 15 * ch, n, &mut plane)?;
                planes.push(plane);
            }
            (0..n * ch).map(|i| planes[i % ch][i / ch]).collect()
        }
        BnkCodec::Other(_) => unreachable!(),
    })
}

/// Minimal RIFF/WAVE writer for 16-bit PCM.
pub fn wav_bytes(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    w.extend_from_slice(&(channels * 2).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-slot bank whose patch carries `tags` followed by `data`.
    fn bank(tags: &[(u8, &[u8])], data: &[u8]) -> Vec<u8> {
        let mut pt = b"PT\0\0".to_vec();
        pt.extend_from_slice(&[0x07, 1, 29, 0x10, 1, 0xc4, 0xfd]);
        for (t, v) in tags {
            pt.push(*t);
            pt.push(v.len() as u8);
            pt.extend_from_slice(v);
        }
        let header = 0x18 + pt.len() + 6 + 1;
        pt.extend_from_slice(&[0x88, 4]);
        pt.extend_from_slice(&(header as u32).to_be_bytes());
        pt.push(0xff);
        let mut d = b"BNKl".to_vec();
        d.extend_from_slice(&4u16.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&(header as u32).to_le_bytes());
        d.extend_from_slice(&(data.len() as u32).to_le_bytes());
        d.extend_from_slice(&u32::MAX.to_le_bytes());
        d.extend_from_slice(&4u32.to_le_bytes());
        d.extend_from_slice(&pt);
        assert_eq!(d.len(), header);
        d.extend_from_slice(data);
        d
    }

    #[test]
    fn parses_defaults_and_signed_patch_tags() {
        let d = bank(&[(0x85, &[0, 28])], &[0u8; 15]);
        let b = parse_bnk(&d).unwrap();
        let p = b.patch(0).unwrap();
        assert_eq!(
            (p.sample_rate, p.channels, p.codec),
            (22050, 1, BnkCodec::EaXa)
        );
        assert_eq!((p.root_note(), p.fine_tune_cents()), (29, -60));
        assert_eq!(p.detune_cents(), 3040);
        assert_eq!(p.num_samples, 28);
        assert!(!p.is_looped());
    }

    #[test]
    fn decodes_pcm8_and_pcm16() {
        let d = bank(&[(0x85, &[3]), (0xa0, &[9])], &[0x80, 0x00, 0x7f]);
        let b = parse_bnk(&d).unwrap();
        let s = decode_patch(&d, b.patch(0).unwrap()).unwrap();
        assert_eq!(s, vec![-32768, 0, 0x7f00]);
        let d = bank(&[(0x85, &[2]), (0xa0, &[8])], &[0x34, 0x12, 0xff, 0xff]);
        let b = parse_bnk(&d).unwrap();
        assert_eq!(
            decode_patch(&d, b.patch(0).unwrap()).unwrap(),
            vec![0x1234, -1]
        );
    }

    #[test]
    fn decodes_ea_xa_frame() {
        // Predictor 0, shift 8+8=16: nibble n -> ((n << 28 >> 16) + 128) >> 8 = n * 16.
        let mut frame = vec![0x08];
        frame.extend_from_slice(&[0x17; 14]); // nibbles 1, 7
        let d = bank(&[(0x85, &[0, 4])], &frame);
        let s = decode_patch(&d, parse_bnk(&d).unwrap().patch(0).unwrap()).unwrap();
        assert_eq!(s, vec![16, 112, 16, 112]);
        // Predictor 1 (240/256) integrates the previous output.
        let mut frame = vec![0x18, 0x10];
        frame.extend_from_slice(&[0; 13]);
        let d = bank(&[(0x85, &[0, 3])], &frame);
        let s = decode_patch(&d, parse_bnk(&d).unwrap().patch(0).unwrap()).unwrap();
        assert_eq!(s, vec![16, 15, 14]);
    }

    #[test]
    fn rejects_bad_predictor_and_truncation() {
        let mut frame = vec![0xe0];
        frame.extend_from_slice(&[0; 14]);
        let d = bank(&[(0x85, &[28])], &frame);
        let b = parse_bnk(&d).unwrap();
        assert!(decode_patch(&d, b.patch(0).unwrap()).is_err());
        let d = bank(&[(0x85, &[29])], &frame);
        assert!(parse_bnk(&d).is_err());
        assert!(parse_bnk(b"BNKx").is_err());
    }

    /// Every patch of every engine bank (`GameData/Sounds/sd*.viv`) parses and decodes.
    #[test]
    fn local_engine_banks_decode() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../..")
            .join("local/game/GameData/Sounds");
        let Ok(read) = std::fs::read_dir(&dir) else {
            return;
        };
        let (mut banks, mut patches) = (0, 0);
        for entry in read {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
            if !(name.starts_with("sd") && name.ends_with(".viv")) {
                continue;
            }
            let viv = std::fs::read(&path).unwrap();
            let files = crate::parse_viv(&viv).unwrap();
            let stem = name.trim_end_matches(".viv");
            let d = crate::viv_file(&viv, &files, &format!("{stem}.bnk")).unwrap();
            let b = parse_bnk(d).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(b.version, 4);
            assert_eq!((b.header_size + b.data_size) as usize, d.len(), "{name}");
            for (slot, p) in b.slots.iter().enumerate() {
                let Some(p) = p else { continue };
                let pcm = decode_patch(d, p).unwrap_or_else(|e| panic!("{name}:{slot}: {e}"));
                assert_eq!(pcm.len(), p.num_samples as usize * p.channels as usize);
                if let (Some(s), Some(e)) = (p.loop_start, p.loop_end) {
                    assert!(s <= e && e < p.num_samples, "{name}:{slot} loop");
                }
                patches += 1;
            }
            banks += 1;
        }
        assert_eq!(banks, 40);
        assert!(patches > 1000, "{patches}");
    }

    #[test]
    fn wav_header_is_44_bytes() {
        let w = wav_bytes(&[1, 2, 3], 1, 22050);
        assert_eq!(w.len(), 50);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(u32le(&w, 24).unwrap(), 22050);
    }
}
