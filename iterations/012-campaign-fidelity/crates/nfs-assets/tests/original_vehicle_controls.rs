//! V4 regression: controls recorded by executing nfs5.exe `Car_ReadInputs`
//! 0x4991e0 (with `Input_GearRequest` 0x413890) and `Car_ApplyControls`
//! 0x49dd20 in Unicorn (scripts/research/replay-vehicle-controls.py),
//! replayed through `original_vehicle::controls`. Every row must match bit
//! for bit.
//!
//! Spec: vehicle-dynamics "Controls follow the original" (scenario
//! "Auto-reverse").

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::collections::HashMap;

use nfs_assets::physics::original_vehicle::controls::{
    apply_controls, read_inputs, ControlsCar, ControlsEnv, GEAR_FIRST, GEAR_NEUTRAL, GEAR_REVERSE,
};
use nfs_assets::physics::original_vehicle::sim::{CarSpecs, CAR_SPECS_SIZE};

static CARS: Fixture = Fixture::new("vehicle-controls-cars.tsv");
static STEP: Fixture = Fixture::new("vehicle-controls-step.tsv");
static SEQ: Fixture = Fixture::new("vehicle-controls-seq.tsv");

struct Row<'a>(HashMap<&'a str, &'a str>);

impl Row<'_> {
    fn s(&self, k: &str) -> &str {
        self.0.get(k).unwrap_or_else(|| panic!("no column {k}"))
    }
    fn f(&self, k: &str) -> f32 {
        self.s(k).parse().unwrap()
    }
    fn i(&self, k: &str) -> i64 {
        self.s(k).parse().unwrap()
    }
}

fn rows(text: &str) -> Vec<Row<'_>> {
    let mut lines = text.lines().filter(|l| !l.starts_with('#'));
    let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
    lines
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), header.len());
            Row(header.iter().copied().zip(cols).collect())
        })
        .collect()
}

