//! V1 regression: carSpecs recorded by executing nfs5.exe Sim_Load from
//! 0x49c82f through Sim_DeriveFields 0x49cb50 on every installed `.sim` with
//! default tuning (scripts/research/replay-vehicle-sim.py), compared bit for
//! bit with `original_vehicle::sim::derive`.
//!
//! The fixture holds only outputs; the `.sim` inputs are read from the game
//! install at `local/game` (the test is skipped without it, like the
//! nfs-formats `.sim` parser test). Spec: vehicle-dynamics "Every installed
//! car".

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::path::PathBuf;

use nfs_assets::physics::original_vehicle::sim::{derive, Tuning, CAR_SPECS_SIZE};

static FIXTURE: Fixture = Fixture::new("vehicle-sim.tsv");
const INT_COLUMNS: [&str; 5] = [
    "flag_48",
    "engine_type_4c",
    "gear_count_50",
    "shift_time_54",
    "tyre_type_1c0",
];
/// Leading non-struct columns: file, model, sim_sha256.
const META: usize = 3;

fn car_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("local/game/GameData/Simulation/CarData")
}

/// Fixture value -> original dword bits.
fn bits(column: &str, text: &str) -> u32 {
    if column.starts_with("name_00_") || INT_COLUMNS.contains(&column) {
        text.parse::<i32>().unwrap() as u32
    } else {
        // Python repr of the f32 widened to double: exact.
        (text.parse::<f64>().unwrap() as f32).to_bits()
    }
}

#[test]
fn every_installed_car_matches_the_original_bit_for_bit() {
    skip_without_local_fixtures!(FIXTURE);
    let dir = car_data_dir();
    if !dir.exists() {
        eprintln!("skipped: no game install at {}", dir.display());
        return;
    }
    let mut lines = FIXTURE.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    assert_eq!(header.len(), META + CAR_SPECS_SIZE / 4);
    let mut rows = 0;
    let mut mismatches = Vec::new();
    for line in lines {
        let cols: Vec<&str> = line.split('\t').collect();
        assert_eq!(cols.len(), header.len());
        let (file, model) = (cols[0], cols[1]);
        let raw = std::fs::read(dir.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let specs = derive(&raw, &Tuning::for_model(model)).to_bytes();
        for (i, (column, text)) in header[META..].iter().zip(&cols[META..]).enumerate() {
            let actual = u32::from_le_bytes(specs[4 * i..4 * i + 4].try_into().unwrap());
            let expected = bits(column, text);
            if actual != expected {
                mismatches.push(format!(
                    "{file} {column}: port {actual:#010x} ({}), original {expected:#010x} ({text})",
                    f32::from_bits(actual)
                ));
            }
        }
        rows += 1;
    }
    assert_eq!(rows, 86);
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn default_tuning_is_neutral_for_downforce_and_brake_bias() {
    let dir = car_data_dir();
    let Ok(raw) = std::fs::read(dir.join("boxster25.sim")) else {
        eprintln!("skipped: no game install at {}", dir.display());
        return;
    };
    let specs = derive(&raw, &Tuning::for_model("boxster"));
    let f = |off: usize| f32::from_le_bytes(raw[off..off + 4].try_into().unwrap());
    // 0x10c/0x110 are front/rear downforce per m/s: load x0.001, then the
    // fe.txt default factor (50*0.02+1)/2, which is 1 within an f32 ulp.
    assert!((specs.front_downforce_10c - f(0x10c) * 0.001).abs() <= 1e-9);
    assert!((specs.rear_downforce_110 - f(0x110) * 0.001).abs() <= 1e-9);
    assert_eq!(
        specs.front_brake_share_108,
        (f(0x108) as f64 * 0.01f32 as f64) as f32
    );
    assert_eq!(
        specs.gear_count_50,
        i32::from_le_bytes(raw[0x50..0x54].try_into().unwrap()) + 2
    );
    assert_eq!(specs.tyre_type_1c0, 0);
    assert_eq!(derive(&raw, &Tuning::for_model("gt1")).tyre_type_1c0, 2);
}
