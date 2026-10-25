//! V6 regression: the original ground contact and attitude, recorded by
//! executing nfs5.exe in Unicorn (scripts/research/replay-vehicle-ground.py).
//!
//! - Closed-loop runs (3 cars x 10 scenarios: braking dive from 30 m/s,
//!   acceleration squat, steady-turn roll, a 6 cm step and a 4 cm bump
//!   under the right wheels, uphill slope, camber, a 0.4 m and a 2.5 m
//!   drop, gravel noise): the V5 step order with the real
//!   `Car_GroundContactTick` `0x49bd40` in place of the flat stand-in. The
//!   port runs `scheduler_step` with `ground_contact_tick` on the same
//!   synthetic ground and must match the body state after every 64 Hz step,
//!   and the dynamics and ground/attitude fields after every 32 Hz step.
//! - Single `0x49bd40` calls on random states (pose, tilt, height, speed,
//!   accel sums, filters, compressions, flags, light mode, remote, handling
//!   mode; flat, sloped, stepped, holed and far grounds, noisy surfaces).
//!
//! Tolerance: none. Everything matches bit for bit.
//!
//! The recorder's ground is a stub of the support owner methods (see the
//! recorder docs); the port reads the same planes through `GroundQuery`.
//! The alternate plane `+0x3e8/+0x400` is held at the start plane in the
//! closed loop (the track code that writes it is not run), so the port
//! calls `ground_contact_tick` directly, not the engine's `GroundAdapter`.
//!
//! Spec: vehicle-dynamics "Kinematic pitch and roll" (scenario "Braking
//! dive").

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

#[path = "support/vehicle_step.rs"]
mod fixture;
#[path = "support/vehicle_ground.rs"]
mod ground_fixture;

use fixture::{body_values, cars, dyn_values, rows, same, Row, V};
use ground_fixture::*;
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_vehicle::sim::CarSpecs;
use nfs_assets::physics::original_vehicle::{
    ground_contact_tick, scheduler_step, OriginalCar, StepQueue,
};

struct Queue(Terrain);

impl StepQueue for Queue {
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
        ground_contact_tick(car, &mut self.0, lcg);
    }
}

/// The recorder's initial state (`GroundRecorder.setup_run`).
fn initial(specs: &CarSpecs, run: &Row) -> (OriginalCar, OriginalLcg, Terrain) {
    let terrain = Terrain::parse(run.s("ground"));
    let mut car = OriginalCar::new(specs.clone());
    car.res_14 = 1;
    car.body_points_8c = BODY_POINTS.to_vec();
    car.half_extents_3a8 = HALF_EXTENTS;
    set_ground(&mut car, run, "init_");
    let start = terrain.pick(10.0, 20.0).unwrap();
    car.alt_normal_3e8 = start.normal;
    car.alt_base_400 = start.base;
    let gear = run.i("gear0") as i8;
    car.gear_d82 = gear;
    car.gear_req_d7e = gear;
    car.prev_gear_d81 = gear;
    car.rpm_db4 = run.f("rpm0");
    car.start_db0 = 0x100;
    car.globals.sched_step_606ac4 = run.i("sched0") as i32;
    car.globals.auto_box_658098 = 1;
    let lcg = OriginalLcg {
        state_5d1028: run.i("rand_state0") as u32,
        multiplier_5d102c: run.i("rand_mul") as u32,
        product_655a0c: 0,
    };
    (car, lcg, terrain)
}

fn check_row(name: &str, what: &str, got: Vec<(String, V)>, r: &Row) {
    for (k, v) in got {
        assert!(
            same(&v, r, &k),
            "{name} {what}: {k} = {v:?}, original {}",
            r.s(&k)
        );
    }
}

/// Replays one recorded run and checks every row; returns the port's state
/// after each 32 Hz step.
fn replay_run(specs: &CarSpecs, run: &Row, body: &[&Row], dyn_rows: &[&Row]) -> Vec<OriginalCar> {
    let name = run.s("run");
    let (mut car, mut lcg, terrain) = initial(specs, run);
    let mut q = Queue(terrain);
    let steps = run.i("steps") as usize;
    assert_eq!(body.len(), steps, "{name}");
    let mut states = Vec::new();
    for n in 0..steps {
        let even = car.globals.sched_step_606ac4 & 1 == 0;
        let word = if even {
            dyn_rows[n / 2].i("input_word") as u32
        } else {
            0
        };
        // The dynamics and ground rows are taken right after the ground
        // tick, before the integration; compare those on a copy.
        if even {
            let mut c = car.clone();
            let mut l = lcg;
            struct Until<'a>(&'a mut Queue, Option<OriginalCar>);
            impl StepQueue for Until<'_> {
                fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
                    self.0.ground(car, lcg);
                    self.1 = Some(car.clone());
                }
            }
            let mut u = Until(&mut q, None);
            scheduler_step(&mut c, word, &mut l, &mut u);
            let after = u.1.unwrap();
            let r = dyn_rows[n / 2];
            check_row(
                name,
                &format!("32 Hz step {}", n / 2),
                dyn_values(&after),
                r,
            );
            check_row(
                name,
                &format!("32 Hz step {}", n / 2),
                ground_values(&after, false),
                r,
            );
            assert_eq!(l.state_5d1028 as i64, r.i("rand_state"), "{name} {n}");
            assert_eq!(l.product_655a0c as i64, r.i("rand_last"), "{name} {n}");
            states.push(after);
        }
        scheduler_step(&mut car, word, &mut lcg, &mut q);
        for (k, v) in body_values(&car) {
            assert_eq!(
                body[n].f(&k).to_bits(),
                v.to_bits(),
                "{name} step {n}: {k} = {v:?}, original {}",
                body[n].s(&k)
            );
        }
    }
    states
}

