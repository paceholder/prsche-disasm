//! V5 regression: closed-loop runs recorded by executing nfs5.exe in Unicorn
//! (scripts/research/replay-vehicle-step.py): per 64 Hz scheduler step, on
//! even steps `Car_ReadInputs` 0x4991e0 and `Car_DynamicsTick` 0x49fd30
//! (-> `Car_PlanarDynamics` 0x49f160) plus the flat-ground stand-in
//! (`Car_BuildGroundFrame` 0x49ba30, speed `+0x35c`), then
//! `Car_IntegratePosition` 0x49b7a0 and `Car_IntegrateOrientation` 0x49b8e0.
//! The port runs the same runs from the same initial state through
//! `original_vehicle::step_64hz`, feeding only the recorded input words.
//!
//! Tolerance: none. Every run (3 cars x 8 scenarios + one LCG run, 192..512
//! steps each, 7424 steps in total) matches the recording bit for bit at
//! every step, over the whole run (the planned 2e-6 per-step tolerance and
//! the aggregate bounds after the first N steps were not needed: the closed
//! loop does not drift, because every operation is reproduced exactly).
//!
//! Spec: vehicle-dynamics "Forces at 32 Hz, integration at 64 Hz" (scenario
//! "Matches the original closed loop"), "Handbrake locks the rear tyres into
//! a sliding force" (scenario "Handbrake turn at speed") and "Automatic
//! gearbox follows the original rules" (scenario "Gear matches speed after a
//! slide").

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

#[path = "support/vehicle_step.rs"]
mod fixture;

use fixture::*;
use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_vehicle::sim::CarSpecs;
use nfs_assets::physics::original_vehicle::step::{
    build_ground_frame, step_64hz, GroundContact, Inputs,
};
use nfs_assets::physics::original_vehicle::OriginalCar;

/// Port state for every recorded step of one run.
struct Replay {
    car: OriginalCar,
    lcg: OriginalLcg,
    /// The port's state after each 32 Hz step (dyn rows).
    dyn_states: Vec<OriginalCar>,
}

fn replay(
    specs: &CarSpecs,
    run: &Row,
    words: &[u32],
    check: &mut dyn FnMut(usize, &OriginalCar),
) -> Replay {
    let (mut car, mut lcg) = initial(specs, run);
    let steps = run.i("steps") as usize;
    let mut dyn_states = Vec::new();
    for n in 0..steps {
        let even = car.globals.sched_step_606ac4 & 1 == 0;
        let inputs = Inputs {
            input_word: if even { words[n / 2] } else { 0 },
            ground: GroundContact::FLAT,
        };
        step_64hz(&mut car, &inputs, &mut lcg);
        if even {
            dyn_states.push(car.clone());
        }
        check(n, &car);
    }
    Replay {
        car,
        lcg,
        dyn_states,
    }
}

