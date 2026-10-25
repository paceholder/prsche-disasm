//! AI 5.2: closed-loop runs of the original AI driving the original car
//! model, recorded in Unicorn (scripts/research/replay-ai-loop.py): per even
//! 64 Hz step the locator update 0x474440 (+ node frame copy), the AI branch
//! of `Car_ReadInputs` 0x4991e0, `Car_DynamicsTick` 0x49fd30 and the flat
//! ground stand-in (normal up, node height + 0.5); every step the 64 Hz
//! integrations. Needs `local/game` (skipped otherwise).
//!
//! Spec: ai-drivers "AI cars run on the original car model ..." (scenario
//! "Closed loop matches the original"), "AI cars start like the original"
//! (scenarios "Realistic start", "Brake lights at the start"), "AI target
//! speed follows the corner ahead" (scenario "Slower in corners").

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_vehicle::sim::{derive, Tuning};
use nfs_assets::physics::original_vehicle::step::{
    build_ground_frame, dynamics_tick, ground_speed, integrate_orientation, integrate_position,
    update_world_points,
};
use nfs_assets::physics::original_vehicle::OriginalCar;
use nfs_assets::race::original_ai::ais::load_ais;
use nfs_assets::race::original_ai::driver::{read_inputs_ai, track_locate, AiState, AiWorld};
use nfs_assets::race::original_ai::planner::OrderSlot;
use nfs_assets::race::original_ai::track::{sim_chunks, TrackNodes};
use nfs_assets::race::original_ai::RacingLine;

static FIXTURE: Fixture = Fixture::new("ai-loop.tsv");

const WHEEL_POINTS: [[f32; 3]; 4] =
    [[-0.75, 0.0, 1.2], [0.75, 0.0, 1.2], [-0.75, 0.0, -1.2], [0.75, 0.0, -1.2]];
const BODY_POINTS: [[f32; 3]; 4] =
    [[-0.9, 0.5, 2.1], [0.9, 0.5, 2.1], [-0.9, 0.5, -2.1], [0.9, 0.5, -2.1]];

/// (run, track, reverse, sim, model, node, speed, gear, sched0, db0) as in
/// the recorder.
const RUNS: [(&str, &str, bool, &str, &str, i32, f32, i8, i32, i32); 4] = [
    ("alps_start_356A", "alps", false, "356Acoupe16", "356A", 30, 0.0, 1, 0x1a0, 0x20),
    ("alps_rolling_356A", "alps", false, "356Acoupe16", "356A", 400, 25.0, 3, 0x1000, 0x100),
    ("monaco1_start_boxster", "monaco1", false, "boxster25", "boxster", 10, 0.0, 1, 0x1a0, 0x20),
    ("alps_rev_993", "alps", true, "993coupe436", "993", 1700, 20.0, 3, 0x1000, 0x100),
];

fn game() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/GameData")
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s, 16).unwrap()
}

fn ground(car: &mut OriginalCar, track: &TrackNodes, s: &AiState) {
    car.ground_up_3a0 = 1.0;
    build_ground_frame(car, [0.0, 1.0, 0.0], 1.0);
    car.speed_35c = ground_speed(car.velocity_33c);
    let y = track.nodes[s.locator_08.node_00 as usize].pos_04[1] + 0.5;
    let dy = y - car.position_330[1];
    car.position_330[1] = y;
    for w in &mut car.wheels {
        w.point_40[1] += dy;
    }
}

