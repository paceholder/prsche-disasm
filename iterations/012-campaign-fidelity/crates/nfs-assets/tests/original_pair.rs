//! Regression vectors captured by executing the original nfs5.exe x86 bytes of
//! CarPair_SeparateOverlap 0x497b10 (with CarPair_FindContact 0x496fd0),
//! CarPair_ApplyImpulse 0x496860 (with 0x494df0/0x494500) and
//! Car_ClampAngularVelocity 0x4964d0 on 212 synthetic car pairs, each with the
//! damage gate [0x657a2c] off and on (scripts/research/replay-car-pair.py),
//! plus the spec scenarios.

use nfs_assets::physics::original_pair::{
    apply_impulse, clamp_angular_velocity, find_contact, separate_overlap, PairCar, PairContact,
    PairGlobals, PairRole,
};

const SEPARATE: &str = include_str!("fixtures/car-pair-separate.tsv");
const IMPULSE: &str = include_str!("fixtures/car-pair-impulse.tsv");

fn close(label: &str, actual: f32, expected: f32, name: &str) {
    let delta = (actual - expected).abs();
    let limit = 2e-6_f32.max(2e-6 * actual.abs().max(expected.abs()));
    assert!(
        delta <= limit,
        "{name} {label}: actual={actual}, original={expected}, delta={delta}"
    );
}

struct Row<'a> {
    header: &'a [&'a str],
    cols: Vec<&'a str>,
}

impl Row<'_> {
    fn text(&self, key: &str) -> &str {
        let i = self
            .header
            .iter()
            .position(|h| *h == key)
            .unwrap_or_else(|| panic!("missing column {key}"));
        self.cols[i]
    }

    /// Fixture values are Python doubles that the harness stored as f32.
    fn f(&self, key: &str) -> f32 {
        self.text(key).parse::<f64>().unwrap() as f32
    }

    fn v(&self, key: &str) -> [f32; 3] {
        [0, 1, 2].map(|i| self.f(&format!("{key}_{i}")))
    }

    fn car(&self, p: &str) -> PairCar {
        PairCar {
            position_330: self.v(&format!("{p}_pos")),
            velocity_33c: self.v(&format!("{p}_vel")),
            mass_354: self.f(&format!("{p}_mass")),
            inv_mass_358: self.f(&format!("{p}_inv_mass")),
            speed_35c: self.f(&format!("{p}_speed")),
            axis_r_364: self.v(&format!("{p}_axis_r")),
            axis_u_370: self.v(&format!("{p}_axis_u")),
            axis_f_37c: self.v(&format!("{p}_axis_f")),
            angular_388: self.v(&format!("{p}_ang")),
            inertia_394: self.f(&format!("{p}_inertia")),
            inv_inertia_398: self.f(&format!("{p}_inv_inertia")),
            extents_3a8: self.v(&format!("{p}_ext")),
            radius_3b4: self.f(&format!("{p}_radius")),
            // replay-car-pair.py: +0x7c = 1, everything else zero.
            active_7c: true,
            ..PairCar::default()
        }
    }
}

fn rows(text: &str) -> (Vec<&str>, Vec<Vec<&str>>) {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().expect("header").split('\t').collect();
    let rows = lines
        .map(|l| {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), header.len());
            cols
        })
        .collect();
    (header, rows)
}

fn close_v(label: &str, actual: [f32; 3], expected: [f32; 3], name: &str) {
    for i in 0..3 {
        close(&format!("{label}[{i}]"), actual[i], expected[i], name);
    }
}

#[test]
fn matches_original_separation_and_contact() {
    let (header, data) = rows(SEPARATE);
    let mut count = 0;
    for cols in data {
        let row = Row {
            header: &header,
            cols,
        };
        let name = format!("{} damage={}", row.text("name"), row.text("damage"));
        let (mut a, mut b) = (row.car("a"), row.car("b"));
        let contact = separate_overlap(&mut a, &mut b);
        assert_eq!(contact.is_some(), row.text("hit") == "1", "{name} hit");
        let (point, normal) = contact.map_or(([0.0; 3], [0.0; 3]), |c| (c.point, c.normal));
        close_v("point", point, row.v("point"), &name);
        close_v("normal", normal, row.v("normal"), &name);
        close_v("a_pos", a.position_330, row.v("a_pos_out"), &name);
        close_v("b_pos", b.position_330, row.v("b_pos_out"), &name);
        count += 1;
    }
    assert_eq!(count, 424);
}

/// Globals as set by the harness: [0x657408] = [0x6573e8] = 0, LCG state
/// 0x1234, multiplier from the shipped .data.
fn harness_globals(damage: bool) -> PairGlobals {
    PairGlobals {
        damage_on_657a2c: damage,
        rand_state_5d1028: 0x1234,
        ..PairGlobals::default()
    }
}

