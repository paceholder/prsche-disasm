//! The race-intro camera port (`nfs_assets::camera::anim`) against the original
//! `Camera_UpdateAnim 0x462f30` (AnimCam) executed in Unicorn by
//! `scripts/research/replay-camera-anim.py` (fixture `local/fixtures/camera-anim.tsv`) on the
//! `CmAn` animations of the track CRPs in `local/game` (both skipped when absent).
#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
use local_fixtures::Fixture;

use nfs_assets::camera::anim::{next_index, update_anim, CameraAnimation, CMAN_TAG};
use nfs_assets::camera::{CameraRecord, CarView};
use std::collections::HashMap;
use std::path::Path;

static ANIM: Fixture = Fixture::new("camera-anim.tsv");

fn track_anims(track: &str) -> Option<Vec<CameraAnimation>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../../local/game/GameData/Track/{track}.crp"));
    let bytes = std::fs::read(path).ok()?;
    let crp = nfs_formats::parse_track_crp(&bytes).unwrap();
    Some(
        crp.misc
            .iter()
            .filter(|e| e.tag == CMAN_TAG)
            .map(|e| CameraAnimation::parse(&e.data).unwrap())
            .collect(),
    )
}

#[test]
fn anim_camera_matches_the_original() {
    skip_without_local_fixtures!(ANIM);
    let mut lines = ANIM.lines().filter(|l| !l.starts_with('#'));
    let header: HashMap<&str, usize> = lines
        .next()
        .unwrap()
        .split('\t')
        .enumerate()
        .map(|(i, k)| (k, i))
        .collect();
    let mut anims: HashMap<String, Vec<CameraAnimation>> = HashMap::new();
    let mut checked = 0;
    let mut worst = 0.0f32;
    let mut first_step: HashMap<String, i32> = HashMap::new();
    for line in lines {
        let row: Vec<&str> = line.split('\t').collect();
        let s = |k: &str| row[header[k]];
        let f = |k: &str| s(k).parse::<f64>().unwrap() as f32;
        let i = |k: &str| s(k).parse::<i32>().unwrap();
        let track = s("track").to_string();
        if !anims.contains_key(&track) {
            let Some(list) = track_anims(&track) else {
                eprintln!("SKIPPED: local/game track {track}.crp not found");
                return;
            };
            anims.insert(track.clone(), list);
        }
        let list = &anims[&track];
        // The factory picked (index + 1) % count; the recorder logs the index after the pick.
        let index = i("index") as usize;
        assert_eq!(
            next_index((index + list.len() - 1) as u32, list.len()),
            index
        );
        let anim = &list[index];
        assert_eq!(anim.name, s("anim"));
        let start = *first_step.entry(s("case").to_string()).or_insert(i("step"));
        let elapsed = i("step") - start;
        let car = CarView {
            node_08: 0,
            pos_330: [f("pos_x"), f("pos_y"), f("pos_z")],
            axes_364: [0, 1, 2].map(|r| [0, 1, 2].map(|c| f(&format!("axis{}", 3 * r + c)))),
            half_extents_3a8: [0.9, 0.6, 2.1],
            direction_d30: 1,
            lateral_speed_d58: 0.0,
            forward_speed_d60: 0.0,
        };
        let mut rec = CameraRecord::default();
        let playing = update_anim(&mut rec, &car, anim, elapsed);
        // The end step: the original asks Camera_Set for list[0] (= 2 in the recorder).
        assert_eq!(playing, i("set_id") == -1, "{line}");
        if !playing {
            assert_eq!(elapsed, anim.duration_steps());
            continue;
        }
        let want_eye = [f("eye_x"), f("eye_y"), f("eye_z")];
        let want_m: Vec<f32> = (0..9).map(|k| f(&format!("m{k}"))).collect();
        let got_m: Vec<f32> = rec.matrix_7c.iter().flatten().copied().collect();
        for (a, b) in rec
            .eye_24
            .iter()
            .zip(&want_eye)
            .chain(got_m.iter().zip(&want_m))
        {
            worst = worst.max((a - b).abs());
        }
        assert_eq!(rec.eye_24, want_eye, "eye, {}", s("case"));
        assert_eq!(got_m, want_m, "matrix, {} step {}", s("case"), s("step"));
        checked += 1;
    }
    assert!(checked > 3000, "{checked} rows");
    eprintln!("{checked} AnimCam steps bit-exact (worst {worst})");
}
