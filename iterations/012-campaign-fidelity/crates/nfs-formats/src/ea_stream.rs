//! EA audio stream (`SCHl` header, `SCDl` data blocks) with EA-XA R1 ADPCM, as used by the
//! music in `GameData/Music/zzzymus.viv`.
//!
//! Format notes: `docs/formats/schl.md` in the research workspace. The game decodes these in
//! its EA audio library (stream opened by `0x567720`, not decompiled); the reference for this
//! decoder is FFmpeg's `adpcm_ea_r1`, which it matches sample for sample
//! (`tests/music_streams.rs`).
use crate::bnk::next_tag;
use crate::{bytes, u32le, Result};

pub const SCHL_MAGIC: &[u8; 4] = b"SCHl";

/// EA-XA predictor pairs (x/256), selected by the high nibble of a frame's first byte.
const EA_XA_COEFS: [(i32, i32); 4] = [(0, 0), (240, 0), (460, -208), (392, -220)];

/// A parsed stream: header values and the payload range of every `SCDl` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EaStream {
    pub channels: usize,
    pub sample_rate: u32,
    /// Header tag `0x85`: samples per channel as stored (the decoder drops the samples past
    /// the last whole 28-sample frame of each block, like FFmpeg).
    pub header_samples: u32,
    /// `(payload offset, payload length)` of each `SCDl` block, in file order.
    pub blocks: Vec<(usize, usize)>,
    /// Samples per channel each block decodes to.
    pub block_samples: Vec<u32>,
}

impl EaStream {
    /// Samples per channel the whole stream decodes to.
    pub fn decoded_samples(&self) -> u64 {
        self.block_samples.iter().map(|&n| n as u64).sum()
    }

    /// Decodes block `index` and appends its samples to `planes` (one `Vec` per channel).
    pub fn decode_block(&self, d: &[u8], index: usize, planes: &mut [Vec<i16>]) -> Result<usize> {
        let &(off, len) = self
            .blocks
            .get(index)
            .ok_or_else(|| format!("block {index} out of range"))?;
        decode_r1_block(bytes(d, off, len)?, self.channels, planes)
    }
}

/// Parses the block list. Only header version 1 without a codec tag (= EA-XA R1) is supported.
pub fn parse_ea_stream(d: &[u8]) -> Result<EaStream> {
    if bytes(d, 0, 4)? != SCHL_MAGIC {
        return Err("not an EA SCHl stream".into());
    }
    let header_len = u32le(d, 4)? as usize;
    let header = bytes(d, 0, header_len)?;
    if bytes(header, 8, 2)? != b"PT" {
        return Err("SCHl: missing PT header".into());
    }
    let mut p = 8 + if bytes(header, 11, 1)?[0] & 2 != 0 {
        8
    } else {
        4
    };
    let (mut version, mut codec, mut channels, mut rate, mut samples) = (None, None, 1, 22050, 0);
    while let Some((tag, value)) = next_tag(header, &mut p)? {
        match (tag, value) {
            (0x80, Some(v)) => version = Some(v),
            (0x82, Some(v)) => channels = v,
            (0x84, Some(v)) => rate = v,
            (0x85, Some(v)) => samples = v,
            (0xa0, Some(v)) => codec = Some(v),
            _ => {}
        }
    }
    if version != Some(1) || codec.is_some() {
        return Err(format!(
            "SCHl: unsupported codec (version {version:?}, codec {codec:?}); only EA-XA R1"
        ));
    }
    if !(1..=2).contains(&channels) {
        return Err(format!("SCHl: {channels} channels"));
    }
    let mut stream = EaStream {
        channels: channels as usize,
        sample_rate: u32::try_from(rate).map_err(|_| "SCHl: negative sample rate")?,
        header_samples: u32::try_from(samples).map_err(|_| "SCHl: negative sample count")?,
        blocks: Vec::new(),
        block_samples: Vec::new(),
    };
    let mut at = header_len;
    while at + 8 <= d.len() {
        let id = bytes(d, at, 4)?;
        let size = u32le(d, at + 4)? as usize;
        if size < 8 {
            return Err(format!("offset 0x{at:x}: block size {size}"));
        }
        match id {
            b"SCDl" => {
                bytes(d, at, size)?;
                let n = u32le(d, at + 8)?;
                stream.blocks.push((at + 8, size - 8));
                stream.block_samples.push(n / 28 * 28);
            }
            b"SCEl" => break,
            _ => {}
        }
        at += size;
    }
    Ok(stream)
}