fn cars() -> HashMap<String, CarSpecs> {
    rows(&CARS)
        .iter()
        .map(|r| {
            let hex = r.s("specs_hex");
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            let bytes: [u8; CAR_SPECS_SIZE] = bytes.try_into().unwrap();
            (r.s("car").to_string(), CarSpecs::from_bytes(&bytes))
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum V {
    F(f32),
    I(i64),
}

/// Every state column (car fields, then globals) of the port.
fn state(car: &ControlsCar, env: &ControlsEnv) -> Vec<(&'static str, V)> {
    use V::{F, I};
    vec![
        ("remote_steer_38", I(car.remote_steer_38 as i64)),
        ("remote_brake_3a", I(car.remote_brake_3a as i64)),
        ("remote_bits_3b", I(car.remote_bits_3b as i64)),
        ("speed_35c", F(car.speed_35c)),
        ("player_520", I(car.player_520 as i64)),
        ("flags_52c", I(car.flags_52c as i64)),
        ("field_540", I(car.field_540 as i64)),
        ("field_544", I(car.field_544 as i64)),
        ("field_b50", I(car.field_b50 as i64)),
        ("field_d2c", I(car.field_d2c as i64)),
        ("vel_fwd_d60", F(car.vel_fwd_d60)),
        ("gas_d7c", I(car.gas_d7c as i64)),
        ("brake_d7d", I(car.brake_d7d as i64)),
        ("gear_req_d7e", I(car.gear_req_d7e as i64)),
        ("gas_eff_d7f", I(car.gas_eff_d7f as i64)),
        ("brake_eff_d80", I(car.brake_eff_d80 as i64)),
        ("prev_gear_d81", I(car.prev_gear_d81 as i64)),
        ("gear_d82", I(car.gear_d82 as i64)),
        ("downshift_d83", I(car.downshift_d83 as i64)),
        ("shift_timer_d84", I(car.shift_timer_d84 as i64)),
        ("handbrake_d85", I(car.handbrake_d85 as i64)),
        ("horn_d86", I(car.horn_d86 as i64)),
        ("toggles_d87", I(car.toggles_d87 as i64)),
        ("word0_d8a", I(car.word0_d8a as i64)),
        ("event_d8b", I(car.event_d8b as i64)),
        ("event_d8c", I(car.event_d8c as i64)),
        ("steer_in_d90", I(car.steer_in_d90 as i64)),
        ("steer_d94", I(car.steer_d94 as i64)),
        ("reverse_d98", I(car.reverse_d98 as i64)),
        ("ai_gas_d9c", I(car.ai_gas_d9c as i64)),
        ("ai_brake_d9d", I(car.ai_brake_d9d as i64)),
        ("ai_steer_da0", I(car.ai_steer_da0 as i64)),
        ("ai_da8", I(car.ai_da8 as i64)),
        ("remote_dac", I(car.remote_dac as i64)),
        ("start_db0", I(car.start_db0 as i64)),
        ("rpm_db4", F(car.rpm_db4)),
        ("ai_side_eb0", I(car.ai_side_eb0 as i64)),
        ("ai_side_ed0", F(car.ai_side_ed0)),
        ("lights_1230", I(car.lights_1230 as i64)),
        ("lights_1232", I(car.lights_1232 as i64)),
        ("input_word", I(env.input_word_5e9664 as i64)),
        ("mirror", I(env.mirror_65742c as i64)),
        ("throttle", F(env.throttle_628fb0)),
        ("brake", F(env.brake_628fac)),
        ("steer_abs", F(env.steer_abs_628fb4)),
    ]
}

fn car_in(r: &Row) -> ControlsCar {
    ControlsCar {
        remote_steer_38: r.i("remote_steer_38") as i8,
        remote_brake_3a: r.i("remote_brake_3a") as u8,
        remote_bits_3b: r.i("remote_bits_3b") as u8,
        speed_35c: r.f("speed_35c"),
        player_520: r.i("player_520") as i32,
        flags_52c: r.i("flags_52c") as u8,
        field_540: r.i("field_540") as i32,
        field_544: r.i("field_544") as i32,
        field_b50: r.i("field_b50") as i32,
        field_d2c: r.i("field_d2c") as i32,
        vel_fwd_d60: r.f("vel_fwd_d60"),
        gas_d7c: r.i("gas_d7c") as u8,
        brake_d7d: r.i("brake_d7d") as u8,
        gear_req_d7e: r.i("gear_req_d7e") as i8,
        gas_eff_d7f: r.i("gas_eff_d7f") as u8,
        brake_eff_d80: r.i("brake_eff_d80") as u8,
        prev_gear_d81: r.i("prev_gear_d81") as i8,
        gear_d82: r.i("gear_d82") as i8,
        downshift_d83: r.i("downshift_d83") as u8,
        shift_timer_d84: r.i("shift_timer_d84") as i8,
        handbrake_d85: r.i("handbrake_d85") as u8,
        horn_d86: r.i("horn_d86") as u8,
        toggles_d87: r.i("toggles_d87") as u8,
        word0_d8a: r.i("word0_d8a") as u8,
        event_d8b: r.i("event_d8b") as u8,
        event_d8c: r.i("event_d8c") as u8,
        steer_in_d90: r.i("steer_in_d90") as i32,
        steer_d94: r.i("steer_d94") as i32,
        reverse_d98: r.i("reverse_d98") as i32,
        ai_gas_d9c: r.i("ai_gas_d9c") as u8,
        ai_brake_d9d: r.i("ai_brake_d9d") as u8,
        ai_steer_da0: r.i("ai_steer_da0") as i32,
        ai_da8: r.i("ai_da8") as i32,
        remote_dac: r.i("remote_dac") as i32,
        start_db0: r.i("start_db0") as i32,
        rpm_db4: r.f("rpm_db4"),
        ai_side_eb0: r.i("ai_side_eb0") as i32,
        ai_side_ed0: r.f("ai_side_ed0"),
        lights_1230: r.i("lights_1230") as u16,
        lights_1232: r.i("lights_1232") as u16,
    }
}

fn env_in(r: &Row) -> ControlsEnv {
    ControlsEnv {
        input_word_5e9664: r.i("input_word") as u32,
        mirror_65742c: r.i("mirror") as i32,
        throttle_628fb0: r.f("throttle"),
        brake_628fac: r.f("brake"),
        steer_abs_628fb4: r.f("steer_abs"),
        sched_step_606ac4: r.i("sched_step") as i32,
        auto_box_658098: r.i("auto") as i32,
        handling_mode_6573f8: r.i("handling") as i32,
        ai_handbrake_turn_5e67b0: r.i("ai_turn") as i32,
        split_657404: r.i("split") as i32,
        res_steer_rate_14: r.i("res_14") as i32,
        res_light_mode_518: r.i("res_518") as i32,
    }
}

/// Runs the row's call(s) on the port.
fn run(r: &Row, specs: &CarSpecs, car: &mut ControlsCar, env: &mut ControlsEnv) {
    let call = r.s("call");
    if call == "read" || call == "step" {
        read_inputs(car, specs, env);
    }
    if call == "apply" || call == "step" {
        apply_controls(car, specs, env);
    }
}

#[derive(Default)]
struct Compare {
    mismatches: Vec<String>,
    values: usize,
}

impl Compare {
    fn outputs(&mut self, n: usize, r: &Row, car: &ControlsCar, env: &ControlsEnv) {
        for (name, port) in state(car, env) {
            self.values += 1;
            let col = format!("out_{name}");
            let same = match port {
                V::F(v) => v.to_bits() == r.f(&col).to_bits(),
                V::I(v) => v == r.i(&col),
            };
            if !same {
                self.mismatches.push(format!(
                    "row {n} ({} step {}) {name}: port {port:?}, original {}",
                    r.s("case"),
                    r.s("step"),
                    r.s(&col)
                ));
            }
        }
    }
    fn finish(self, rows: usize) {
        let shown: Vec<&str> = self
            .mismatches
            .iter()
            .take(40)
            .map(|s| s.as_str())
            .collect();
        assert!(
            self.mismatches.is_empty(),
            "{} of {} values in {rows} rows differ:\n{}",
            self.mismatches.len(),
            self.values,
            shown.join("\n")
        );
    }
}

fn replay_rows(text: &str, min_rows: usize) {
    let cars = cars();
    let rows = rows(text);
    assert!(rows.len() >= min_rows);
    let mut cmp = Compare::default();
    for (n, r) in rows.iter().enumerate() {
        let specs = &cars[r.s("car")];
        let mut car = car_in(r);
        let mut env = env_in(r);
        run(r, specs, &mut car, &mut env);
        cmp.outputs(n, r, &car, &env);
    }
    cmp.finish(rows.len());
}

#[test]
fn controls_single_calls_match_the_original_bit_for_bit() {
    skip_without_local_fixtures!(STEP);
    replay_rows(&STEP, 5000);
}

#[test]
fn controls_sequence_rows_match_the_original_bit_for_bit() {
    skip_without_local_fixtures!(SEQ);
    replay_rows(&SEQ, 1000);
}

/// Each sequence as one chain: the controls state comes from the port's
/// previous step; only the script parts (input word, scheduler step, the
/// speed trace and rpm) are taken from the row.
#[test]
fn controls_sequences_chain_like_the_original() {
    skip_without_local_fixtures!(SEQ);
    let cars = cars();
    let rows = rows(&SEQ);
    let mut cmp = Compare::default();
    let mut prev: Option<(ControlsCar, ControlsEnv)> = None;
    let mut sequences = 0;
    for (n, r) in rows.iter().enumerate() {
        let specs = &cars[r.s("car")];
        let (mut car, mut env) = match prev.take() {
            Some((mut car, mut env)) if r.i("step") != 0 => {
                car.vel_fwd_d60 = r.f("vel_fwd_d60");
                car.speed_35c = r.f("speed_35c");
                car.rpm_db4 = r.f("rpm_db4");
                env.input_word_5e9664 = r.i("input_word") as u32;
                env.sched_step_606ac4 += 2;
                assert_eq!(car, car_in(r), "row {n}: chained car state differs");
                assert_eq!(env, env_in(r), "row {n}: chained globals differ");
                (car, env)
            }
            _ => {
                sequences += 1;
                (car_in(r), env_in(r))
            }
        };
        run(r, specs, &mut car, &mut env);
        cmp.outputs(n, r, &car, &env);
        prev = Some((car, env));
    }
    assert!(sequences >= 20);
    cmp.finish(rows.len());
}

/// Input word as `Input_PollControllers` packs it.
fn word(gas: u8, brake: u8, steer: i8) -> u32 {
    ((steer / 2) as u8 as u32) << 8 | ((gas / 2) as u32) << 16 | ((brake / 2) as u32) << 24
}

/// One 32 Hz step (inputs then controls) and a simple motion model:
/// forward gears accelerate with `g_Throttle`, R backwards, `g_Brake`
/// decelerates towards 0.
fn drive(car: &mut ControlsCar, specs: &CarSpecs, env: &mut ControlsEnv, w: u32) {
    env.input_word_5e9664 = w;
    read_inputs(car, specs, env);
    apply_controls(car, specs, env);
    let mut v = car.vel_fwd_d60;
    v += match car.gear_d82 {
        GEAR_REVERSE => -5.0 * env.throttle_628fb0 / 32.0,
        GEAR_NEUTRAL => 0.0,
        _ => 8.0 * env.throttle_628fb0 / 32.0,
    };
    let dec = 9.0 * env.brake_628fac / 32.0;
    v = if v > 0.0 {
        (v - dec).max(0.0)
    } else {
        (v + dec).min(0.0)
    };
    car.vel_fwd_d60 = v;
    car.speed_35c = v.abs();
    env.sched_step_606ac4 += 2;
}

fn automatic() -> (ControlsCar, ControlsEnv) {
    let car = ControlsCar {
        gear_d82: GEAR_FIRST,
        gear_req_d7e: GEAR_FIRST,
        prev_gear_d81: GEAR_NEUTRAL,
        start_db0: 0x100,
        ..ControlsCar::default()
    };
    let env = ControlsEnv {
        sched_step_606ac4: 0x200,
        auto_box_658098: 1,
        res_steer_rate_14: 1,
        ..ControlsEnv::default()
    };
    (car, env)
}

/// Spec "Auto-reverse": stopped, brake held, gas released → the box
/// selects R and the brake input drives the car backwards; gas at a
/// standstill selects first again.
#[test]
fn auto_reverse_selects_r_and_the_brake_drives_backwards() {
    skip_without_local_fixtures!(CARS);
    let specs = cars()["Boxster"].clone();
    let (mut car, mut env) = automatic();
    car.vel_fwd_d60 = 3.0;
    car.speed_35c = 3.0;
    let mut reversed_at = None;
    for step in 0..64 {
        drive(&mut car, &specs, &mut env, word(0, 254, 0));
        if car.gear_d82 == GEAR_REVERSE && reversed_at.is_none() {
            reversed_at = Some(step);
            assert!(car.reverse_d98 == 1);
        }
        if reversed_at.is_none() {
            assert!(
                car.vel_fwd_d60 >= 0.0,
                "rolled back before R at step {step}"
            );
        }
    }
    assert!(reversed_at.is_some(), "never selected reverse");
    assert_eq!(env.throttle_628fb0, 1.0, "brake input is the throttle in R");
    assert_eq!(env.brake_628fac, 0.0);
    assert!(
        car.vel_fwd_d60 < -3.0,
        "not backing up: {}",
        car.vel_fwd_d60
    );
    // Gas brakes the reversing car, then selects first at a standstill.
    for _ in 0..64 {
        drive(&mut car, &specs, &mut env, word(254, 0, 0));
    }
    assert_eq!(car.gear_d82, GEAR_FIRST);
    assert_eq!(car.reverse_d98, 0);
    assert!(
        car.vel_fwd_d60 > 0.5,
        "not driving forward: {}",
        car.vel_fwd_d60
    );
}

/// Spec "Controls follow the original", race start: with the gas held the
/// car does not roll before step 0x1c0 (brake forced full, throttle 0 until
/// `db0` reaches 0x40), and the automatic box takes first only after it.
#[test]
fn no_roll_before_the_race_start() {
    skip_without_local_fixtures!(CARS);
    let specs = cars()["Boxster"].clone();
    let (mut car, mut env) = automatic();
    car.gear_d82 = GEAR_NEUTRAL;
    car.gear_req_d7e = GEAR_NEUTRAL;
    car.start_db0 = 1;
    env.sched_step_606ac4 = 0x100;
    let mut revved = false;
    while env.sched_step_606ac4 < 0x1c0 {
        drive(&mut car, &specs, &mut env, word(254, 0, 0));
        assert_eq!(env.brake_628fac, 1.0);
        assert_eq!(car.gear_d82, GEAR_NEUTRAL);
        assert_eq!(car.vel_fwd_d60, 0.0);
        if car.start_db0 < 0x40 {
            assert_eq!(env.throttle_628fb0, 0.0);
        }
        revved |= env.throttle_628fb0 == 1.0;
    }
    assert!(revved, "the throttle never reached the engine in N");
    for _ in 0..8 {
        drive(&mut car, &specs, &mut env, word(254, 0, 0));
    }
    assert_eq!(car.gear_d82, GEAR_FIRST);
    assert!(car.vel_fwd_d60 > 0.0);
}

/// Steering moves 12 per step toward the input (18 with the handling flag).
#[test]
fn steering_rate_limit() {
    skip_without_local_fixtures!(CARS);
    let specs = cars()["Boxster"].clone();
    for (handling, rate) in [(0, 12), (1, 18)] {
        let (mut car, mut env) = automatic();
        env.handling_mode_6573f8 = handling;
        drive(&mut car, &specs, &mut env, word(0, 0, 126));
        assert_eq!(car.steer_in_d90, 126);
        assert_eq!(car.steer_d94, rate);
    }
}
