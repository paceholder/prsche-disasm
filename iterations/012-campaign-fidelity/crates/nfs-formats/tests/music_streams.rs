//! Music streams decode exactly like FFmpeg's `adpcm_ea_r1` (skipped without `local/game`).
//!
//! References: `ffmpeg -v error -i <file> -f s16le -acodec pcm_s16le -`, FNV-1a 64 over the
//! interleaved s16le bytes, and the sample count per channel (FFmpeg 7, 2026-10-24).
use nfs_formats::{parse_ea_stream, parse_viv, viv_file};
use std::path::PathBuf;

fn music_viv() -> Option<Vec<u8>> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../local/game/GameData/Music/zzzymus.viv");
    std::fs::read(p).ok()
}

fn fnv1a_interleaved(planes: &[Vec<i16>]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for i in 0..planes[0].len() {
        for p in planes {
            for b in p[i].to_le_bytes() {
                h = (h ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
    }
    h
}

#[test]
fn installed_music_matches_ffmpeg() {
    let Some(d) = music_viv() else {
        eprintln!("skip: local/game not installed");
        return;
    };
    let entries = parse_viv(&d).unwrap();
    assert_eq!(entries.len(), 28);
    for e in &entries {
        let s = parse_ea_stream(&d[e.offset..e.offset + e.size])
            .unwrap_or_else(|err| panic!("{}: {err}", e.name));
        assert_eq!((s.channels, s.sample_rate), (2, 32000), "{}", e.name);
        let diff = s.header_samples as i64 - s.decoded_samples() as i64;
        assert!((0..28 * 4).contains(&diff), "{}: {diff}", e.name);
    }
    for (name, samples, fnv) in [
        (
            "vic01-Congrats-CaptainGinger",
            550_340,
            0x8c5d070a1f6f479c_u64,
        ),
        (
            "menu01-Psychonaught-Captain Ginger",
            4_927_776,
            0x88fad5a704db78c5,
        ),
        ("game13-Activator-Morphadron", 4_743_760, 0x2d0000b0a351f6b0),
    ] {
        let file = viv_file(&d, &entries, name).unwrap();
        let s = parse_ea_stream(file).unwrap();
        let mut planes = vec![Vec::new(), Vec::new()];
        for b in 0..s.blocks.len() {
            s.decode_block(file, b, &mut planes).unwrap();
        }
        assert_eq!(planes[0].len(), samples, "{name}");
        assert_eq!(s.decoded_samples(), samples as u64, "{name}");
        assert_eq!(fnv1a_interleaved(&planes), fnv, "{name}");
    }
}
