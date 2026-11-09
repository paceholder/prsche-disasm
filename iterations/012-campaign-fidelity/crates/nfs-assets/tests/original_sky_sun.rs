//! The original sun position `Sky_SunPosition` 0x461090 recorded in Unicorn
//! (scripts/research/replay-sky-sun.py): synthetic arguments (fixtures/sky-sun.tsv) and the
//! fields of every shipped `Sky/<track>.bin` (local/fixtures/sky-sun-tracks.tsv), compared bit
//! for bit with `nfs_formats::SkySun::position` (and, for the tracks, `parse_sky_sun` of the
//! game files).
//!
//! Spec: original-lighting (sky sun).

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;

use nfs_formats::SkySun;

const SYNTHETIC: &str = include_str!("fixtures/sky-sun.tsv");

fn bits(s: &str) -> Vec<u32> {
    s.split(',').map(|v| u32::from_str_radix(v, 16).unwrap()).collect()
}

fn check(text: &str) -> usize {
    let mut n = 0;
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.starts_with("case\t")) {
        let c: Vec<&str> = line.split('\t').collect();
        let a = bits(c[1]).into_iter().map(f32::from_bits).collect::<Vec<_>>();
        let out = bits(c[2]);
        let sun = SkySun { rotation: a[2], curvature: a[4], height: a[3], offset: [a[0], a[1]], scale: a[5] };
        let ours = sun.position().map(f32::to_bits);
        assert_eq!(&out[..3], &ours[..], "{}: {:?} vs {:?}", c[0], out.iter().map(|&v| f32::from_bits(v)).collect::<Vec<_>>(), sun.position());
        assert_eq!(out[3], 1.0f32.to_bits(), "{}: w", c[0]);
        n += 1;
    }
    n
}

#[test]
fn synthetic_sun_positions_match_bit_for_bit() {
    assert_eq!(check(SYNTHETIC), 66);
}

#[test]
fn track_sun_positions_match_bit_for_bit() {
    skip_without_local_files!("sky-sun-tracks.tsv");
    let text = std::fs::read_to_string(local_fixtures::dir().join("sky-sun-tracks.tsv")).unwrap();
    assert!(check(&text) >= 15);
}
