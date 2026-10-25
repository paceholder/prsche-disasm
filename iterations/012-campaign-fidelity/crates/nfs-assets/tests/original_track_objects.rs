//! Regression vectors captured by executing the original nfs5.exe x86 bytes of
//! Car_ObjectNearTest 0x498280, the cylinder response 0x498aa0 (block response
//! 0x498380) and the box response 0x498690 (0x486d70, 0x489cc0, 0x496610,
//! 0x495f30) on synthetic cars and objects, each with the damage gate
//! [0x657a2c] off and on (scripts/research/replay-track-objects.py).
//!
//! Every row stores the car struct before (`car_in`) and the dwords that
//! differ afterwards (`car_chg`). The test maps the car struct onto
//! `ObjectCar` by offset, asserts that every changed dword is a modelled field
//! (no original write is silently dropped) and that every modelled field
//! equals the original after the call (unchanged dwords keep their input).

use std::collections::{BTreeMap, BTreeSet};

use nfs_assets::physics::original_pair::{PairCar, PairGlobals, PairRole};
use nfs_assets::physics::original_track_objects::{
    BoxObject, CylinderObject, ObjectCar, box_response, cylinder_response, near_test,
};

const FIXTURE: &str = include_str!("fixtures/track-objects.tsv");

type Words = BTreeMap<u32, u32>;

fn words(text: &str) -> Words {
    text.split(';')
        .filter(|e| !e.is_empty())
        .map(|e| {
            let (o, v) = e.split_once(':').unwrap();
            (
                u32::from_str_radix(o, 16).unwrap(),
                u32::from_str_radix(v, 16).unwrap(),
            )
        })
        .collect()
}

fn f(w: &Words, off: u32) -> f32 {
    f32::from_bits(w.get(&off).copied().unwrap_or(0))
}

fn v3(w: &Words, off: u32) -> [f32; 3] {
    [f(w, off), f(w, off + 4), f(w, off + 8)]
}

fn u(w: &Words, off: u32) -> u32 {
    w.get(&off).copied().unwrap_or(0)
}

/// Map the dword dump onto the model. The resource pointer `+0x558` is the
/// only input that is not modelled: the harness zeroes the resource it points
/// to (`+0x518` = 0).
fn load_car(w: &Words, damage: bool) -> (ObjectCar, PairGlobals) {
    let count = u(w, 0x32c) as usize;
    let pair = PairCar {
        position_330: v3(w, 0x330),
        velocity_33c: v3(w, 0x33c),
        mass_354: f(w, 0x354),
        inv_mass_358: f(w, 0x358),
        speed_35c: f(w, 0x35c),
        axis_r_364: v3(w, 0x364),
        axis_u_370: v3(w, 0x370),
        axis_f_37c: v3(w, 0x37c),
        angular_388: v3(w, 0x388),
        inertia_394: f(w, 0x394),
        inv_inertia_398: f(w, 0x398),
        extents_3a8: v3(w, 0x3a8),
        radius_3b4: f(w, 0x3b4),
        active_7c: u(w, 0x7c) & 0xff != 0,
        contacts_418: u(w, 0x418) as u16 as i16,
        field_424: f(w, 0x424),
        hit_flag_42c: u(w, 0x42c) as i32,
        impact_430: f(w, 0x430),
        other_434: (u(w, 0x434) != 0).then_some(PairRole::B),
        code_438: u(w, 0x438),
        contact_pos_440: v3(w, 0x440),
        type_44c: u(w, 0x44c),
        field_450: u(w, 0x450) as i32,
        damage_4e4: std::array::from_fn(|i| f(w, 0x4e4 + 4 * i as u32)),
        flags_52c: u(w, 0x52c),
        resource_518: false,
        field_b50: u(w, 0xb50) as i32,
        local_angular_d64: v3(w, 0xd64),
        field_dac: u(w, 0xdac) as i32,
        field_db0: u(w, 0xdb0) as i32,
        field_1100: u(w, 0x1100) as i32,
        field_1104: u(w, 0x1104) as i32,
        damage_events_110c: (0..u(w, 0x110c)).map(|i| f(w, 0x1120 + 0x18 * i)).collect(),
    };
    let car = ObjectCar {
        pair,
        wheel_records_7f8: std::array::from_fn(|i| v3(w, 0x7f8 + 0xc4 * i as u32)),
        body_points_1dc: (0..count).map(|i| v3(w, 0x1dc + 12 * i as u32)).collect(),
        local_velocity_d58: v3(w, 0xd58),
        field_3a0: f(w, 0x3a0),
        field_428: f(w, 0x428),
        code_464: u(w, 0x464),
        ground_right_490: v3(w, 0x490),
        ground_forward_4a8: v3(w, 0x4a8),
    };
    let globals = PairGlobals {
        damage_on_657a2c: damage,
        // replay-track-objects.py setup_car: [0x657408] = [0x6573e8] = 0, LCG 0x1234.
        rand_state_5d1028: 0x1234,
        ..PairGlobals::default()
    };
    (car, globals)
}