#[test]
fn matches_original_impulse_and_clamp() {
    let (header, data) = rows(IMPULSE);
    let mut count = 0;
    for cols in data {
        let row = Row {
            header: &header,
            cols,
        };
        let name = format!("{} damage={}", row.text("name"), row.text("damage"));
        let (mut a, mut b) = (row.car("a"), row.car("b"));
        a.position_330 = row.v("a_pos_sep");
        b.position_330 = row.v("b_pos_sep");
        let contact = PairContact {
            point: row.v("point"),
            normal: row.v("normal"),
        };
        let mut globals = harness_globals(row.text("damage") == "1");
        let applied = apply_impulse(&mut a, &mut b, contact, &mut globals);
        assert_eq!(applied, row.text("applied") == "1", "{name} applied");
        for (p, car, other) in [("a", &a, PairRole::B), ("b", &b, PairRole::A)] {
            let out = |key: &str| format!("{p}_out_{key}");
            close_v(&out("pos"), car.position_330, row.v(&out("pos")), &name);
            close_v(&out("vel"), car.velocity_33c, row.v(&out("vel")), &name);
            close_v(&out("ang"), car.angular_388, row.v(&out("ang")), &name);
            close(
                &out("impact"),
                car.impact_430,
                row.f(&out("impact_0")),
                &name,
            );
            close_v(
                &out("contact_pos"),
                car.contact_pos_440,
                row.v(&out("contact_pos")),
                &name,
            );
            for (i, damage) in car.damage_4e4.iter().enumerate() {
                let key = format!("{p}_out_damage_{i}");
                close(&key, *damage, row.f(&key), &name);
            }
            let int = |key: &str| row.text(&out(key)).parse::<i64>().unwrap();
            assert_eq!(
                car.contacts_418 as i64,
                int("contacts"),
                "{name} {p} contacts"
            );
            assert_eq!(car.code_438 as i64, int("code"), "{name} {p} code");
            let expected_other = match int("other") {
                0 => None,
                _ => Some(other),
            };
            assert_eq!(car.other_434, expected_other, "{name} {p} other");
        }
        clamp_angular_velocity(&mut a);
        clamp_angular_velocity(&mut b);
        close_v(
            "a_ang_clamped",
            a.angular_388,
            row.v("a_ang_clamped"),
            &name,
        );
        close_v(
            "b_ang_clamped",
            b.angular_388,
            row.v("b_ang_clamped"),
            &name,
        );
        count += 1;
    }
    assert_eq!(count, 420);
}

fn yaw_car(pos: [f32; 3], yaw: f32, vel: [f32; 3], mass: f32) -> PairCar {
    let (s, c) = yaw.sin_cos();
    PairCar {
        position_330: pos,
        velocity_33c: vel,
        mass_354: mass,
        inv_mass_358: 1.0 / mass,
        axis_r_364: [c, 0.0, -s],
        axis_u_370: [0.0, 1.0, 0.0],
        axis_f_37c: [s, 0.0, c],
        inertia_394: 6.0 * mass,
        inv_inertia_398: 1.0 / (6.0 * mass),
        extents_3a8: [0.9, 0.65, 2.1],
        ..PairCar::default()
    }
}

/// Spec "Face hit": B's front face centre inside A gives that point and B's
/// forward axis.
#[test]
fn face_hit() {
    let a = yaw_car([0.0, 0.65, 4.0], 0.0, [0.0; 3], 1.25);
    let b = yaw_car([0.0, 0.65, 0.0], 0.0, [0.0; 3], 1.25);
    let contact = find_contact(&a, &b).expect("contact");
    assert_eq!(contact.point, [0.0, 0.65, 2.1]);
    assert_eq!(contact.normal, b.axis_f_37c);
}

/// Spec "Edge crossing is missed": crossing boxes with no face centre or
/// corner inside the other produce no contact.
#[test]
fn edge_crossing_is_missed() {
    let mut a = yaw_car(
        [1.5, 0.65, 1.0],
        std::f32::consts::FRAC_PI_2,
        [-5.0, 0.0, 0.0],
        1.25,
    );
    a.extents_3a8 = [0.9, 0.65, 3.0];
    let mut b = yaw_car([0.0, 0.65, 0.0], 0.0, [0.0; 3], 1.25);
    assert_eq!(find_contact(&a, &b), None);
    let (a0, b0) = (a.position_330, b.position_330);
    assert_eq!(separate_overlap(&mut a, &mut b), None);
    assert_eq!((a.position_330, b.position_330), (a0, b0));
}

/// Spec "Mass-weighted push": mass 1 against mass 3 at low relative speed; the
/// lighter car moves three times as far, along the centre line.
#[test]
fn mass_weighted_push() {
    let mut a = yaw_car([0.0, 0.65, 4.1], 0.0, [0.0, 0.0, -5.0], 1.0);
    let mut b = yaw_car([0.0, 0.65, 0.0], 0.0, [0.0, 0.0, -4.0], 3.0);
    let (a0, b0) = (a.position_330, b.position_330);
    assert!(separate_overlap(&mut a, &mut b).is_some());
    let moved_a = a.position_330[2] - a0[2];
    let moved_b = b0[2] - b.position_330[2];
    assert!(moved_a > 0.0 && moved_b > 0.0);
    assert!(
        (moved_a - 3.0 * moved_b).abs() < 1e-6,
        "{moved_a} vs {moved_b}"
    );
    assert_eq!([a.position_330[0], b.position_330[0]], [0.0, 0.0]);
    assert!(find_contact(&a, &b).is_none());
}

/// Spec "Separating cars": touching cars that already move apart at the
/// contact point get no impulse.
#[test]
fn separating_cars() {
    let mut a = yaw_car([-1.75, 0.65, 0.0], 0.0, [-3.0, 0.0, -20.0], 1.25);
    let mut b = yaw_car([0.0, 0.65, 0.0], 0.0, [0.0, 0.0, -20.0], 1.25);
    let contact = separate_overlap(&mut a, &mut b).expect("contact");
    let (a0, b0) = (a.clone(), b.clone());
    let mut globals = harness_globals(false);
    assert!(!apply_impulse(&mut a, &mut b, contact, &mut globals));
    assert_eq!((a, b), (a0, b0));
}

/// Spec "Hard spin": 2 turns/s of yaw is clamped to 0.75 turns/s.
#[test]
fn hard_spin() {
    let mut car = yaw_car([0.0; 3], 0.0, [0.0; 3], 1.25);
    car.angular_388 = [0.25, 2.0, -3.0];
    clamp_angular_velocity(&mut car);
    assert_eq!(car.angular_388, [0.25, 0.75, -0.75]);
}
