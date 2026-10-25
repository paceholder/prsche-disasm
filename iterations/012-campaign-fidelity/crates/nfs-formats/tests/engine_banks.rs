//! Every installed engine sound archive parses and decodes (skipped without `local/game`).
use nfs_formats::{decode_patch, parse_bnk, parse_crd, parse_viv, viv_file, ENGINE_TABLE_EXTS};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn sounds_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game");
    let dir = root.join("GameData/Sounds");
    dir.is_dir().then_some(dir)
}

#[test]
fn installed_engine_banks_decode_with_loops_inside_samples() {
    let Some(dir) = sounds_dir() else {
        eprintln!("skip: local/game not installed");
        return;
    };
    let mut archives: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let n = p
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase();
            n.starts_with("sd") && n.ends_with(".viv")
        })
        .collect();
    archives.sort();
    assert!(!archives.is_empty(), "no sd*.viv in {}", dir.display());

    let mut no_loop = Vec::new();
    let mut omitted = Vec::new();
    for path in &archives {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let d = std::fs::read(path).unwrap();
        let entries = parse_viv(&d).unwrap_or_else(|e| panic!("{name}: viv: {e}"));
        let bnk_name = format!("{name}.bnk");
        let bank = viv_file(&d, &entries, &bnk_name)
            .unwrap_or_else(|| panic!("{name}: missing {bnk_name}"));
        let parsed = parse_bnk(bank).unwrap_or_else(|e| panic!("{name}: bnk: {e}"));

        let mut slots = BTreeSet::new();
        for ext in ENGINE_TABLE_EXTS {
            let fname = format!("{name}.{ext}");
            let t =
                viv_file(&d, &entries, &fname).unwrap_or_else(|| panic!("{name}: missing {fname}"));
            let crd = parse_crd(t).unwrap_or_else(|e| panic!("{fname}: {e}"));
            slots.extend(crd.layers.iter().flatten().map(|l| l.slot as usize));
        }
        assert!(!slots.is_empty(), "{name}: tables reference no slots");

        for slot in slots {
            let Some(p) = parsed.patch(slot) else {
                // Low-detail `*16` banks omit slots below 0x40; the mixer mutes them in that mode.
                assert!(
                    name.ends_with("16") && slot < 0x40,
                    "{name}: slot {slot} missing in bank"
                );
                omitted.push(format!("{name} slot {slot}"));
                continue;
            };
            let pcm =
                decode_patch(bank, p).unwrap_or_else(|e| panic!("{name}: slot {slot} decode: {e}"));
            assert!(!pcm.is_empty(), "{name}: slot {slot} decodes to nothing");
            let frames = pcm.len() / p.channels.max(1) as usize;
            match (p.loop_start, p.loop_end) {
                (Some(s), Some(e)) => assert!(
                    s <= e && (e as usize) < frames,
                    "{name}: slot {slot} loop {s}..{e} outside {frames} frames"
                ),
                (None, None) => no_loop.push(format!("{name} slot {slot}")),
                other => panic!("{name}: slot {slot} half-defined loop {other:?}"),
            }
        }
    }
    println!("checked {} archives", archives.len());
    println!("low-detail slots <0x40 absent from bank: {}", omitted.len());
    if no_loop.is_empty() {
        println!("all referenced slots have loop points");
    } else {
        println!("referenced slots WITHOUT loop points ({}):", no_loop.len());
        for s in &no_loop {
            println!("  {s}");
        }
    }
}
