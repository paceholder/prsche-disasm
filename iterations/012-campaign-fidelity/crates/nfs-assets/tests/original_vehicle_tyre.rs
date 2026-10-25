//! V2 regression: grip and tyre forces recorded by executing nfs5.exe
//! `Car_WheelGripLimits` 0x4a1900, `Wheel_TyreForce` 0x4a20d0,
//! `Wheel_LockedSlide` 0x4a1c70 and `Wheel_FrictionCircle` 0x4a1dc0 in
//! Unicorn (scripts/research/replay-vehicle-tyre.py), replayed through
//! `original_vehicle::tyre`. Every row must match bit for bit.
//!
//! Spec: vehicle-dynamics "Handbrake locks the rear tyres into a sliding
//! force" and "Rolling tyres follow the original force model".

#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use std::collections::HashMap;

use nfs_assets::physics::original_vehicle::sim::{CarSpecs, CAR_SPECS_SIZE};
use nfs_assets::physics::original_vehicle::tyre::{
    friction_circle, grip_limits, locked_slide, tyre_force, TyreCar, TyreEnv, WheelRecord,
    SLIDE_FACTOR,
};

static CARS: Fixture = Fixture::new("vehicle-tyre-cars.tsv");
static GRIP: Fixture = Fixture::new("vehicle-tyre-grip.tsv");
static FORCE: Fixture = Fixture::new("vehicle-tyre-force.tsv");

struct Row<'a>(HashMap<&'a str, &'a str>);

impl Row<'_> {
    fn s(&self, k: &str) -> &str {
        self.0.get(k).unwrap_or_else(|| panic!("no column {k}"))
    }
    fn f(&self, k: &str) -> f32 {
        self.s(k).parse().unwrap()
    }
    fn i(&self, k: &str) -> i32 {
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
    specs.tyre_type_1c0 = r.i("tyre_type");
    specs
}

/// Collects (row, field, port, original) for every differing f32/i32 bit pattern.
#[derive(Default)]
struct Compare {
    mismatches: Vec<String>,
    max_diff: f64,
    values: usize,
}

