//! `LightGroups` against the original light groups and pop-up headlight channel, executed in
//! Unicorn by `scripts/research/replay-car-lights.py` (fixture
//! `tests/fixtures/car-lights-steps.tsv`, synthetic inputs): `R3DCar_UpdateLights` 0x439360
//! followed by `R3DCar_SetAnimChannels` 0x438440, per step. The lit bytes, the 14 group levels
//! (bit for bit) and transform channel 0x11 x (the lamp lid rotation) must match, for fixed
//! lamps (extent 0), the 944 / 914 / 928 extents (0.14, 0.19, -0.18), braking, indicators,
//! hazard, reverse, broken zones and the showroom.
use nfs_assets::lights::{LightGroups, LightInputs};
use std::collections::HashMap;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/car-lights-steps.tsv");
/// `[0x5deb48]` written by the recorder; the blink phase is `(ticks / (rate >> 1)) & 1`.
const TICK_RATE: i64 = 128;

fn rows() -> Vec<HashMap<String, String>> {
    let text = std::fs::read_to_string(FIXTURE).unwrap();
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    lines
        .map(|l| header.iter().zip(l.split('\t')).map(|(k, v)| (k.to_string(), v.to_string())).collect())
        .collect()
}

fn floats(s: &str) -> Vec<f32> {
    s.split(',').map(|v| v.parse::<f64>().unwrap() as f32).collect()
}

#[test]
fn light_groups_and_lamp_channel_match_the_original() {
    let rows = rows();
    assert!(rows.len() > 1000, "{} rows", rows.len());
    let mut groups = LightGroups::default();
    let mut case = String::new();
    for r in &rows {
        let num = |k: &str| r[k].parse::<i64>().unwrap();
        if r["case"] != case {
            case = r["case"].clone();
            groups = LightGroups::with_headlight_extent(r["extent"].parse::<f64>().unwrap() as f32);
        }
        let switches = num("switches");
        let damage: [f32; 9] = floats(&r["damage"]).try_into().unwrap();
        let inputs = LightInputs {
            brake: if num("reverse_d98") != 0 { num("gas") } else { num("brake") } as u8,
            reverse_gear: num("gear") == 0,
            light_switch: switches & 1 != 0,
            indicators: [switches & 8 != 0, switches & 0x10 != 0],
            hazard: num("b50") == 3,
            damage,
            showroom: num("showroom") != 0,
        };
        let ms = num("ms") as u32;
        let blink = (num("ticks") / (TICK_RATE >> 1)) & 1 == 1;
        groups.update(&inputs, ms, blink);
        let at = format!("{} step {}", r["case"], r["step"]);
        let lit: String = (0..14u8).map(|g| if groups.is_lit(g) { '1' } else { '0' }).collect();
        assert_eq!(lit, r["lit"], "lit, {at}");
        for (g, want) in floats(&r["level"]).into_iter().enumerate() {
            assert_eq!(groups.level(g as u8).to_bits(), want.to_bits(), "level {g}, {at}");
        }
        let channel = r["ch11"].parse::<f64>().unwrap() as f32;
        assert_eq!(groups.headlight_channel(ms).to_bits(), channel.to_bits(), "channel 0x11, {at}");
    }
}