#[test]
fn closed_loop_ai_matches_the_original() {
    skip_without_local_fixtures!(FIXTURE);
    if !game().exists() {
        eprintln!("skipped: no local/game");
        return;
    }
    let mut lines = FIXTURE.lines().filter(|l| !l.starts_with('#'));
    let head: Vec<&str> = lines.next().unwrap().split('\t').collect();
    let h: HashMap<&str, usize> = head.iter().enumerate().map(|(i, &k)| (k, i)).collect();
    let rows: Vec<Vec<&str>> = lines.map(|l| l.split('\t').collect()).collect();
    let g = game();
    let mut checked = 0;
    for (name, track_name, reverse, sim, model, node0, v0, gear0, sched0, db0) in RUNS {
        let crp = nfs_formats::decompress(&std::fs::read(g.join(format!("Track/{track_name}.crp"))).unwrap()).unwrap();
        let (nodes, sections) = sim_chunks(&crp).unwrap();
        let jnc = std::fs::read(g.join(format!("Track/{track_name}.jnc"))).ok();
        let track = TrackNodes::build(nodes, &sections, jnc.as_deref());
        let line = RacingLine::from_bytes(
            &std::fs::read(g.join("Track/racer/work").join(RacingLine::file_name(track_name, reverse))).unwrap(),
        );
        let raw = std::fs::read(g.join(format!("Simulation/CarData/{sim}.sim"))).unwrap();
        let mut specs = derive(&raw, &Tuning::for_model(model));
        let ais_file = std::fs::read(g.join(format!("Simulation/AICarData/{model}.ais"))).unwrap();
        let mut s140 = specs.field_140;
        let (ais, tables) = load_ais(&ais_file, model, track_name, reverse, 1.0, &mut s140);
        specs.field_140 = s140;
        let world = AiWorld {
            track: &track,
            line: &line,
            ais: &ais,
            tables: &tables,
            res_skill_154: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            skill_6573f4: 0,
            reverse_657430: reverse,
            mode_6573e8: 0,
            running_606ac0: 1,
            player_af8: 0.0,
            player_b00: 0.0,
        };
        // Initial state as the recorder sets it.
        let mut car = OriginalCar::new(specs.clone());
        car.res_14 = 1;
        car.flags_52c = 1;
        let n = &track.nodes[node0 as usize];
        let sg = if reverse { -1.0f32 } else { 1.0 };
        let (fx, fz) = ((sg * n.forward_1c[0]) as f64, (sg * n.forward_1c[2]) as f64);
        let len = (fx * fx + fz * fz).sqrt();
        let fwd = [(fx as f32 as f64 / len) as f32, 0.0, (fz as f32 as f64 / len) as f32];
        let right = [fwd[2], 0.0, -fwd[0]];
        let pos = [n.pos_04[0], n.pos_04[1] + 0.5, n.pos_04[2]];
        car.position_330 = pos;
        car.velocity_33c = [fwd[0] * v0, 0.0, fwd[2] * v0];
        car.axes_364 = [right, [0.0, 1.0, 0.0], fwd];
        car.ground_up_3a0 = 1.0;
        car.body_points_8c = BODY_POINTS.to_vec();
        for (i, w) in car.wheels.iter_mut().enumerate() {
            let wp = WHEEL_POINTS[i];
            w.point_40 = [0, 1, 2].map(|j| pos[j] + right[j] * wp[0] + fwd[j] * wp[2]);
            w.tyre.surface_8c = 1;
        }
        car.gear_d82 = gear0;
        car.gear_req_d7e = gear0;
        car.prev_gear_d81 = gear0;
        car.rpm_db4 = if gear0 > 1 { v0 * specs.gear_ratio_60[gear0 as usize] } else { 0.0 };
        car.start_db0 = db0;
        car.vel_ground_d58[2] = v0;
        car.globals.sched_step_606ac4 = sched0;
        car.globals.auto_box_658098 = 1;
        update_world_points(&mut car);
        let mut s = AiState::at_node(&track, node0);
        s.locator_08.section_04 = -1;
        ground(&mut car, &track, &s);
        let mut lcg = OriginalLcg::default();
        let want: Vec<&Vec<&str>> = rows.iter().filter(|r| r[0] == name).collect();
        let steps = if want.is_empty() { 0 } else { want.len() * 2 };
        let mut k = 0;
        let mut start_brake_lights = false;
        let mut v100: Option<usize> = None;
        let (mut tmin, mut tmax) = (f32::MAX, 0.0f32);
        for i in 0..steps {
            let even = car.globals.sched_step_606ac4 & 1 == 0;
            if even {
                track_locate(&car, &mut s, &track);
                read_inputs_ai(&mut car, &mut s, &world, &[OrderSlot::Me], 0, &mut lcg);
                dynamics_tick(&mut car, &mut lcg);
                ground(&mut car, &track, &s);
                if car.brake_eff_d80 == 0xff {
                    start_brake_lights = true;
                }
                let r = want[k];
                k += 1;
                let f = |c: &str| hex(r[h[c]]);
                let iv = |c: &str| r[h[c]].parse::<i64>().unwrap();
                let got_i: Vec<(&str, i64)> = vec![
                    ("node_08", s.locator_08.node_00 as i64),
                    ("section_0c", s.locator_08.section_04 as i64),
                    ("junction_e94", s.junction_e94.map_or(-1, |j| j as i64)),
                    ("branch_e98", s.branch_e98 as i64),
                    ("class_e9c", s.class_e9c as i64),
                    ("stuck_ea4", s.stuck_ea4 as i64),
                    ("event_d8b", car.event_d8b as i64),
                    ("gas_d9c", car.ai_gas_d9c as i64),
                    ("brake_d9d", car.ai_brake_d9d as i64),
                    ("steer_da0", car.ai_steer_da0 as i64),
                    ("gear_d82", car.gear_d82 as i64),
                    ("shift_d84", car.shift_timer_d84 as i64),
                    ("gas_d7f", car.gas_eff_d7f as i64),
                    ("brake_d80", car.brake_eff_d80 as i64),
                    ("steer_d94", car.steer_d94 as i64),
                    ("rand_state", lcg.state_5d1028 as i64),
                ];
                for (c, v) in got_i {
                    assert_eq!(v, iv(c), "{name} step {i}: {c}");
                }
                let got_f: Vec<(String, f32)> = [
                    ("skill_ea0".to_string(), s.skill_ea0),
                    ("target_eb8".into(), s.target_eb8),
                    ("rpm_db4".into(), car.rpm_db4),
                    ("fwd_d60".into(), car.vel_ground_d58[2]),
                ]
                .into_iter()
                .chain(["x", "y", "z"].iter().enumerate().flat_map(|(j, a)| {
                    [
                        (format!("pos_{a}"), car.position_330[j]),
                        (format!("vel_{a}"), car.velocity_33c[j]),
                        (format!("ang_{a}"), car.angular_388[j]),
                        (format!("f_{a}"), car.axes_364[2][j]),
                    ]
                }))
                .collect();
                for (c, v) in got_f {
                    assert_eq!(v.to_bits(), f(&c), "{name} step {i}: {c} = {v}");
                }
                checked += 1;
                tmin = tmin.min(s.target_eb8);
                tmax = tmax.max(s.target_eb8);
                if v100.is_none() && car.vel_ground_d58[2] * 3.6 >= 100.0 {
                    v100 = Some(i);
                }
            }
            integrate_position(&mut car);
            integrate_orientation(&mut car);
            car.globals.sched_step_606ac4 += 1;
        }
        if name == "monaco1_start_boxster" {
            // Slower in corners: the hairpin after the start straight.
            assert!(tmin < 0.5 * tmax, "{name}: target speed {tmin}..{tmax}");
        }
        if name.ends_with("start_356A") {
            // Realistic start: brake held by the start gate, then several
            // seconds to 100 km/h (the kinematic AI needed about 1 s).
            assert!(start_brake_lights, "{name}: brake lights at the start");
            let t = v100.map(|i| i as f32 / 64.0);
            assert!(t.is_none_or(|t| t > 8.0), "{name}: 0-100 km/h in {t:?} s");
        }
    }
    assert_eq!(checked, rows.len());
}