impl Compare {
    fn f(&mut self, row: usize, case: &str, field: &str, port: f32, original: f32) {
        self.values += 1;
        if port.to_bits() != original.to_bits() {
            let diff = (port as f64 - original as f64).abs();
            self.max_diff = self
                .max_diff
                .max(if diff.is_nan() { f64::INFINITY } else { diff });
            self.mismatches.push(format!(
                "row {row} ({case}) {field}: port {port:e}, original {original:e}"
            ));
        }
    }
    fn i(&mut self, row: usize, case: &str, field: &str, port: i32, original: i32) {
        self.values += 1;
        if port != original {
            self.max_diff = f64::INFINITY;
            self.mismatches.push(format!(
                "row {row} ({case}) {field}: port {port}, original {original}"
            ));
        }
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

#[test]
fn grip_limits_match_the_original_bit_for_bit() {
    skip_without_local_fixtures!(GRIP);
    let cars = cars();
    let rows = rows(&GRIP);
    assert!(rows.len() > 1000);
    let mut cmp = Compare::default();
    for (n, r) in rows.iter().enumerate() {
        let specs = specs_for(&cars, r);
        let mut car = TyreCar {
            field_84: -777.0,
            ground_up_3a0: r.f("ground_up_3a0"),
            ground_normal_y_4a0: r.f("normal_y_4a0"),
            flags_52c: r.i("flags_52c") as u8,
            accel_lat_d4c: r.f("accel_lat_d4c"),
            accel_long_d54: r.f("accel_long_d54"),
            vel_fwd_d60: r.f("vel_fwd_d60"),
            gas_d7f: r.i("gas_d7f") as u8,
            ai_da8: r.i("ai_da8"),
            slip_ratio_dc0: r.f("slip_ratio_dc0"),
            ..TyreCar::default()
        };
        let env = TyreEnv {
            handling_mode_6573f8: r.i("handling"),
            autobahn_606ae0: r.i("autobahn"),
            track_query_473330: r.i("offroad") as i16,
            damage_aero: r.f("dmg_aero") as f64,
            ..TyreEnv::default()
        };
        let mut wheels: [WheelRecord; 4] = std::array::from_fn(|i| WheelRecord {
            surface_8c: r.i(&format!("surface_8c_{i}")),
            grip_ac: 12345.0,
            is_front_c0: (i < 2) as i32,
            ..WheelRecord::default()
        });
        grip_limits(&mut car, &mut wheels, &specs, &env);
        let case = r.s("case");
        for (i, w) in wheels.iter().enumerate() {
            cmp.i(
                n,
                case,
                &format!("alt_80_{i}"),
                w.alt_grip_80,
                r.i(&format!("out_alt_80_{i}")),
            );
            cmp.f(
                n,
                case,
                &format!("grip_ac_{i}"),
                w.grip_ac,
                r.f(&format!("out_grip_ac_{i}")),
            );
        }
        cmp.f(n, case, "field_84", car.field_84, r.f("out_field_84"));
    }
    cmp.finish(rows.len());
}

#[test]
fn tyre_forces_match_the_original_bit_for_bit() {
    skip_without_local_fixtures!(FORCE);
    let cars = cars();
    let rows = rows(&FORCE);
    assert!(rows.len() > 5000);
    let mut cmp = Compare::default();
    let mut locked = 0;
    for (n, r) in rows.iter().enumerate() {
        let specs = specs_for(&cars, r);
        let car = TyreCar {
            vel_fwd_d60: r.f("vel_fwd_d60"),
            slip_ratio_dc0: r.f("slip_ratio_dc0"),
            gas_d7f: r.i("gas_d7f") as u8,
            brake_d80: r.i("brake_d80") as u8,
            gear_d82: r.i("gear_d82") as i8,
            handbrake_d85: r.i("handbrake_d85") as u8,
            ai_da8: r.i("ai_da8"),
            spin_rear_db8: r.i("spin_rear_db8"),
            spin_front_dbc: r.i("spin_front_dbc"),
            res_20: r.i("res20"),
            res_24: r.i("res24"),
            ..TyreCar::default()
        };
        let env = TyreEnv {
            throttle_628fb0: r.f("throttle"),
            sched_step_606ac4: r.i("step"),
            handling_mode_6573f8: r.i("handling"),
            damage_tyres: r.f("dmg_tyres") as f64,
            ..TyreEnv::default()
        };
        let mut w = WheelRecord {
            surface_8c: r.i("surface_8c"),
            lat_90: r.f("lat_90"),
            field_94: 99.0,
            long_98: r.f("long_98"),
            cancel_lat_9c: r.f("cancel_lat_9c"),
            cancel_long_a4: r.f("cancel_long_a4"),
            demand_a8: r.f("demand_a8"),
            grip_ac: r.f("grip_ac"),
            steer_b4: r.f("steer_b4"),
            skid_b8: r.f("skid_b8"),
            locked_bc: 7,
            is_front_c0: (r.i("wheel") < 2) as i32,
            ..WheelRecord::default()
        };
        match r.s("fn") {
            "tyre" => tyre_force(&car, &mut w, &specs, &env),
            "locked" => locked_slide(&mut w, &specs, &env),
            "circle" => friction_circle(&car, &mut w, &specs, &env),
            other => panic!("unknown fn {other}"),
        }
        locked += (w.locked_bc == 1) as usize;
        let case = r.s("case");
        for (field, port) in [
            ("lat_90", w.lat_90),
            ("field_94", w.field_94),
            ("long_98", w.long_98),
            ("cancel_lat_9c", w.cancel_lat_9c),
            ("cancel_long_a4", w.cancel_long_a4),
            ("demand_a8", w.demand_a8),
            ("skid_b8", w.skid_b8),
        ] {
            cmp.f(n, case, field, port, r.f(&format!("out_{field}")));
        }
        cmp.i(n, case, "locked_bc", w.locked_bc, r.i("out_locked_bc"));
    }
    assert!(
        locked > 500,
        "fixture covers the locked path ({locked} rows)"
    );
    cmp.finish(rows.len());
}

// ---------------------------------------------------------------------------
// Spec scenarios on the port.

fn car_specs(name: &str) -> CarSpecs {
    cars()[name].clone()
}

/// A wheel rolling at `v_fwd` with side slip `v_lat`, cancel vector as
/// `Car_PlanarDynamics` builds it (no yaw term).
fn rolling(front: bool, v_fwd: f32, v_lat: f32, grip: f32) -> WheelRecord {
    WheelRecord {
        surface_8c: 3,
        cancel_lat_9c: -16.0 * v_lat,
        cancel_long_a4: -16.0 * v_fwd,
        grip_ac: grip,
        is_front_c0: front as i32,
        ..WheelRecord::default()
    }
}

fn octagonal(a: f32, b: f32) -> f64 {
    let (a, b) = ((a as f64).abs(), (b as f64).abs());
    a.max(b) + 0.25 * a.min(b)
}

#[test]
fn handbrake_locked_rear_tyre_gives_constant_force_against_slip_velocity() {
    skip_without_local_fixtures!(CARS);
    for (name, k) in [("356A", 8.0), ("993C4", 8.0), ("GT2", 6.0)] {
        let specs = car_specs(name);
        assert_eq!(SLIDE_FACTOR[specs.tyre_type_1c0 as usize], k);
        let grip = 7.0f32;
        for (v_fwd, v_lat) in [
            (30.0f32, 1.0f32),
            (30.0, -4.0),
            (12.0, 6.0),
            (-8.0, 0.5),
            (2.0, 0.0),
        ] {
            let car = TyreCar {
                vel_fwd_d60: v_fwd,
                handbrake_d85: 1,
                ..TyreCar::default()
            };
            let mut w = rolling(false, v_fwd, v_lat, grip);
            let cancel = (w.cancel_lat_9c, w.cancel_long_a4);
            tyre_force(&car, &mut w, &specs, &TyreEnv::default());
            assert_eq!(w.locked_bc, 1, "{name} {v_fwd} {v_lat}");
            // (1 - 1/k)·grip in the octagonal norm the original uses ...
            let expected = grip as f64 * (1.0 - 1.0 / k as f64);
            let got = octagonal(w.lat_90, w.long_98);
            assert!((got - expected).abs() < 1e-5, "{name}: {got} vs {expected}");
            // ... directed along the cancel vector, i.e. against the slip velocity.
            let cross = w.lat_90 as f64 * cancel.1 as f64 - w.long_98 as f64 * cancel.0 as f64;
            let dot = w.lat_90 as f64 * cancel.0 as f64 + w.long_98 as f64 * cancel.1 as f64;
            assert!(
                cross.abs() < 1e-3 * dot,
                "{name}: not along the slip velocity"
            );
            assert!(dot > 0.0);
        }
        // At 30 m/s with 1 m/s side slip the locked rear keeps only a few
        // percent of the rolling tyre's cornering force.
        let mut locked = rolling(false, 30.0, 1.0, grip);
        let mut rolling_w = locked;
        let held = TyreCar {
            vel_fwd_d60: 30.0,
            handbrake_d85: 1,
            ..TyreCar::default()
        };
        let free = TyreCar {
            handbrake_d85: 0,
            ..held
        };
        tyre_force(&held, &mut locked, &specs, &TyreEnv::default());
        tyre_force(&free, &mut rolling_w, &specs, &TyreEnv::default());
        assert_eq!(rolling_w.locked_bc, 0);
        assert_eq!(
            rolling_w.lat_90, -grip,
            "rolling rear holds the slip with full grip"
        );
        assert!(
            locked.lat_90.abs() < 0.05 * grip,
            "{name}: {}",
            locked.lat_90
        );
    }
}

#[test]
fn handbrake_needs_rear_wheel_and_speed_above_half_a_metre_per_second() {
    skip_without_local_fixtures!(CARS);
    let specs = car_specs("356A");
    let car = |v| TyreCar {
        vel_fwd_d60: v,
        handbrake_d85: 1,
        ..TyreCar::default()
    };
    let mut slow = rolling(false, 0.5, 0.1, 7.0);
    tyre_force(&car(0.5), &mut slow, &specs, &TyreEnv::default());
    assert_eq!(slow.locked_bc, 0);
    let mut front = rolling(true, 30.0, 1.0, 7.0);
    tyre_force(&car(30.0), &mut front, &specs, &TyreEnv::default());
    assert_eq!(front.locked_bc, 0);
}

#[test]
fn rolling_rear_lateral_force_saturates_from_alpha_two() {
    skip_without_local_fixtures!(CARS);
    let specs = car_specs("993C4");
    let grip = 7.0f32;
    let v_fwd = 30.0f32;
    // alpha (1/256 turn) = 256·atan(2·v_lat/v_fwd)/2π.
    let v_lat_for =
        |alpha: f64| (v_fwd as f64 / 2.0 * (alpha / 256.0 * std::f64::consts::TAU).tan()) as f32;
    let car = TyreCar {
        vel_fwd_d60: v_fwd,
        ..TyreCar::default()
    };
    let lateral = |alpha: f64| {
        let mut w = rolling(false, v_fwd, v_lat_for(alpha), grip);
        tyre_force(&car, &mut w, &specs, &TyreEnv::default());
        assert_eq!(w.locked_bc, 0);
        w.lat_90
    };
    for alpha in [2.001, 2.5, 4.0, 10.0, 40.0, 63.0] {
        assert_eq!(lateral(alpha), -grip, "alpha {alpha}");
        assert_eq!(lateral(-alpha), grip, "alpha -{alpha}");
    }
    // Below 2 it is |alpha|·grip/2 (at least grip/2), limited to |w+0x9c|.
    for alpha in [1.2, 1.5, 1.9] {
        let f = lateral(alpha) as f64;
        assert!(
            (f + alpha * grip as f64 / 2.0).abs() < 1e-3,
            "alpha {alpha}: {f}"
        );
    }
    assert!((lateral(0.9) as f64 + grip as f64 / 2.0).abs() < 1e-6);
}
