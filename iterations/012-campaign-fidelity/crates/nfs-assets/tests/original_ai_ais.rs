//! AI car data: `Ais_Load` 0x49ca30 (with 0x4a2dc0, 0x4a2950, 0x4a2bc0) for
//! every track, direction, model and grip ratio, recorded in Unicorn by
//! scripts/research/replay-ai-ais.py. Needs `local/game` (skipped otherwise).

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::path::{Path, PathBuf};

use nfs_assets::race::original_ai::ais::{load_ais, AIS_SIZE};

static FIXTURE: Fixture = Fixture::new("ai-ais.tsv");

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData/Simulation/AICarData")
}

fn bits(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}

fn find_ais(model: &str) -> Vec<u8> {
    for e in std::fs::read_dir(dir()).unwrap().flatten() {
        let p = e.path();
        let ok = p.extension().is_some_and(|x| x.eq_ignore_ascii_case("ais"))
            && p.file_stem().unwrap().to_string_lossy().eq_ignore_ascii_case(model);
        if ok {
            return std::fs::read(p).unwrap();
        }
    }
    panic!("no {model}.ais");
}

#[test]
fn ais_load_matches_the_original() {
    skip_without_local_fixtures!(FIXTURE);
    if !dir().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    let (mut rows, mut mism) = (0, Vec::new());
    for line in FIXTURE.lines().filter(|l| !l.starts_with('#')).skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        let (track, reverse, model) = (c[0], c[1] == "1", c[2]);
        let mu = f32::from_bits(bits(c[3]));
        let mut sim = f32::from_bits(bits(c[4]));
        let (ais, tables) = load_ais(&find_ais(model), model, track, reverse, mu, &mut sim);
        let want_ais: Vec<u8> = (0..c[5].len() / 2)
            .map(|i| u8::from_str_radix(&c[5][2 * i..2 * i + 2], 16).unwrap())
            .collect();
        assert_eq!(want_ais.len(), AIS_SIZE);
        let mut r = Vec::new();
        for (a, b) in tables.ranges_5e74ac.iter() {
            r.extend_from_slice(&a.to_le_bytes());
            r.extend_from_slice(&b.to_le_bytes());
        }
        for f in tables.factors_5e74fc.iter() {
            r.extend_from_slice(&f.to_le_bytes());
        }
        let ranges_hex: String = r.iter().map(|b| format!("{b:02x}")).collect();
        let mut bad = Vec::new();
        if ais.raw[..] != want_ais[..] {
            let i = (0..AIS_SIZE).find(|&i| ais.raw[i] != want_ais[i]).unwrap();
            bad.push(format!("ais byte {i:#x}"));
        }
        if sim.to_bits() != bits(c[6]) {
            bad.push(format!("sim140 {:08x} vs {}", sim.to_bits(), c[6]));
        }
        if tables.width_ref_5e6e34.to_bits() != bits(c[7]) {
            bad.push("width".to_string());
        }
        if ranges_hex != c[8] {
            bad.push("ranges".to_string());
        }
        if !bad.is_empty() {
            mism.push(format!("{track} rev={} {model} mu={}: {}", c[1], c[3], bad.join(", ")));
        }
        rows += 1;
    }
    assert!(rows == 1320, "rows {rows}");
    assert!(
        mism.is_empty(),
        "{} of {rows} rows differ, e.g.:\n{}",
        mism.len(),
        mism.iter().take(8).cloned().collect::<Vec<_>>().join("\n")
    );
}
