//! Shared setup of the V5 closed-loop fixtures (`vehicle-step-*.tsv`,
//! recorded by `scripts/research/replay-vehicle-step.py`): parsing, the
//! recorder's initial state and the compared values.
#![allow(dead_code)]

use std::collections::HashMap;
use crate::local_fixtures::Fixture;

use nfs_assets::physics::original_lcg::OriginalLcg;
use nfs_assets::physics::original_vehicle::sim::{CarSpecs, CAR_SPECS_SIZE};
use nfs_assets::physics::original_vehicle::step::{build_ground_frame, ground_speed};
use nfs_assets::physics::original_vehicle::OriginalCar;

pub static CARS: Fixture = Fixture::new("vehicle-step-cars.tsv");
pub static RUNS: Fixture = Fixture::new("vehicle-step-runs.tsv");
pub static BODY: Fixture = Fixture::new("vehicle-step-body.tsv");
pub static DYN: Fixture = Fixture::new("vehicle-step-dyn.tsv");
pub static FRAME: Fixture = Fixture::new("vehicle-step-frame.tsv");

// Setup constants of the recorder (replay-vehicle-step.py).
pub const WHEEL_POINTS: [[f32; 3]; 4] = [
    [0.75, 0.0, 1.2],
    [-0.75, 0.0, 1.2],
    [0.75, 0.0, -1.2],
    [-0.75, 0.0, -1.2],
];
pub const BODY_POINTS: [[f32; 3]; 4] = [
    [0.9, 0.5, 2.1],
    [-0.9, 0.5, 2.1],
    [0.9, 0.5, -2.1],
    [-0.9, 0.5, -2.1],
];
pub const START_POS: [f32; 3] = [10.0, 0.0, 20.0];
pub const SURFACE: i32 = 1;

pub struct Row<'a>(pub HashMap<&'a str, &'a str>);

impl Row<'_> {
    pub fn s(&self, k: &str) -> &str {
        self.0.get(k).unwrap_or_else(|| panic!("no column {k}"))
    }
    pub fn f(&self, k: &str) -> f32 {
        self.s(k).parse().unwrap()
    }
    pub fn i(&self, k: &str) -> i64 {
        self.s(k).parse().unwrap()
    }
}

pub fn rows(text: &str) -> Vec<Row<'_>> {
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

pub fn cars() -> HashMap<String, CarSpecs> {
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

/// The recorder's initial state (`Recorder.setup`).
pub fn initial(specs: &CarSpecs, run: &Row) -> (OriginalCar, OriginalLcg) {
    let mut car = OriginalCar::new(specs.clone());
    let v0 = run.f("v0");
    car.res_14 = 1;
    car.position_330 = START_POS;
    car.velocity_33c = [0.0, 0.0, v0];
    car.angular_388 = [0.0, run.f("wy0"), 0.0];
    car.speed_35c = ground_speed(car.velocity_33c);
    assert_eq!(car.speed_35c.to_bits(), run.f("speed0").to_bits());
    car.body_points_8c = BODY_POINTS.to_vec();
    for (i, w) in car.wheels.iter_mut().enumerate() {
        w.point_40 = [0, 1, 2].map(|j| START_POS[j] + WHEEL_POINTS[i][j]);
        w.tyre.surface_8c = SURFACE;
    }
    let gear = run.i("gear0") as i8;
    car.gear_d82 = gear;
    car.gear_req_d7e = gear;
    car.prev_gear_d81 = gear;
    car.rpm_db4 = run.f("rpm0");
    car.start_db0 = 0x100;
    car.globals.sched_step_606ac4 = run.i("sched0") as i32;
    car.globals.auto_box_658098 = 1;
    build_ground_frame(&mut car, [0.0, 1.0, 0.0], 1.0);
    let lcg = OriginalLcg {
        state_5d1028: run.i("rand_state0") as u32,
        multiplier_5d102c: run.i("rand_mul") as u32,
        product_655a0c: 0,
    };
    (car, lcg)
}

pub fn body_values(car: &OriginalCar) -> Vec<(String, f32)> {
    let mut out = Vec::new();
    for (name, v) in [
        ("pos", car.position_330),
        ("vel", car.velocity_33c),
        ("ang", car.angular_388),
        ("r", car.axes_364[0]),
        ("u", car.axes_364[1]),
        ("f", car.axes_364[2]),
    ] {
        for (c, x) in ["x", "y", "z"].iter().zip(v) {
            out.push((format!("{name}_{c}"), x));
        }
    }
    for (i, w) in car.wheels.iter().enumerate() {
        out.push((format!("spin_88_{i}"), w.spin_88));
    }
    out
}

#[derive(Debug, PartialEq)]
pub enum V {
    F(u32),
    I(i64),
}

pub fn dyn_values(car: &OriginalCar) -> Vec<(String, V)> {
    use V::{F, I};
    let f = |x: f32| F(x.to_bits());
    let mut out: Vec<(String, V)> = [
        ("lat_d4c", f(car.accel_d4c[0])),
        ("long_d54", f(car.accel_d4c[2])),
        ("vlat_d58", f(car.vel_ground_d58[0])),
        ("vnorm_d5c", f(car.vel_ground_d58[1])),
        ("vfwd_d60", f(car.vel_ground_d58[2])),
        ("wp_d64", f(car.angular_ground_d64[0])),
        ("wy_d68", f(car.angular_ground_d64[1])),
        ("wr_d6c", f(car.angular_ground_d64[2])),
        ("slip_dc0", f(car.slip_ratio_dc0)),
        ("speed_35c", f(car.speed_35c)),
        ("rpm_db4", f(car.rpm_db4)),
        ("filt_e04", f(car.filter_e04)),
        ("filt_e08", f(car.filter_e08)),
        ("steer_ac8", f(car.steer_smooth_ac8[0])),
        ("steer_acc", f(car.steer_smooth_ac8[1])),
        ("gear_req_d7e", I(car.gear_req_d7e as i64)),
        ("prev_gear_d81", I(car.prev_gear_d81 as i64)),
        ("gear_d82", I(car.gear_d82 as i64)),
        ("shift_timer_d84", I(car.shift_timer_d84 as i64)),
        ("gas_d7f", I(car.gas_eff_d7f as i64)),
        ("brake_d80", I(car.brake_eff_d80 as i64)),
        ("downshift_d83", I(car.downshift_d83 as i64)),
        ("handbrake_d85", I(car.handbrake_d85 as i64)),
        ("steer_d94", I(car.steer_d94 as i64)),
        ("reverse_d98", I(car.reverse_d98 as i64)),
        ("limiter_548", I(car.limiter_548 as i64)),
        ("spin_db8", I(car.spin_rear_db8 as i64)),
        ("spin_dbc", I(car.spin_front_dbc as i64)),
        ("throttle", f(car.globals.throttle_628fb0)),
        ("brake", f(car.globals.brake_628fac)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    for (i, w) in car.wheels.iter().enumerate() {
        let t = &w.tyre;
        for (k, v) in [
            ("lat_90", t.lat_90),
            ("long_98", t.long_98),
            ("demand_a8", t.demand_a8),
            ("grip_ac", t.grip_ac),
            ("steer_b4", t.steer_b4),
            ("skid_b8", t.skid_b8),
        ] {
            out.push((format!("{k}_{i}"), f(v)));
        }
        out.push((format!("locked_bc_{i}"), I(t.locked_bc as i64)));
    }
    out
}

pub fn same(got: &V, r: &Row, k: &str) -> bool {
    match got {
        V::F(bits) => r.f(k).to_bits() == *bits,
        V::I(v) => r.i(k) == *v,
    }
}
