//! V3 regression: engine and automatic gearbox recorded by executing nfs5.exe
//! `Car_EngineDriveForce` 0x49e440 and `Car_AutoGearbox` 0x49daf0 in Unicorn
//! (scripts/research/replay-vehicle-engine.py), replayed through
//! `original_vehicle::engine`. Every row must match bit for bit.
//!
//! Spec: vehicle-dynamics "Engine and drag follow the original" and
//! "Automatic gearbox follows the original rules".

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::collections::HashMap;

use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_vehicle::engine::{
    auto_gearbox, engine_drive_force, EngineCar, EngineEnv,
};
use nfs_assets::physics::original_vehicle::sim::{CarSpecs, CAR_SPECS_SIZE};

static CARS: Fixture = Fixture::new("vehicle-engine-cars.tsv");
static STEP: Fixture = Fixture::new("vehicle-engine-step.tsv");
static GEARBOX: Fixture = Fixture::new("vehicle-engine-gearbox.tsv");
static SEQ: Fixture = Fixture::new("vehicle-engine-seq.tsv");

struct Row<'a>(HashMap<&'a str, &'a str>);

impl Row<'_> {
    fn s(&self, k: &str) -> &str {
        self.0.get(k).unwrap_or_else(|| panic!("no column {k}"))
    }
    fn f(&self, k: &str) -> f32 {
        self.s(k).parse().unwrap()
    }
    fn d(&self, k: &str) -> f64 {
        self.s(k).parse().unwrap()
    }
    fn i(&self, k: &str) -> i32 {
        self.s(k).parse().unwrap()
    }
    fn u(&self, k: &str) -> u32 {
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

fn specs_for(cars: &HashMap<String, CarSpecs>, r: &Row) -> CarSpecs {
    let mut specs = cars[r.s("car")].clone();
    specs.engine_type_4c = r.i("engine_type");
    specs
}

fn car_in(r: &Row) -> EngineCar {
    EngineCar {
        speed_35c: r.f("speed_35c"),
        flags_52c: r.i("flags_52c") as u8,
        limiter_548: r.i("limiter_548"),
        boost_54c: r.f("boost_54c"),
        vel_fwd_d60: r.f("vel_fwd_d60"),
        gas_d7c: r.i("gas_d7c") as u8,
        gear_req_d7e: r.i("gear_req_d7e") as i8,
        brake_d80: r.i("brake_d80") as u8,
        prev_gear_d81: r.i("prev_gear_d81") as i8,
        gear_d82: r.i("gear_d82") as i8,
        downshift_d83: r.i("downshift_d83") as u8,
        shift_timer_d84: r.i("shift_timer_d84") as i8,
        ai_da8: r.i("ai_da8"),
        rpm_db4: r.f("rpm_db4"),
        spin_rear_db8: r.i("spin_rear_db8"),
        spin_front_dbc: r.i("spin_front_dbc"),
        slip_ratio_dc0: r.f("slip_ratio_dc0"),
        grip_ac: [r.f("grip_0"), r.f("grip_1"), r.f("grip_2"), r.f("grip_3")],
    }
}

fn env_in(r: &Row) -> EngineEnv {
    EngineEnv {
        throttle_628fb0: r.f("throttle"),
        rear_contact_628f98: r.i("rear_contact"),
        sched_step_606ac4: r.i("sched_step"),
        handling_mode_6573f8: r.i("handling"),
        auto_box_658098: r.i("auto"),
        rand: OriginalLcg {
            state_5d1028: r.u("rand_state"),
            multiplier_5d102c: r.u("rand_mul"),
            product_655a0c: r.u("rand_last"),
        },
        damage_engine: r.f("dmg_engine") as f64,
        damage_gearbox: r.f("dmg_gearbox") as f64,
    }
}

/// Collects (row, field, port, original) for every differing bit pattern.
#[derive(Default)]
struct Compare {
    mismatches: Vec<String>,
    max_diff: f64,
    values: usize,
}

impl Compare {
    fn d(&mut self, row: usize, case: &str, field: &str, port: f64, original: f64) {
        self.values += 1;
        if port.to_bits() != original.to_bits() {
            let diff = (port - original).abs();
            self.max_diff = self
                .max_diff
                .max(if diff.is_nan() { f64::INFINITY } else { diff });
            self.mismatches.push(format!(
                "row {row} ({case}) {field}: port {port:e}, original {original:e}"
            ));
        }
    }
    fn f(&mut self, row: usize, case: &str, field: &str, port: f32, original: f32) {
        if port.to_bits() != original.to_bits() {
            self.d(row, case, field, port as f64, original as f64);
        } else {
            self.values += 1;
        }
    }
    fn i(&mut self, row: usize, case: &str, field: &str, port: i64, original: i64) {
        self.values += 1;
        if port != original {
            self.max_diff = f64::INFINITY;
            self.mismatches.push(format!(
                "row {row} ({case}) {field}: port {port}, original {original}"
            ));
        }
    }
    /// Every written field of a row.
    fn outputs(&mut self, n: usize, r: &Row, car: &EngineCar, env: &EngineEnv) {
        let case = r.s("case");
        self.f(
            n,
            case,
            "throttle",
            env.throttle_628fb0,
            r.f("out_throttle"),
        );
        self.i(
            n,
            case,
            "rand_state",
            env.rand.state_5d1028 as i64,
            r.u("out_rand_state") as i64,
        );
        self.i(
            n,
            case,
            "rand_last",
            env.rand.product_655a0c as i64,
            r.u("out_rand_last") as i64,
        );
        let ints = [
            ("limiter_548", car.limiter_548 as i64),
            ("prev_gear_d81", car.prev_gear_d81 as i64),
            ("gear_d82", car.gear_d82 as i64),
            ("downshift_d83", car.downshift_d83 as i64),
            ("shift_timer_d84", car.shift_timer_d84 as i64),
            ("spin_rear_db8", car.spin_rear_db8 as i64),
            ("spin_front_dbc", car.spin_front_dbc as i64),
        ];
        for (name, port) in ints {
            self.i(n, case, name, port, r.i(&format!("out_{name}")) as i64);
        }
        self.f(n, case, "boost_54c", car.boost_54c, r.f("out_boost_54c"));
        self.f(n, case, "rpm_db4", car.rpm_db4, r.f("out_rpm_db4"));
    }
    fn finish(self, rows: usize) {
        let shown: Vec<&String> = self.mismatches.iter().take(40).collect();
        assert!(
            self.mismatches.is_empty(),
            "{} of {} values in {rows} rows differ (max |diff| {:e}):\n{}",
            self.mismatches.len(),
            self.values,
            self.max_diff,
            shown
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn replay_rows(text: &str, min_rows: usize) {
    let cars = cars();
    let rows = rows(text);
    assert!(rows.len() >= min_rows);
    let mut cmp = Compare::default();
    for (n, r) in rows.iter().enumerate() {
        let specs = specs_for(&cars, r);
        let mut car = car_in(r);
        let mut env = env_in(r);
        let force = engine_drive_force(&mut car, &specs, &mut env);
        cmp.d(n, r.s("case"), "force", force, r.d("out_force"));
        cmp.outputs(n, r, &car, &env);
    }
    cmp.finish(rows.len());
}

#[test]
fn engine_steps_match_the_original_bit_for_bit() {
    skip_without_local_fixtures!(STEP);
    replay_rows(&STEP, 5000);
}

#[test]
fn engine_sequence_rows_match_the_original_bit_for_bit() {
    skip_without_local_fixtures!(SEQ);
    replay_rows(&SEQ, 4000);
}

#[test]
fn auto_gearbox_matches_the_original_bit_for_bit() {
    skip_without_local_fixtures!(GEARBOX);
    let cars = cars();
    let rows = rows(&GEARBOX);
    assert!(rows.len() >= 900);
    let mut cmp = Compare::default();
    for (n, r) in rows.iter().enumerate() {
        let specs = specs_for(&cars, r);
        let mut car = car_in(r);
        let mut env = env_in(r);
        auto_gearbox(&mut car, &specs, &mut env);
        cmp.outputs(n, r, &car, &env);
    }
    cmp.finish(rows.len());
}

/// The sequences replayed as one closed loop each: the engine state comes
/// from the port's previous step, the script parts (d84 countdown, gear
/// events, throttle, the speed trace) are applied as the recorder does.
#[test]
fn engine_sequences_chain_like_the_original() {
    skip_without_local_fixtures!(SEQ);
    let cars = cars();
    let rows = rows(&SEQ);
    let mut cmp = Compare::default();
    let mut state: Option<(EngineCar, EngineEnv, f32)> = None;
    let mut sequences = 0;
    for (n, r) in rows.iter().enumerate() {
        let specs = specs_for(&cars, r);
        if r.i("step") == 0 {
            state = None;
            sequences += 1;
        }
        let (mut car, mut env) = match state.take() {
            None => (car_in(r), env_in(r)),
            Some((mut car, mut env, v)) => {
                if car.shift_timer_d84 > 0 {
                    car.shift_timer_d84 -= 1;
                }
                if r.s("event") != "-" {
                    let old = car.gear_d82;
                    let gear = r.i("gear_d82") as i8;
                    car.prev_gear_d81 = old;
                    car.gear_d82 = gear;
                    car.shift_timer_d84 = specs.shift_time_54 as i8;
                    car.downshift_d83 = (gear < old && gear >= 2) as u8;
                }
                car.gear_req_d7e = car.gear_d82;
                env.throttle_628fb0 = r.f("throttle");
                car.gas_d7c = (r.f("throttle") * 254.0) as u8;
                car.brake_d80 = r.i("brake_d80") as u8;
                car.vel_fwd_d60 = v;
                car.speed_35c = v.abs();
                assert_eq!(car, car_in(r), "row {n}: chained input differs");
                assert_eq!(env, env_in(r), "row {n}: chained globals differ");
                (car, env)
            }
        };
        let force = engine_drive_force(&mut car, &specs, &mut env);
        cmp.d(n, r.s("case"), "force", force, r.d("out_force"));
        cmp.outputs(n, r, &car, &env);
        let mut f = force as f32;
        if r.i("traction") != 0 {
            let limit = (car.grip_ac[0] as f64
                + car.grip_ac[1] as f64
                + car.grip_ac[2] as f64
                + car.grip_ac[3] as f64) as f32;
            f = f.min(limit).max(-limit);
        }
        let v = (car.vel_fwd_d60 as f64 + f as f64 / 32.0) as f32;
        state = Some((car, env, v));
    }
    assert!(sequences >= 10);
    cmp.finish(rows.len());
}

/// Full throttle on a flat straight, automatic box, grip-limited traction,
/// `v += F/32` per 32 Hz step. Returns (speed trace, gear trace).
fn straight_run(
    specs: &CarSpecs,
    steps: usize,
    start_gear: i8,
    start_rpm: f32,
    grip: f32,
) -> (Vec<f32>, Vec<i8>) {
    let mut car = EngineCar {
        gear_d82: start_gear,
        prev_gear_d81: 1,
        gear_req_d7e: start_gear,
        rpm_db4: start_rpm,
        gas_d7c: 254,
        grip_ac: [grip; 4],
        ..EngineCar::default()
    };
    let mut env = EngineEnv {
        rear_contact_628f98: 1,
        sched_step_606ac4: 0x1c4,
        auto_box_658098: 1,
        ..EngineEnv::default()
    };
    let (mut speeds, mut gears) = (Vec::new(), Vec::new());
    let mut v = 0.0f32;
    for _ in 0..steps {
        if car.shift_timer_d84 > 0 {
            car.shift_timer_d84 -= 1;
        }
        car.gear_req_d7e = car.gear_d82;
        env.throttle_628fb0 = 1.0;
        car.vel_fwd_d60 = v;
        car.speed_35c = v.abs();
        let force = engine_drive_force(&mut car, specs, &mut env) as f32;
        let limit = grip * 4.0;
        v = (v as f64 + force.min(limit).max(-limit) as f64 / 32.0) as f32;
        speeds.push(v);
        gears.push(car.gear_d82);
    }
    (speeds, gears)
}

/// Spec "Top speed": a Boxster 2.5 at full throttle approaches `sim+0x90`.
#[test]
fn boxster_reaches_its_sim_top_speed() {
    skip_without_local_fixtures!(CARS);
    let specs = cars()["Boxster"].clone();
    let top = specs.top_speed_90;
    let (speeds, gears) = straight_run(&specs, 32 * 240, 2, 0.0, 9.0);
    let end = *speeds.last().unwrap();
    let late = speeds[speeds.len() - 32 * 10];
    assert!(
        (end - top).abs() < 0.01 * top,
        "final speed {end} m/s, sim+0x90 {top} m/s"
    );
    assert!(
        (end - late).abs() < 0.2,
        "still accelerating: {late} -> {end}"
    );
    assert_eq!(*gears.last().unwrap() as i32, specs.gear_count_50 - 1);
}

/// Spec "No upshift from wheelspin at launch": revved in N, dropped into
/// first with low grip, the wheels spin (db4 far above the wheel rpm), yet
/// the box only shifts up once the road speed gives the upshift rpm. The
/// original counts the engine rpm when it is below 1.2x the road rpm
/// (`0x49dc05`), so the road rpm must reach the upshift point (at least 0.9
/// redline) divided by 1.2; the GT2 recording shifts at 0.82 redline.
#[test]
fn no_upshift_from_wheelspin_at_launch() {
    skip_without_local_fixtures!(CARS);
    for name in ["Boxster", "993C4", "GT2"] {
        let specs = cars()[name].clone();
        let red = specs.redline_94 as f64;
        let ratio = specs.gear_ratio_60[2] as f64;
        let mut car = EngineCar {
            gear_d82: 2,
            prev_gear_d81: 1,
            gear_req_d7e: 2,
            rpm_db4: (0.95 * red) as f32,
            gas_d7c: 254,
            grip_ac: [2.0; 4],
            ..EngineCar::default()
        };
        let mut env = EngineEnv {
            rear_contact_628f98: 1,
            sched_step_606ac4: 0x1c4,
            auto_box_658098: 1,
            ..EngineEnv::default()
        };
        let mut v = 0.0f32;
        let mut spun = false;
        let mut shifted = false;
        for step in 0..32 * 20 {
            env.throttle_628fb0 = 1.0;
            car.vel_fwd_d60 = v;
            car.speed_35c = v;
            let before = car.gear_d82;
            let force = engine_drive_force(&mut car, &specs, &mut env) as f32;
            spun |= car.spin_rear_db8 == 2;
            if car.gear_d82 != before {
                let road_rpm = v as f64 * ratio;
                assert!(
                    road_rpm * 1.2 > 0.9 * red && car.rpm_db4 as f64 > 0.9 * red,
                    "{name}: upshift at step {step} with road rpm {road_rpm} (engine {})",
                    car.rpm_db4
                );
                assert!(spun, "{name}: the launch never spun the wheels");
                shifted = true;
                break;
            }
            v = (v as f64 + force.min(8.0) as f64 / 32.0) as f32;
        }
        assert!(shifted, "{name}: no upshift within 20 s");
    }
}
