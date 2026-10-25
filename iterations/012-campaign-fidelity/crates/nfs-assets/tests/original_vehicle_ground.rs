//! Task 6.1: the ground-contact adapter on a flat plane reproduces the V5
//! fixture runs.
//!
//! The V5 recordings (`replay-vehicle-step.py`) hold the ground fields
//! constant (`+0x3a0 = 1`, normal `+y`, `+0x424 = 0`, `w+0x84 = 0`,
//! `w+0x8c = 1`). Here the same runs go through
//! `original_vehicle::GroundAdapter` with a flat plane at the car's height
//! as the support query, and the car's first four body points at the
//! recorder's wheel points (on the ground). The adapter derives every ground
//! field itself (contact points, normal from the contact diagonals, `0x49a900`
//! height, the vertical contact of `0x49bd40`); the body and dynamics outputs
//! must equal the recording bit for bit at every step.
//!
//! Not compared: the final contact points `w+0x40` and world points
//! `+0x1dc`. The recorder only moved `w+0x40` with the body (no ground step)
//! and used other body points; the adapter rewrites `w+0x40` from the ground
//! every 32 Hz step.

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

#[path = "support/vehicle_step.rs"]
mod fixture;

use fixture::*;
use nfs_assets::physics::original_vehicle::step::update_world_points;
use nfs_assets::physics::original_vehicle::{
    scheduler_step, FlatGround, GroundAdapter, OriginalCar, StepQueue,
};

struct Flat {
    adapter: GroundAdapter,
    ground: FlatGround,
}

impl StepQueue for Flat {
    fn ground(&mut self, car: &mut OriginalCar) {
        self.adapter.ground_step(car, &mut self.ground);
    }
}

#[test]
fn flat_plane_through_the_adapter_matches_the_fixture_runs() {
    skip_without_local_fixtures!(fixture::BODY, fixture::CARS, fixture::DYN, fixture::RUNS);
    let specs = cars();
    let body = rows(&BODY);
    let dyn_rows = rows(&DYN);
    let mut total = 0;
    for run in rows(&RUNS) {
        let name = run.s("run");
        let specs = &specs[run.s("car")];
        let b: Vec<&Row> = body.iter().filter(|r| r.s("run") == name).collect();
        let d: Vec<&Row> = dyn_rows.iter().filter(|r| r.s("run") == name).collect();
        let (mut car, mut lcg) = initial(specs, &run);
        car.body_points_8c = WHEEL_POINTS.to_vec();
        update_world_points(&mut car);
        let mut queue = Flat {
            adapter: GroundAdapter::default(),
            ground: FlatGround {
                height: START_POS[1],
                surface: SURFACE as u16,
            },
        };
        let steps = run.i("steps") as usize;
        let mut k32 = 0;
        for n in 0..steps {
            let even = car.globals.sched_step_606ac4 & 1 == 0;
            let word = if even {
                d[n / 2].i("input_word") as u32
            } else {
                0
            };
            scheduler_step(&mut car, word, &mut lcg, &mut queue);
            for (k, v) in body_values(&car) {
                assert_eq!(
                    b[n].f(&k).to_bits(),
                    v.to_bits(),
                    "{name} step {n}: {k} = {v:?}, original {}",
                    b[n].s(&k)
                );
            }
            if even {
                for (k, v) in dyn_values(&car) {
                    assert!(
                        same(&v, d[k32], &k),
                        "{name} 32 Hz step {k32}: {k} = {v:?}, original {}",
                        d[k32].s(&k)
                    );
                }
                k32 += 1;
                // The ground fields the recorder held constant.
                assert_eq!(car.ground_up_3a0, 1.0, "{name} {n}");
                assert_eq!(car.height_424, 0.0, "{name} {n}");
                assert_eq!(car.ground_490[1].map(f32::abs), [0.0, 1.0, 0.0]);
                for w in &car.wheels {
                    assert_eq!(w.airborne_84, 0, "{name} {n}");
                    assert_eq!(w.tyre.surface_8c, SURFACE, "{name} {n}");
                }
            }
        }
        assert_eq!(car.globals.front_contact_628f9c, 1, "{name}");
        assert_eq!(car.globals.rear_contact_628f98, 1, "{name}");
        assert_eq!(lcg.state_5d1028 as i64, run.i("final_rand_state"), "{name}");
        total += steps;
    }
    assert_eq!(total, 7424);
}
