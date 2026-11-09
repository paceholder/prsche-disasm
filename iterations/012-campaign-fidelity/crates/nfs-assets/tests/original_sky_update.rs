//! The per-frame update of the original MULTI* sky layers recorded in Unicorn
//! (scripts/research/replay-sky-update.py): nfs5.exe `SkyManager_Update` 0x45d030 (ticks from
//! the 64 Hz `g_SchedStep`, the camera vector in the dome frame, the weather blend) -> the
//! layer update 0x45bce0 -> the UV grid update 0x45f0a0 (A/B blend 0x45f180 / 0x45f160, drift
//! and camera offset 0x45ed40, warp 0x45ee80 on the game generator 0x538040, UVs 0x45ec40),
//! on layers built by `SkyMapLayer_Ctor` 0x45b5a0 (warp start 0x45f3c0) from synthetic txt
//! blocks and a synthetic generator state. No game data: the fixture is committed.
//!
//! Compared bit for bit with `nfs_formats::sky_frame_ticks`, `sky_camera_vector` and
//! `SkyGridAnim` driven by `nfs_assets::precipitation::GameRandom` (the 0x538040 port): the
//! ticks per frame, the camera vectors, every layer's drift / camera offset / blended 1/scale /
//! sampled vertex UVs, all 256 warp offsets, the warp counters and the generator state.
//!
//! Spec: original-lighting (sky layer animation).

use std::collections::HashMap;

use nfs_assets::precipitation::GameRandom;
use nfs_formats::{parse_sky_txt, sky_camera_vector, sky_frame_ticks, SkyGridAnim, SkyMapBlock};

const RECORDING: &str = include_str!("fixtures/sky-update.tsv");
const SAMPLE: [u32; 16] = [0, 1, 7, 15, 16, 17, 40, 85, 119, 136, 170, 200, 238, 240, 254, 255];

fn f(hex: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(hex, 16).unwrap())
}

fn words(s: &str) -> Vec<u32> {
    s.split(',').map(|v| u32::from_str_radix(v, 16).unwrap()).collect()
}

fn le_f32(b: &[u8]) -> f32 {
    f32::from_le_bytes(b.try_into().unwrap())
}

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn fields(s: &str) -> HashMap<&str, &str> {
    s.split(';').filter_map(|p| p.split_once('=')).collect()
}

fn block(tokens: &str) -> SkyMapBlock {
    let txt = format!(
        "BEGIN\n\tHIGH\n\tMULTINORMAL\n{}\nEND\n",
        tokens.split(' ').map(|t| format!("\t{t}")).collect::<Vec<_>>().join("\n")
    );
    parse_sky_txt(&txt).into_iter().next().unwrap()
}

struct State {
    anim: SkyGridAnim,
    build: SkyGridAnim,
    rng: GameRandom,
    angle: f32,
    last: Option<i32>,
}

fn check_grid(s: &State, case: &str, key: &str, out: &str) {
    let layer: usize = key.rsplit_once('.').unwrap().1.parse().unwrap();
    let fl = fields(out);
    let pair = |v: &str| {
        let w = words(v);
        [w[0], w[1]]
    };
    let a = &s.anim;
    let label = format!("case {case} grid {key}");
    assert_eq!(pair(fl["offset"]), a.drift[layer].map(f32::to_bits), "{label}: drift offset");
    assert_eq!(pair(fl["camoff"]), a.camera[layer].map(f32::to_bits), "{label}: camera offset");
    assert_eq!(pair(fl["inv"]), a.inv[layer].map(f32::to_bits), "{label}: 1/scale");
    // The UVs are rewritten only while the grid is animated; otherwise they keep the build's.
    let uvs = if a.animated() { a } else { &s.build };
    for (uv, &v) in fl["uv"].split(',').zip(&SAMPLE) {
        let b = hex_bytes(uv);
        let ours = uvs.uv(layer, v % 16, v / 16);
        assert_eq!([le_f32(&b[..4]), le_f32(&b[4..])].map(f32::to_bits), ours.map(f32::to_bits), "{label}: uv of vertex {v}");
    }
}

fn check_warp(s: &State, case: &str, key: &str, out: &str) {
    let fl = fields(out);
    let label = format!("case {case} warp {key}");
    let pos = hex_bytes(fl["pos"]);
    for k in 0..256 {
        let rec = [le_f32(&pos[8 * k..8 * k + 4]), le_f32(&pos[8 * k + 4..8 * k + 8])];
        assert_eq!(rec.map(f32::to_bits), s.anim.warp[k].map(f32::to_bits), "{label}: warp offset {k}");
    }
    assert_eq!(words(fl["rng"]), s.rng.state(), "{label}: generator state");
}

#[test]
fn sky_layer_update_matches_the_original_bit_for_bit() {
    let mut states: HashMap<&str, State> = HashMap::new();
    let (mut frames, mut grids, mut warps) = (0, 0, 0);
    for line in RECORDING.lines().filter(|l| !l.starts_with('#') && !l.starts_with("kind\t")) {
        let c: Vec<&str> = line.splitn(4, '\t').collect();
        let (kind, case, key, out) = (c[0], c[1], c[2], c[3]);
        match kind {
            "case" => {
                let (tokens, rest) = out.split_once(';').unwrap();
                let fl = fields(rest);
                let block = block(tokens);
                let w = words(fl["rng"]);
                let mut rng = GameRandom::from_state([w[0], w[1], w[2], w[3], w[4], w[5]]);
                let layers = block.drawn_layers();
                // The layer build draws from the generator before the grid: the map objects'
                // sprites (0x45dd90, 200 records x 2 per map object, 0x45dd90 -> 0x5561d0).
                let maps = block.maps.iter().filter(|m| m.as_str() != "none").count();
                for _ in 0..maps * 400 {
                    rng.unit();
                }
                let anim = SkyGridAnim::new(&block, layers, &mut || rng.unit());
                let angle = f(fl["dome"].split(',').next().unwrap());
                states.insert(case, State { build: anim.clone(), anim, rng, angle, last: None });
            }
            "start" => {
                let s = &states[case];
                let fl = fields(out);
                assert_eq!(words(fl["rng"]), s.rng.state(), "case {case}: generator after the build");
            }
            "grid" => {
                check_grid(&states[case], case, key, out);
                grids += 1;
            }
            "warp" => {
                check_warp(&states[case], case, key, out);
                warps += 1;
            }
            "frame" => {
                let s = states.get_mut(case).unwrap();
                let fl = fields(out);
                let step: i32 = fl["step"].parse().unwrap();
                let ticks = sky_frame_ticks(s.last, step);
                s.last = Some(step);
                assert_eq!(ticks, fl["ticks"].parse::<i32>().unwrap(), "case {case} frame {key}: ticks");
                let cam: Vec<f32> = fl["cam"].split(',').map(f).collect();
                let v = sky_camera_vector(s.angle, cam[0], cam[1]);
                let va = hex_bytes(fl["va"]);
                assert_eq!([le_f32(&va[..4]), le_f32(&va[8..12])].map(f32::to_bits), v.map(f32::to_bits), "case {case} frame {key}: camera vector");
                let blend = f(fl["blend"]);
                assert_eq!(f(fl["arg_blend"]), blend);
                let rng = &mut s.rng;
                s.anim.update(v, blend, ticks, &mut || rng.unit());
                frames += 1;
            }
            other => panic!("row kind {other}"),
        }
    }
    assert_eq!(states.len(), 4);
    assert!(frames >= 160 && grids >= 280 && warps >= 80, "{frames} {grids} {warps}");
}
