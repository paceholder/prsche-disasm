//! The race HUD speed word `car+0xec4`, recorded by executing nfs5.exe `Car_UpdateSpeedEc4`
//! 0x40eec0 in Unicorn on synthetic velocities (scripts/research/replay-hud-gauges.py, rows
//! `ec4`), replayed through `cockpit::speed_ec4`. Bit for bit. The gauge rows of the same
//! recording are checked by `web/race-hud.test.mjs`.
//!
//! Spec: change original-race-hud (no main spec yet).

use nfs_assets::cockpit::speed_ec4;

const RECORDING: &str = include_str!("fixtures/hud-gauges.tsv");

fn f32_bits(hex: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
}

#[test]
fn speed_ec4_matches_the_original_bit_for_bit() {
    let mut n = 0;
    for line in RECORDING.lines().filter(|l| l.starts_with("ec4\t")) {
        let c: Vec<&str> = line.split('\t').collect();
        let (vx, vz, expected) = (f32_bits(c[1]), f32_bits(c[2]), f32_bits(c[3]));
        let got = speed_ec4([vx, 123.0, vz]);
        assert_eq!(got.to_bits(), expected.to_bits(), "vx={vx} vz={vz}: {got} vs {expected}");
        n += 1;
    }
    assert_eq!(n, 544);
}