/// Decodes one `SCDl` payload: u32 samples, u32 channel offsets (relative to the end of the
/// offset table), then per channel s16 current, s16 previous and 15-byte frames. Every block
/// restarts from its own history. Appends `28 × floor(n / 28)` samples per channel.
pub fn decode_r1_block(payload: &[u8], channels: usize, planes: &mut [Vec<i16>]) -> Result<usize> {
    if planes.len() < channels {
        return Err("decode_r1_block: too few output planes".into());
    }
    let frames = u32le(payload, 0)? as usize / 28;
    let table = 4 * (channels + 1);
    for (c, plane) in planes.iter_mut().enumerate().take(channels) {
        let mut p = table + u32le(payload, 4 + 4 * c)? as usize;
        let h = bytes(payload, p, 4)?;
        let mut cur = i16::from_le_bytes([h[0], h[1]]) as i32;
        let mut prev = i16::from_le_bytes([h[2], h[3]]) as i32;
        p += 4;
        plane.reserve(frames * 28);
        for _ in 0..frames {
            let f = bytes(payload, p, 15)?;
            p += 15;
            let (c1, c2) = EA_XA_COEFS
                .get((f[0] >> 4) as usize)
                .copied()
                .ok_or_else(|| format!("EA-XA R1 predictor {}", f[0] >> 4))?;
            let shift = 20 - (f[0] & 0x0f) as i32;
            for i in 0..28 {
                let b = f[1 + i / 2];
                let nib = if i % 2 == 0 { b >> 4 } else { b & 0x0f };
                let nib = ((nib as i32) << 28) >> 28;
                let x = shl_signed(nib, shift) + cur * c1 + prev * c2;
                let x = (x >> 8).clamp(-32768, 32767);
                prev = cur;
                cur = x;
                plane.push(x as i16);
            }
        }
    }
    Ok(frames * 28)
}

/// `v << s` for `s` in 5..=20 (FFmpeg computes it in unsigned arithmetic; a negative shift
/// never occurs with a 4-bit shift field but is handled as a right shift).
fn shl_signed(v: i32, s: i32) -> i32 {
    if s >= 0 {
        v.wrapping_shl(s as u32)
    } else {
        v >> (-s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(n: u32, channels: &[(i16, i16, Vec<u8>)]) -> Vec<u8> {
        let mut b = n.to_le_bytes().to_vec();
        let mut off = 0u32;
        for (_, _, frames) in channels {
            b.extend_from_slice(&off.to_le_bytes());
            off += 4 + frames.len() as u32;
        }
        for (cur, prev, frames) in channels {
            b.extend_from_slice(&cur.to_le_bytes());
            b.extend_from_slice(&prev.to_le_bytes());
            b.extend_from_slice(frames);
        }
        b
    }

    fn stream(blocks: &[Vec<u8>]) -> Vec<u8> {
        let tags: &[u8] = &[
            b'P', b'T', 0, 0, 0xfd, 0x80, 1, 1, 0x82, 1, 2, 0x84, 2, 0x7d, 0, 0x85, 1, 30, 0xff,
        ];
        let mut d = b"SCHl".to_vec();
        d.extend_from_slice(&((8 + tags.len()) as u32).to_le_bytes());
        d.extend_from_slice(tags);
        for b in blocks {
            d.extend_from_slice(b"SCDl");
            d.extend_from_slice(&((8 + b.len()) as u32).to_le_bytes());
            d.extend_from_slice(b);
        }
        d.extend_from_slice(b"SCEl\x08\0\0\0");
        d
    }

    #[test]
    fn decodes_predictor_and_history_like_ffmpeg() {
        // Channel 0: predictor 1 (240/256), shift 20 - 12 = 8; nibbles +1, -1, then zeros.
        let mut f0 = vec![0x1c, 0x1f];
        f0.extend_from_slice(&[0; 13]);
        // Channel 1: predictor 0, shift 20; all nibbles +7.
        let mut f1 = vec![0x00];
        f1.extend_from_slice(&[0x77; 14]);
        let d = stream(&[block(30, &[(1000, 500, f0), (0, 0, f1)])]);
        let s = parse_ea_stream(&d).unwrap();
        assert_eq!(
            (s.channels, s.sample_rate, s.header_samples),
            (2, 32000, 30)
        );
        assert_eq!(s.block_samples, vec![28]);
        let mut planes = vec![Vec::new(), Vec::new()];
        assert_eq!(s.decode_block(&d, 0, &mut planes).unwrap(), 28);
        // x0 = (1 << 8) + 1000 * 240 = 240256 >> 8 = 938; x1 = (-256 + 938 * 240) >> 8 = 878.
        assert_eq!(&planes[0][..3], &[938, 878, 823]);
        // 7 << 20 >> 8 = 28672 for every sample (no predictor), clamped range respected.
        assert!(planes[1].iter().all(|&v| v == 28672));
    }

    #[test]
    fn rejects_other_codecs() {
        let mut d = stream(&[]);
        d[13] = 0xa0; // the version tag 0x80 becomes a codec tag 0xA0
        assert!(parse_ea_stream(&d).is_err());
        assert!(parse_ea_stream(b"BNKl\0\0\0\0").is_err());
    }
}
