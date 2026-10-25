//! Every phrase exists and either decodes or matches the exact known unsupported
//! archive revision's rejection. Skipped without `local/game`. `Speech_Load`: 0x4affd0.
use nfs_formats::{decode_patch, parse_bnk, parse_oneshots, parse_viv, viv_file};
use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn supported_decode_and_known_layout_rejection() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../local/game/GameData/Speech/zzzxeng.viv");
    let Ok(d) = std::fs::read(p) else {
        eprintln!("skip: local/game not installed");
        return;
    };
    let entries = parse_viv(&d).unwrap();
    // Exact local revision, SHA256 f89ab2839d14515a170e3dc2dfff37cff2b3df1be3dfcec6f50b8a3fd52cd64a.
    // A test-local byte fingerprint avoids adding a hashing dependency. No other
    // archive may take this expected-rejection path.
    let fingerprint = d.iter().fold(0xcbf29ce484222325u64, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    let known_layout = d.len() == 3_617_974 && fingerprint == 0x5823509da473f07a;
    let mut rejected = BTreeSet::new();
    let mut negative_counts = BTreeSet::new();
    let mut decoded_names = BTreeSet::new();
    let mut decoded = 0;
    let phrases = parse_oneshots(viv_file(&d, &entries, "oneshots.txt").unwrap());
    assert_eq!(phrases.len(), 166);
    let names: Vec<_> = phrases[..4].iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        ["SOR-a-3.bnk", "SOR-a-2.bnk", "SOR-a-1.bnk", "SOR-a-Go.bnk"]
    );
    assert_eq!(phrases[132].name, "Lap-your1st.bnk");
    assert_eq!(phrases[160].name, "Bon-youwonboncar.bnk");
    for (i, ph) in phrases.iter().enumerate() {
        let bank = viv_file(&d, &entries, &ph.name).unwrap_or_else(|| panic!("{i} {}", ph.name));
        let b = match parse_bnk(bank) {
            Ok(b) => b,
            Err(e) if known_layout => {
                // This revision's PCM8 banks have 64 header bytes plus N data
                // bytes, but tag88 points to128. Original0059952F/0059962B..637
                // uses that bank-relative offset; no preparation fix is proved.
                assert_eq!(&bank[..8], b"BNKl\x04\0\x01\0", "{}", ph.name);
                let header = u32::from_le_bytes(bank[8..12].try_into().unwrap());
                let samples = u32::from_le_bytes(bank[12..16].try_into().unwrap()) as usize;
                assert_eq!(header, 64, "{}", ph.name);
                assert_eq!(bank.len(), 64 + samples, "{}", ph.name);
                assert!(bank[..64].windows(3).any(|w| w == [0xa0, 1, 9]), "{}", ph.name);
                assert!(bank[..64].windows(6).any(|w| w == [0x88, 4, 0, 0, 0, 128]), "{}", ph.name);
                let sample_word = (samples as u16).to_be_bytes();
                let signed_word = samples <= u16::MAX as usize
                    && sample_word[0] & 0x80 != 0
                    && bank[..64].windows(4).any(|w| w == [0x85, 2, sample_word[0], sample_word[1]]);
                if signed_word {
                    // Original0059BDB5..BDE7 sign-extends one/two/three-byte
                    // values;005911A7..AF stores tag85 unchanged. These25
                    // revisions encode a negative count, not an unsigned u16.
                    assert_eq!(e, "slot 0: offset 0x18: tag 0x85 < 0", "{}", ph.name);
                    negative_counts.insert(ph.name.to_ascii_lowercase());
                } else {
                    assert_eq!(e, format!("slot 0: offset 0x80: need {samples} bytes, file has {}", bank.len()), "{}", ph.name);
                    rejected.insert(ph.name.to_ascii_lowercase());
                }
                continue;
            }
            Err(e) => panic!("{}: {e}", ph.name),
        };
        let patch = b.patch(0).unwrap();
        assert_eq!(patch.channels, 1, "{}", ph.name);
        if ph.name.to_ascii_lowercase().ends_with(".bnk") {
            assert_eq!(patch.sample_rate, 11025, "{}", ph.name); // static1..4 differ
        }
        let pcm = decode_patch(bank, patch).unwrap();
        assert_eq!(pcm.len(), patch.num_samples as usize);
        decoded += 1;
        decoded_names.insert(ph.name.to_ascii_lowercase());
    }
    if known_layout {
        // The archive has131 such banks;130 distinct ones are referenced here.
        assert_eq!(rejected.len(), 130, "known unsupported referenced bank revisions");
        assert_eq!(negative_counts.len(), 25, "known signed-count rejections");
        assert_eq!(decoded_names.len(), 5, "remaining distinct phrase banks must decode");
        eprintln!("known speech revision:130 bounds rejections,25 signed-count rejections,5 distinct banks decoded");
    } else {
        assert_eq!(decoded, phrases.len(), "every phrase must decode in other archives");
    }
}