fn by_run<'a>(rows: &'a [Row<'a>], name: &str) -> Vec<&'a Row<'a>> {
    rows.iter().filter(|r| r.s("run") == name).collect()
}

#[test]
fn closed_loop_runs_match_bit_for_bit() {
    skip_without_local_fixtures!(BODY, fixture::CARS, DYN, RUNS);
    let specs = cars();
    let body = rows(&BODY);
    let dyn_rows = rows(&DYN);
    let mut total = 0;
    for run in rows(&RUNS) {
        let name = run.s("run");
        let b = by_run(&body, name);
        let d = by_run(&dyn_rows, name);
        replay_run(&specs[run.s("car")], &run, &b, &d);
        total += run.i("steps");
    }
    assert_eq!(total, 7296);
}

#[test]
fn single_ground_ticks_match_bit_for_bit() {
    skip_without_local_fixtures!(CALL, fixture::CARS);
    let specs = cars();
    let mut n = 0;
    for r in rows(&CALL) {
        let case = r.i("case");
        let mut car = OriginalCar::new(specs[r.s("car")].clone());
        car.body_points_8c = BODY_POINTS.to_vec();
        set_ground(&mut car, &r, "in_");
        set_inputs(&mut car, &r, "in_");
        car.globals.handling_mode_6573f8 = r.i("handling") as i32;
        car.res_518 = r.i("res_518") as i32;
        car.globals.sched_step_606ac4 = r.i("sched") as i32;
        let mut lcg = OriginalLcg {
            state_5d1028: r.i("in_rand_state") as u32,
            multiplier_5d102c: r.i("rand_mul") as u32,
            product_655a0c: r.i("in_rand_last") as u32,
        };
        let mut terrain = Terrain::parse(r.s("ground"));
        ground_contact_tick(&mut car, &mut terrain, &mut lcg);
        let mut out = ground_values(&car, true);
        out.push(("rand_state".into(), V::I(lcg.state_5d1028 as i64)));
        out.push(("rand_last".into(), V::I(lcg.product_655a0c as i64)));
        for (k, v) in out {
            let col = format!("out_{k}");
            assert!(
                same(&v, &r, &col),
                "case {case} ({}): {k} = {v:?}, original {}",
                r.s("ground"),
                r.s(&col)
            );
        }
        n += 1;
    }
    assert_eq!(n, 400);
}

/// Scenario "Braking dive": braking hard from 30 m/s the nose pitches down
/// by the amount the original computes (the whole run matches the
/// recording), front compressions above the rear ones, and the body rises
/// back when the car stops.
#[test]
fn braking_dive() {
    skip_without_local_fixtures!(BODY, fixture::CARS, DYN, RUNS);
    let specs = cars();
    let body = rows(&BODY);
    let dyn_rows = rows(&DYN);
    for car in ["356A", "Boxster", "993C4"] {
        let runs = rows(&RUNS);
        let run = runs
            .iter()
            .find(|r| r.s("run") == format!("{car}/brake_dive"))
            .unwrap();
        let name = run.s("run");
        let b = by_run(&body, name);
        let d = by_run(&dyn_rows, name);
        let states = replay_run(&specs[car], run, &b, &d);
        // Nose down: the forward axis points below the horizon while braking.
        let dive = states
            .iter()
            .take(40)
            .map(|c| c.axes_364[2][1])
            .fold(0.0f32, f32::min);
        assert!(dive < -0.01, "{name}: F.y {dive}");
        let c = &states[20];
        let front = c.wheels[0].disp_58[1] + c.wheels[1].disp_58[1];
        let rear = c.wheels[2].disp_58[1] + c.wheels[3].disp_58[1];
        assert!(front > rear, "{name}: front {front} rear {rear}");
        assert!(c.pitch_filter_dc4 < 0.0, "{name}");
    }
}