#[test]
fn closed_loop_runs_match_bit_for_bit() {
    skip_without_local_fixtures!(BODY, CARS, DYN, RUNS);
    let specs = cars();
    let body = rows(&BODY);
    let dyn_rows = rows(&DYN);
    let mut total = 0;
    for run in rows(&RUNS) {
        let name = run.s("run");
        let specs = &specs[run.s("car")];
        let b: Vec<&Row> = body.iter().filter(|r| r.s("run") == name).collect();
        let d: Vec<&Row> = dyn_rows.iter().filter(|r| r.s("run") == name).collect();
        let words: Vec<u32> = d.iter().map(|r| r.i("input_word") as u32).collect();
        assert_eq!(b.len(), run.i("steps") as usize, "{name}");
        let rep = replay(specs, &run, &words, &mut |n, car| {
            let r = b[n];
            assert_eq!(r.i("step") as usize, n);
            for (k, v) in body_values(car) {
                assert_eq!(
                    r.f(&k).to_bits(),
                    v.to_bits(),
                    "{name} step {n}: {k} = {v:?}, original {}",
                    r.s(&k)
                );
            }
        });
        // The dynamics outputs are not touched by the integration, so the
        // state after the whole 64 Hz step carries them.
        assert_eq!(rep.dyn_states.len(), d.len(), "{name}");
        for (k32, (car, r)) in rep.dyn_states.iter().zip(&d).enumerate() {
            for (k, v) in dyn_values(car) {
                assert!(
                    same(&v, r, &k),
                    "{name} 32 Hz step {k32}: {k} = {v:?}, original {}",
                    r.s(&k)
                );
            }
        }
        // Final state.
        let car = &rep.car;
        let fin = |k: &str| run.f(&format!("final_{k}")).to_bits();
        for (i, w) in car.wheels.iter().enumerate() {
            for (c, v) in ["x", "y", "z"].iter().zip(w.point_40) {
                assert_eq!(
                    v.to_bits(),
                    fin(&format!("point_40_{i}_{c}")),
                    "{name} wheel {i}"
                );
            }
        }
        for (i, p) in car.world_points_1dc.iter().enumerate() {
            for (c, v) in ["x", "y", "z"].iter().zip(p) {
                assert_eq!(
                    v.to_bits(),
                    fin(&format!("world_1dc_{i}_{c}")),
                    "{name} point {i}"
                );
            }
        }
        for (c, (a, l)) in ["x", "y", "z"]
            .iter()
            .zip(car.angular_accel_d34.iter().zip(car.accel_d40))
        {
            assert_eq!(a.to_bits(), fin(&format!("wacc_d34_{c}")), "{name}");
            assert_eq!(l.to_bits(), fin(&format!("acc_d40_{c}")), "{name}");
        }
        for (k, v) in [
            ("ff_55c", car.feedback_55c[0]),
            ("ff_560", car.feedback_55c[1]),
            ("ff_564", car.feedback_55c[2]),
            ("boost_54c", car.boost_54c),
            ("field_84", car.field_84),
            ("steer_abs", car.globals.steer_abs_628fb4),
            ("grav_x", car.globals.gravity_local_628fa0[0]),
            ("grav_y", car.globals.gravity_local_628fa0[1]),
            ("grav_z", car.globals.gravity_local_628fa0[2]),
        ] {
            assert_eq!(v.to_bits(), fin(k), "{name} {k}");
        }
        let fi = |k: &str| run.i(&format!("final_{k}"));
        assert_eq!(car.reortho_80 as i64, fi("counter_80"), "{name}");
        assert_eq!(car.gas_d7c as i64, fi("gas_d7c"), "{name}");
        assert_eq!(car.brake_d7d as i64, fi("brake_d7d"), "{name}");
        assert_eq!(car.steer_in_d90 as i64, fi("steer_in_d90"), "{name}");
        assert_eq!(car.start_db0 as i64, fi("start_db0"), "{name}");
        assert_eq!(
            car.globals.front_contact_628f9c as i64,
            fi("front_contact"),
            "{name}"
        );
        assert_eq!(
            car.globals.rear_contact_628f98 as i64,
            fi("rear_contact"),
            "{name}"
        );
        assert_eq!(rep.lcg.state_5d1028 as i64, fi("rand_state"), "{name}");
        assert_eq!(rep.lcg.product_655a0c as i64, fi("rand_last"), "{name}");
        assert_eq!(
            car.globals.sched_step_606ac4 as i64,
            run.i("sched0") + run.i("steps"),
            "{name}"
        );
        total += b.len();
    }
    assert_eq!(total, 7424);
}

#[test]
fn ground_frame_matches() {
    skip_without_local_fixtures!(CARS, FRAME);
    let specs = cars();
    let specs = specs.values().next().unwrap();
    for r in rows(&FRAME) {
        let mut car = OriginalCar::new(specs.clone());
        car.flags_52c = r.i("flags_52c") as u8;
        for i in 0..9 {
            car.axes_364[i / 3][i % 3] = r.f(&format!("axes_{i}"));
        }
        let normal = [0, 1, 2].map(|i| r.f(&format!("normal_{i}")));
        car.ground_490 = [[0.0; 3]; 3];
        build_ground_frame(&mut car, normal, r.f("up"));
        for i in 0..9 {
            assert_eq!(
                car.ground_490[i / 3][i % 3].to_bits(),
                r.f(&format!("out_{i}")).to_bits(),
                "case {} out_{i}",
                r.s("case")
            );
        }
    }
}

/// The port's run of a recorded scenario (inputs from the fixture).
fn port_run(car: &str, scenario: &str) -> (CarSpecs, Vec<OriginalCar>) {
    let specs = cars().remove(car).unwrap();
    let name = format!("{car}/{scenario}");
    let runs = rows(&RUNS);
    let run = runs.iter().find(|r| r.s("run") == name).unwrap();
    let dyn_rows = rows(&DYN);
    let words: Vec<u32> = dyn_rows
        .iter()
        .filter(|r| r.s("run") == name)
        .map(|r| r.i("input_word") as u32)
        .collect();
    let rep = replay(&specs, run, &words, &mut |_, _| {});
    (specs, rep.dyn_states)
}