/// Every modelled dword: `(offset, bits, is_float)`.
fn dump_car(car: &ObjectCar) -> Vec<(u32, u32, bool)> {
    let p = &car.pair;
    let mut out = Vec::new();
    let mut fl = |off: u32, v: &[f32]| {
        for (i, x) in v.iter().enumerate() {
            out.push((off + 4 * i as u32, x.to_bits(), true));
        }
    };
    fl(0x330, &p.position_330);
    fl(0x33c, &p.velocity_33c);
    fl(0x354, &[p.mass_354, p.inv_mass_358, p.speed_35c]);
    fl(0x364, &p.axis_r_364);
    fl(0x370, &p.axis_u_370);
    fl(0x37c, &p.axis_f_37c);
    fl(0x388, &p.angular_388);
    fl(0x394, &[p.inertia_394, p.inv_inertia_398]);
    fl(0x3a0, &[car.field_3a0]);
    fl(0x3a8, &p.extents_3a8);
    fl(0x3b4, &[p.radius_3b4]);
    fl(0x424, &[p.field_424, car.field_428]);
    fl(0x430, &[p.impact_430]);
    fl(0x440, &p.contact_pos_440);
    fl(0x490, &car.ground_right_490);
    fl(0x4a8, &car.ground_forward_4a8);
    fl(0x4e4, &p.damage_4e4);
    for (i, record) in car.wheel_records_7f8.iter().enumerate() {
        fl(0x7f8 + 0xc4 * i as u32, record);
    }
    for (i, point) in car.body_points_1dc.iter().enumerate() {
        fl(0x1dc + 12 * i as u32, point);
    }
    fl(0xd58, &car.local_velocity_d58);
    fl(0xd64, &p.local_angular_d64);
    for (i, strength) in p.damage_events_110c.iter().enumerate() {
        fl(0x1120 + 0x18 * i as u32, &[*strength]);
    }
    let mut int = |off: u32, v: u32| out.push((off, v, false));
    int(0x7c, p.active_7c as u32);
    int(0x32c, car.body_points_1dc.len() as u32);
    int(0x418, p.contacts_418 as u16 as u32);
    int(0x42c, p.hit_flag_42c as u32);
    int(0x434, p.other_434.is_some() as u32);
    int(0x438, p.code_438);
    int(0x44c, p.type_44c);
    int(0x450, p.field_450 as u32);
    int(0x464, car.code_464);
    int(0x52c, p.flags_52c);
    int(0xb50, p.field_b50 as u32);
    int(0xdac, p.field_dac as u32);
    int(0xdb0, p.field_db0 as u32);
    int(0x1100, p.field_1100 as u32);
    int(0x1104, p.field_1104 as u32);
    int(0x110c, p.damage_events_110c.len() as u32);
    for i in 0..p.damage_events_110c.len() as u32 {
        // 0x40f020(car, 1, -1, 4, 3, strength, 0)
        for (j, v) in [1, u32::MAX, 4, 3].into_iter().enumerate() {
            int(0x1110 + 0x18 * i + 4 * j as u32, v);
        }
    }
    out
}

fn close(label: &str, actual: f32, expected: f32, name: &str) {
    if actual.to_bits() == expected.to_bits() {
        return;
    }
    let delta = (actual - expected).abs();
    let limit = 2e-6_f32.max(2e-6 * actual.abs().max(expected.abs()));
    assert!(
        delta <= limit,
        "{name} {label}: actual={actual}, original={expected}, delta={delta}"
    );
}

fn check_car(name: &str, car: &ObjectCar, input: &Words, changed: &Words) {
    let dump = dump_car(car);
    let modelled: BTreeSet<u32> = dump.iter().map(|d| d.0).collect();
    for off in changed.keys() {
        assert!(
            modelled.contains(off),
            "{name}: original wrote +{off:#x}, not modelled"
        );
    }
    for off in input.keys() {
        assert!(
            modelled.contains(off) || *off == 0x558,
            "{name}: input +{off:#x} is not modelled"
        );
    }
    for (off, bits, is_float) in dump {
        let expected = changed.get(&off).or(input.get(&off)).copied().unwrap_or(0);
        let label = format!("+{off:#x}");
        if is_float {
            close(&label, f32::from_bits(bits), f32::from_bits(expected), name);
        } else if off == 0x434 {
            assert_eq!(bits != 0, expected != 0, "{name} {label}");
        } else {
            assert_eq!(bits, expected, "{name} {label}");
        }
    }
}

struct Row {
    kind: String,
    name: String,
    damage: bool,
    ret: u32,
    car_in: Words,
    obj_in: Words,
    car_chg: Words,
}

