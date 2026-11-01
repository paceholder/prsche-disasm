//! Camera keys (`nfs_assets::camera::input`) against the original chain run in Unicorn by
//! `scripts/research/replay-look-input.py` (fixture `local/fixtures/camera-look-input.tsv`):
//! the 128 Hz timer `0x4a45d0` polls `Input_PollControllers` `0x412e70` with the default
//! bindings `I_LOOKBEHIND = key,SC_B` and `I_CYCLECAMERA = key,SC_C` (`fe.txt`), and for each
//! new poll the recorder calls `0x4138d0` as `Race_MainLoop` `0x4152e0` does: levels for the
//! look keys (bits 0x13..0x16), the edge for the camera cycle (bit 0x17).
//! Tolerance: none (exact words and flags).
#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
use local_fixtures::Fixture;

use nfs_assets::camera::input::{look_keys, pressed, CameraKeys, OP_CYCLE_CAMERA};
use std::collections::HashMap;

static LOOK_INPUT: Fixture = Fixture::new("camera-look-input.tsv");

#[test]
fn camera_keys_match_the_original_poll_and_main_loop_reads() {
    skip_without_local_fixtures!(LOOK_INPUT);
    let mut lines = LOOK_INPUT.lines().filter(|l| !l.starts_with('#'));
    let header: HashMap<&str, usize> =
        lines.next().unwrap().split('\t').enumerate().map(|(i, k)| (k, i)).collect();
    let (mut rows, mut held_polls, mut edges) = (0, 0, 0);
    let mut previous: HashMap<String, u32> = HashMap::new();
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        let s = |k: &str| f[header[k]];
        let i = |k: &str| s(k).parse::<u64>().unwrap();
        let at = format!("{} poll {}", s("run"), s("poll"));
        let keys = CameraKeys {
            look_behind: i("look_behind") != 0,
            cycle_camera: i("cycle_camera") != 0,
            ..Default::default()
        }
        .with_modifier(i("shift") != 0);
        let word = keys.interface_word();
        assert_eq!(word as u64, i("word"), "{at}: interface word");
        let look = look_keys(word);
        assert_eq!(look.behind_5e99b0 as u64, i("g_look_behind"), "{at}: g_LookBehind");
        assert_eq!(look.left_5e99b4 as u64, i("g_look_left"), "{at}: g_LookLeft");
        assert_eq!(look.right_5e99ac as u64, i("g_look_right"), "{at}: g_LookRight");
        assert_eq!(look.forward_5e99a8 as u64, i("g_look_forward"), "{at}: g_LookForward");
        let prev = previous.insert(s("run").to_string(), word).unwrap_or(0);
        let edge = pressed(prev, word, OP_CYCLE_CAMERA);
        assert_eq!(edge as u64, i("cycle_edge"), "{at}: cycle edge");
        rows += 1;
        held_polls += look.behind_5e99b0 as usize;
        edges += edge as usize;
    }
    assert!(rows >= 100 && held_polls >= 40 && edges == 2, "{rows} rows, {held_polls} held, {edges} edges");
}
