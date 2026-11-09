//! The original animated-scenery player recorded in Unicorn
//! (scripts/research/replay-lighthouse-anim.py): `WAnimBundle` 0x477710 / 0x4777f0 builds one
//! looping `CamAnim` player per offered article with an `Anqt` entry (0x471490, flags 1,
//! 0.1 s per key, started by 0x4716e0 at the current `g_SchedStep`), and per frame the bundle's
//! slot 5 0x4777b0 -> 0x4715a0 -> 0x470e90 -> 0x470f70 writes the 4x4. Compared bit for bit
//! with `camera::anim::sample_scenery(keys, g_SchedStep - start)`:
//! - synthetic CRP articles (fixtures/scenery-anim.tsv, committed): odd quaternions and
//!   scales, start steps 0 and 37 (negative elapsed), 2^31 - 1, 60 Hz and 20 Hz frame clocks with
//!   one and two calls per frame;
//! - every `Anqt` article of the shipped track CRPs (local/fixtures/scenery-anim-tracks.tsv),
//!   the Cote d'Azur lighthouse (BOX01, OBJECT01) at every step 0..6000.
//!
//! Spec: original-lighthouse-beam.

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;

use std::collections::HashMap;

use local_fixtures::Fixture;
use nfs_assets::camera::anim::{parse_anqt_keys, sample_scenery, AnimKey, SCENERY_KEY_SECONDS};

const SYNTHETIC: &str = include_str!("fixtures/scenery-anim.tsv");
static TRACKS: Fixture = Fixture::new("scenery-anim-tracks.tsv");

fn hex_words(s: &str) -> Vec<u32> {
    s.split(',').map(|v| u32::from_str_radix(v, 16).unwrap()).collect()
}

#[derive(Default, Debug)]
struct Checked {
    poses: usize,
    players: usize,
    /// (case, article) -> number of pose rows.
    per_article: HashMap<(String, i64), usize>,
    /// case -> articles taken by 0x4777f0, in offer order.
    claims: HashMap<String, Vec<i64>>,
    /// (case, name) -> article index (`article` rows of the track recording).
    names: HashMap<(String, String), i64>,
}

fn check(text: &str) -> Checked {
    let mut keys: HashMap<(String, i64), Vec<u8>> = HashMap::new();
    let mut players: HashMap<(String, i64), (i64, u32, i64)> = HashMap::new();
    let mut out = Checked::default();
    let rows = text.lines().filter(|l| !l.starts_with('#')).map(|l| l.split('\t').collect::<Vec<_>>());
    for c in rows {
        match c[0] {
            "key" => {
                let bytes: Vec<u8> = hex_words(c[4]).iter().flat_map(|w| w.to_le_bytes()).collect();
                assert_eq!(bytes.len(), 0x34);
                let k: usize = c[3].parse().unwrap();
                let v = keys.entry((c[1].into(), c[2].parse().unwrap())).or_default();
                assert_eq!(v.len(), k * 0x34, "{c:?}");
                v.extend(bytes);
            }
            "player" => {
                let (flags, keytime) = (u32::from_str_radix(c[4], 16).unwrap(), u32::from_str_radix(c[5], 16).unwrap());
                // 0x4777f0: loop flag only ([0x5b45a4] = 1), 0.1 s per key.
                assert_eq!(flags, 1, "{c:?}");
                assert_eq!(keytime, SCENERY_KEY_SECONDS.to_bits(), "{c:?}");
                // Started at creation (slot 5 0x4716e0): state 1.
                assert_eq!(c[8], "1", "{c:?}");
                let slot: i64 = c[2].parse().unwrap();
                players.insert((c[1].into(), slot), (c[3].parse().unwrap(), c[7].parse().unwrap(), c[6].parse().unwrap()));
                out.players += 1;
            }
            "article" => {
                out.names.insert((c[1].into(), c[3].into()), c[2].parse().unwrap());
            }
            "claim" => {
                let taken: Vec<i64> = c[4].split(',').filter(|s| !s.is_empty()).map(|s| s.parse().unwrap()).collect();
                assert_eq!(taken.len().to_string(), c[3], "{c:?}");
                out.claims.insert(c[1].into(), taken);
            }
            "pose" => {
                let case = c[1].to_string();
                let slot: i64 = c[2].parse().unwrap();
                let &(article, start, count) = players.get(&(case.clone(), slot)).expect("player row first");
                let raw = &keys[&(case.clone(), article)];
                let parsed: Vec<AnimKey> = parse_anqt_keys(raw);
                assert_eq!(parsed.len() as i64, count);
                let sched: u32 = c[4].parse().unwrap();
                // 0x4715a0: `g_SchedStep - start` (signed; negative -> 0 in 0x470e90).
                let elapsed = sched.wrapping_sub(start) as i32;
                assert_eq!(c[5], "1", "never finishes with the loop flag: {c:?}");
                let m = hex_words(c[6]);
                let (rows, pos) = sample_scenery(&parsed, elapsed).expect("looping sampler");
                let ours: Vec<u32> = (0..3)
                    .flat_map(|r| rows[r].iter().copied().chain([0.0]))
                    .chain(pos.iter().copied().chain([1.0]))
                    .map(f32::to_bits)
                    .collect();
                assert_eq!(
                    m,
                    ours,
                    "{case} slot {slot} article {article} step {sched}: original {:?} ours {:?}",
                    m.iter().map(|&w| f32::from_bits(w)).collect::<Vec<_>>(),
                    ours.iter().map(|&w| f32::from_bits(w)).collect::<Vec<_>>()
                );
                *out.per_article.entry((case, article)).or_default() += 1;
                out.poses += 1;
            }
            _ => {}
        }
    }
    out
}

#[test]
fn synthetic_scenery_players_match_bit_for_bit() {
    let c = check(SYNTHETIC);
    // 0x4777f0 takes every offered article with `Anqt`, with or without `tr` (article 1);
    // the static article 2 and the empty slot are left for the next bundle.
    for case in ["syn_start0", "syn_start37", "syn_fps60", "syn_fps20"] {
        assert_eq!(c.claims[case], [0, 1, 3], "{case}");
    }
    assert_eq!(c.players, 12);
    assert!(c.poses > 4000, "{}", c.poses);
}

#[test]
fn track_scenery_players_match_bit_for_bit() {
    skip_without_local_fixtures!(TRACKS);
    let c = check(&TRACKS);
    for track in ["alps", "autobahn", "castle", "coastal", "farmland", "industrial"] {
        assert!(!c.claims[track].is_empty(), "{track}");
    }
    // The lighthouse beam (BOX01 / OBJECT01 of coastal.crp): every step 0..6000, two full
    // loops of 399 keys x 6.4 steps.
    for name in ["BOX01", "OBJECT01"] {
        let article = c.names[&("coastal".to_string(), name.to_string())];
        assert_eq!(c.per_article[&("coastal".to_string(), article)], 6001, "{name}");
    }
    assert!(c.poses > 20_000, "{}", c.poses);
}
