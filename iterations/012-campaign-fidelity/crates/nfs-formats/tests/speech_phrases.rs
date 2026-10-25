//! Every phrase of `oneshots.txt` exists in the speech archive and decodes (skipped without
//! `local/game`). Phrase numbering: `Speech_Load` 0x4affd0.
use nfs_formats::{decode_patch, parse_bnk, parse_oneshots, parse_viv, viv_file};
use std::path::PathBuf;

#[test]
fn installed_phrases_decode() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../local/game/GameData/Speech/zzzxeng.viv");
    let Ok(d) = std::fs::read(p) else {
        eprintln!("skip: local/game not installed");
        return;
    };
    let entries = parse_viv(&d).unwrap();
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
        let b = parse_bnk(bank).unwrap_or_else(|e| panic!("{}: {e}", ph.name));
        let patch = b.patch(0).unwrap();
        assert_eq!(patch.channels, 1, "{}", ph.name);
        if ph.name.to_ascii_lowercase().ends_with(".bnk") {
            assert_eq!(patch.sample_rate, 11025, "{}", ph.name); // static1..4 differ
        }
        let pcm = decode_patch(bank, patch).unwrap();
        assert_eq!(pcm.len(), patch.num_samples as usize);
    }
}