/// Spec "Handbrake turn at speed": at 30 m/s with slight steer, pulling the
/// handbrake makes the rear axle lose almost all cornering force, the yaw
/// rate rises and the car slides; without it the car turns gently.
#[test]
fn handbrake_turn_at_speed() {
    skip_without_local_fixtures!(CARS);
    for car in ["356A", "Boxster", "993C4"] {
        let (_, hb) = port_run(car, "handbrake");
        let (_, off) = port_run(car, "handbrake_off");
        let window = 48; // 1.5 s of 32 Hz steps
        let max = |s: &[OriginalCar], f: &dyn Fn(&OriginalCar) -> f32| {
            s[..window]
                .iter()
                .map(|c| f(c).abs())
                .fold(0.0f32, f32::max)
        };
        let yaw = |c: &OriginalCar| c.angular_388[1];
        let slip = |c: &OriginalCar| c.vel_ground_d58[0];
        // Rear wheels locked while rolling forward or back (|d60| > 0.5).
        assert!(hb[..window]
            .iter()
            .filter(|c| c.vel_fwd_d60().abs() > 0.5)
            .all(|c| c.wheels[2].tyre.locked_bc == 1 && c.wheels[3].tyre.locked_bc == 1));
        assert!(off[..window]
            .iter()
            .all(|c| c.wheels[2].tyre.locked_bc == 0));
        // Rear cornering force collapses: above 20 m/s with 1..5 m/s of
        // side slip a rolling rear tyre gives its full grip (|α| ≥ 2/256
        // turn), the locked one only 7/8·grip·|v_lat|/|v| (< 0.22 grip).
        let mut sliding = 0;
        for c in &hb[..window] {
            let vlat = c.vel_ground_d58[0].abs();
            if c.speed_35c > 20.0 && (1.0..5.0).contains(&vlat) {
                let rear = |i: usize| c.wheels[i].tyre.lat_90.abs() / c.wheels[i].tyre.grip_ac;
                assert!(
                    rear(2) < 0.3 && rear(3) < 0.3,
                    "{car}: rear lateral share {} {}",
                    rear(2),
                    rear(3)
                );
                sliding += 1;
            }
        }
        assert!(sliding > 0, "{car}: no slip window");
        // Yaw rate rises and the car slides.
        assert!(
            max(&hb, &yaw) > 3.0 * max(&off, &yaw),
            "{car}: yaw {} vs {}",
            max(&hb, &yaw),
            max(&off, &yaw)
        );
        assert!(
            max(&hb, &slip) > 5.0,
            "{car}: side slip {}",
            max(&hb, &slip)
        );
        assert!(
            max(&off, &slip) < 1.0,
            "{car}: side slip without handbrake {}",
            max(&off, &slip)
        );
    }
}

/// Spec "Gear matches speed after a slide": after a handbrake slide the car
/// recovers at full throttle; whenever it runs straight (`|dc0| < 0.01`) with
/// the clutch engaged, the selected gear is one the automatic box would keep:
/// its road-speed rpm is at most the redline and the gear below would turn
/// at least 0.8 of the redline (so no downshift is due).
#[test]
fn gear_matches_speed_after_a_slide() {
    skip_without_local_fixtures!(CARS);
    for car in ["356A", "Boxster", "993C4"] {
        let (specs, states) = port_run(car, "slide_recover");
        let red = specs.redline_94 as f64;
        let mut checked = 0;
        // The handbrake is released after 32 steps; give the car 1 s more.
        for c in &states[64..] {
            let g = c.gear_d82;
            if c.shift_timer_d84 != 0 || c.slip_ratio_dc0.abs() >= 0.01 || g < 2 {
                continue;
            }
            // Full gas (g_Throttle itself drops to 0 during limiter cuts).
            assert_eq!(c.gas_d7c, 254, "{car}");
            let v = c.vel_fwd_d60() as f64;
            let rpm = |g: i8| v * specs.gear_ratio_60[g as usize] as f64;
            assert!(rpm(g) <= red, "{car}: gear {g} at {} rpm", rpm(g));
            if g > 2 {
                assert!(
                    rpm(g - 1) >= 0.8 * red,
                    "{car}: gear {g}, the gear below at {} rpm",
                    rpm(g - 1)
                );
            }
            checked += 1;
        }
        assert!(checked > 40, "{car}: {checked} steps checked");
    }
}