fn rows() -> Vec<Row> {
    FIXTURE
        .lines()
        .skip(1)
        .map(|line| {
            let c: Vec<&str> = line.split('\t').collect();
            assert_eq!(c.len(), 7);
            Row {
                kind: c[0].to_string(),
                name: format!("{} damage={}", c[1], c[2]),
                damage: c[2] == "1",
                ret: c[3].parse().unwrap(),
                car_in: words(c[4]),
                obj_in: words(c[5]),
                car_chg: words(c[6]),
            }
        })
        .collect()
}

#[test]
fn near_test_matches_original() {
    let mut count = 0;
    for row in rows().iter().filter(|r| r.kind == "near") {
        let (car, _) = load_car(&row.car_in, false);
        let centre = v3(&row.obj_in, 0);
        let radius = f(&row.obj_in, 0xc);
        assert_eq!(
            near_test(&car, centre, radius),
            row.ret & 0xff != 0,
            "{}",
            row.name
        );
        count += 1;
    }
    assert_eq!(count, 150);
}

#[test]
fn cylinder_response_matches_original() {
    let (mut count, mut hits) = (0, 0);
    for row in rows().iter().filter(|r| r.kind == "cyl") {
        let (mut car, mut globals) = load_car(&row.car_in, row.damage);
        let o = &row.obj_in;
        let obj = CylinderObject {
            bounds_min: v3(o, 8),
            bounds_max: v3(o, 0x14),
            radius: f(o, 0x34),
            id: u(o, 0x20),
        };
        cylinder_response(&mut car, &obj, &mut globals);
        check_car(&row.name, &car, &row.car_in, &row.car_chg);
        hits += !row.car_chg.is_empty() as usize;
        count += 1;
    }
    assert_eq!(count, 316);
    assert!(hits > 100, "only {hits} cylinder rows touch the car");
}

#[test]
fn box_response_matches_original() {
    let (mut count, mut hits) = (0, 0);
    for row in rows().iter().filter(|r| r.kind == "box") {
        let (mut car, mut globals) = load_car(&row.car_in, row.damage);
        let o = &row.obj_in;
        let obj = BoxObject {
            bounds_min: v3(o, 8),
            bounds_max: v3(o, 0x14),
            corners: std::array::from_fn(|i| v3(o, 0x34 + 12 * i as u32)),
            id: u(o, 0x20),
        };
        box_response(&mut car, &obj, &mut globals);
        check_car(&row.name, &car, &row.car_in, &row.car_chg);
        hits += !row.car_chg.is_empty() as usize;
        count += 1;
    }
    assert_eq!(count, 314);
    assert!(hits > 100, "only {hits} box rows touch the car");
}

#[test]
fn object_dynamics_tick_matches_original() {
    use nfs_assets::physics::original_smackable::{SmackBody, SmackRecord, octagon_distance};
    let record = SmackRecord {
        article_index: 0,
        tag: 0,
        position: [0.0; 3],
        axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        hull: vec![[-0.5, -0.5, -0.5], [0.5, 0.5, 0.5]],
        mass: 1.0,
        type_44c: 0,
    };
    let (mut count, mut changed) = (0, 0);
    for row in rows().iter().filter(|r| r.kind == "dyn") {
        let w = &row.car_in;
        let mut b = SmackBody::from_record(0, &record);
        b.asleep = (u(w, 0x7c) >> 8) & 0xff != 0;
        b.car.pair.active_7c = u(w, 0x7c) & 0xff != 0;
        b.car.pair.position_330 = v3(w, 0x330);
        b.car.pair.velocity_33c = v3(w, 0x33c);
        b.car.pair.angular_388 = v3(w, 0x388);
        b.car.pair.field_424 = f(w, 0x424);
        b.car.field_428 = f(w, 0x428);
        let real = &row.obj_in;
        let near = (u(real, 0x7c) >> 24) & 0xff != 0
            && octagon_distance(b.car.pair.position_330, v3(real, 0x330)) < 10.0;
        b.dynamics_tick(near);
        let exp = |off: u32| {
            f32::from_bits(
                row.car_chg
                    .get(&off)
                    .or(row.car_in.get(&off))
                    .copied()
                    .unwrap_or(0),
            )
        };
        for i in 0..3u32 {
            close("vel", b.car.pair.velocity_33c[i as usize], exp(0x33c + 4 * i), &row.name);
            close("ang", b.car.pair.angular_388[i as usize], exp(0x388 + 4 * i), &row.name);
        }
        let asleep = (row.car_chg.get(&0x7c).or(row.car_in.get(&0x7c)).copied().unwrap_or(0) >> 8) & 0xff != 0;
        assert_eq!(b.asleep, asleep, "{}", row.name);
        changed += !row.car_chg.is_empty() as usize;
        count += 1;
    }
    assert_eq!(count, 150);
    assert!(changed > 60, "{changed}");
}
