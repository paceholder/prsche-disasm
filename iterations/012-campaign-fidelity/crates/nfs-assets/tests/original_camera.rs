//! The camera port (`nfs_assets::camera`) against the original `Camera_UpdateTail 0x461e80`,
//! `Camera_UpdateHeli 0x4623d0`, `Camera_LookAt 0x464bf0`, `Camera_UpdateBumper 0x461d50` and
//! `Camera_UpdateCockpit 0x461a30`, executed in Unicorn by `scripts/research/replay-camera.py`
//! (fixtures `local/fixtures/camera-{tail,bumper}.tsv`). Each case is replayed as a chain from
//! its first row's state; the recorder's track-node strip (64 nodes, one section) and ceiling
//! stub are reproduced by `FixtureWorld`.
#[macro_use]
#[path = "support/local_fixtures.rs"]
mod local_fixtures;
#[allow(unused_imports)]
use local_fixtures::Fixture;

use nfs_assets::camera::{
    look_at, update_bumper, update_bumper2, update_heli, update_tail, CameraGlobals, CameraRecord,
    CameraWorld, CarView, LookKeys, NodeStrip, CAMERA_PARAMS, CAMERA_PARAMS_VA,
};
use std::collections::HashMap;
use std::path::PathBuf;

const TOLERANCE: f32 = 2e-6;
const NODE_COUNT: i32 = 64;

struct Tsv {
    header: HashMap<String, usize>,
    rows: Vec<Vec<String>>,
}

impl Tsv {
    fn read(name: &str) -> Self {
        let text = std::fs::read_to_string(local_fixtures::dir().join(name)).unwrap();
        let mut lines = text.lines().filter(|l| !l.starts_with('#'));
        let header = lines
            .next()
            .unwrap()
            .split('\t')
            .enumerate()
            .map(|(i, k)| (k.to_string(), i))
            .collect();
        let rows = lines
            .map(|l| l.split('\t').map(str::to_string).collect())
            .collect();
        Self { header, rows }
    }
    fn s<'a>(&self, row: &'a [String], key: &str) -> &'a str {
        &row[self.header[key]]
    }
    fn f(&self, row: &[String], key: &str) -> f32 {
        self.s(row, key).parse::<f64>().unwrap() as f32
    }
    fn i(&self, row: &[String], key: &str) -> i32 {
        self.s(row, key).parse().unwrap()
    }
    fn v(&self, row: &[String], prefix: &str) -> [f32; 3] {
        ["x", "y", "z"].map(|c| self.f(row, &format!("{prefix}_{c}")))
    }
    fn m(&self, row: &[String], prefix: &str) -> [[f32; 3]; 3] {
        let e = |i: usize| self.f(row, &format!("{prefix}{i}"));
        [[e(0), e(1), e(2)], [e(3), e(4), e(5)], [e(6), e(7), e(8)]]
    }
    /// Rows grouped by `case`, in file order.
    fn cases(&self) -> Vec<(String, Vec<&Vec<String>>)> {
        let mut out: Vec<(String, Vec<&Vec<String>>)> = Vec::new();
        for row in &self.rows {
            let name = self.s(row, "case");
            match out.last_mut() {
                Some((n, rows)) if n == name => rows.push(row),
                _ => out.push((name.to_string(), vec![row])),
            }
        }
        out
    }
}

/// Recorder world: node strip with the recorded heights, flat ceiling over a z span.
struct FixtureWorld {
    y: HashMap<i32, f32>,
    ceiling: Option<(f32, f32, f32)>,
    hits: i32,
}

impl CameraWorld for FixtureWorld {
    fn node_step(&self, node: i32, delta: i32) -> i32 {
        (node + delta).clamp(0, NODE_COUNT - 1)
    }
    fn node_y(&self, node: i32) -> f32 {
        self.y[&node]
    }
    fn clamp_under_ceiling(&mut self, car_pos: [f32; 3], pos: [f32; 3], dy: &mut f32) -> bool {
        let Some((cy, z0, z1)) = self.ceiling else {
            return false;
        };
        let car_y = car_pos[1] as f64;
        let q = (car_y + *dy as f64) as f32;
        if z0 <= pos[2] && pos[2] <= z1 && q as f64 > cy as f64 - 1.0 {
            *dy = (cy as f64 - car_y - 1.0).max(1.0) as f32;
            self.hits += 1;
            return true;
        }
        false
    }
}

#[derive(Default)]
struct Compare {
    values: usize,
    inexact: usize,
    max_err: f32,
    worst: String,
}

impl Compare {
    fn check(&mut self, what: &str, got: f32, want: f32) {
        self.values += 1;
        if got.to_bits() != want.to_bits() {
            self.inexact += 1;
            let err = (got - want).abs();
            // NaN counts as the worst error.
            if err.is_nan() || err > self.max_err {
                self.max_err = err;
                self.worst = format!("{what}: got {got:?} want {want:?}");
            }
        }
    }
    fn v(&mut self, what: &str, got: [f32; 3], want: [f32; 3]) {
        for i in 0..3 {
            self.check(&format!("{what}[{i}]"), got[i], want[i]);
        }
    }
    fn m(&mut self, what: &str, got: [[f32; 3]; 3], want: [[f32; 3]; 3]) {
        for r in 0..3 {
            self.v(&format!("{what}[{r}]"), got[r], want[r]);
        }
    }
    fn finish(&self, name: &str) {
        eprintln!(
            "{name}: {} values, {} not bit-exact, max error {:e} {}",
            self.values, self.inexact, self.max_err, self.worst
        );
        assert!(
            self.max_err <= TOLERANCE && !self.max_err.is_nan(),
            "{name}: {}",
            self.worst
        );
    }
}

fn car(t: &Tsv, row: &[String]) -> CarView {
    CarView {
        node_08: t.i(row, "node") as i16,
        pos_330: t.v(row, "pos"),
        axes_364: t.m(row, "axis"),
        half_extents_3a8: [0.0, 0.0, t.f(row, "half_len")],
        direction_d30: t.i(row, "d30"),
        lateral_speed_d58: t.f(row, "vl"),
        forward_speed_d60: t.f(row, "vf"),
    }
}

#[test]
fn tail_and_heli_match_fixture() {
    skip_without_local_files!("camera-tail.tsv");
    let t = Tsv::read("camera-tail.tsv");
    let mut cmp = Compare::default();
    let mut rows_seen = 0;
    for (name, rows) in t.cases() {
        let first = rows[0];
        let mut world = FixtureWorld {
            y: HashMap::new(),
            ceiling: (t.i(first, "ceiling") != 0).then(|| {
                (
                    t.f(first, "ceil_y"),
                    t.f(first, "ceil_z0"),
                    t.f(first, "ceil_z1"),
                )
            }),
            hits: 0,
        };
        for row in &rows {
            world.y.insert(t.i(row, "n1"), t.f(row, "y1"));
            world.y.insert(t.i(row, "n2"), t.f(row, "y2"));
        }
        let mut rec = CameraRecord {
            offset_08: t.v(first, "o_in"),
            eye_24: t.v(first, "eye_in"),
            matrix_7c: t.m(first, "axis"),
            id_bc: t.i(first, "id") as i16,
            flags_c0: t.i(first, "flags") as u8,
            settle_c1: t.i(first, "settle_in") as i8,
            ..CameraRecord::default()
        };
        let g = CameraGlobals {
            direction_flag_657430: t.i(first, "dir_flag"),
            ..CameraGlobals::default()
        };
        for row in rows {
            let c = car(&t, row);
            let keys = LookKeys {
                behind_5e99b0: t.i(row, "look_behind") != 0,
                ..LookKeys::default()
            };
            let variant = t.i(row, "variant");
            assert_eq!(world.node_step(c.node_08 as i32, 0), c.node_08 as i32);
            world.hits = 0;
            match t.s(row, "func") {
                "tail" => update_tail(&mut rec, &c, &mut world, &g, &keys, &CAMERA_PARAMS, variant),
                "heli" => update_heli(&mut rec, &c, &mut world, &g, &keys, &CAMERA_PARAMS, variant),
                f => panic!("unknown func {f}"),
            }
            let eye = rec.eye_24;
            look_at(&mut rec, c.pos_330, eye);
            let step = format!("{name}/{}", t.s(row, "step"));
            cmp.v(&format!("{step} o"), rec.offset_08, t.v(row, "o"));
            cmp.v(&format!("{step} aux"), rec.aux_14, t.v(row, "aux"));
            cmp.v(&format!("{step} eye"), rec.eye_24, t.v(row, "eye"));
            cmp.m(&format!("{step} m"), rec.matrix_7c, t.m(row, "m"));
            assert_eq!(rec.settle_c1 as i32, t.i(row, "settle"), "{step} settle");
            assert_eq!(world.hits, t.i(row, "ceiling_hits"), "{step} ceiling");
            rows_seen += 1;
        }
    }
    assert_eq!(rows_seen, 2240);
    cmp.finish("camera-tail");
}

#[test]
fn bumper_cams_match_fixture() {
    skip_without_local_files!("camera-bumper.tsv");
    let t = Tsv::read("camera-bumper.tsv");
    let mut cmp = Compare::default();
    let mut rows_seen = 0;
    for (name, rows) in t.cases() {
        let first = rows[0];
        let id = t.i(first, "id");
        let mut params = CAMERA_PARAMS;
        params[id as usize].offset = t.v(first, "cam");
        let mut rec = CameraRecord {
            offset_08: t.v(first, "o_in"),
            cockpit_rot_4c: t.v(first, "rot"),
            id_bc: id as i16,
            flags_c0: if id == 0 { 0x04 } else { 0x00 },
            look_angle_c6: t.i(first, "angle_in") as i16,
            ..CameraRecord::default()
        };
        for row in rows {
            let c = CarView {
                node_08: 0,
                pos_330: t.v(row, "pos"),
                axes_364: t.m(row, "axis"),
                half_extents_3a8: [0.0; 3],
                direction_d30: 0,
                lateral_speed_d58: 0.0,
                forward_speed_d60: 0.0,
            };
            let keys = LookKeys {
                forward_5e99a8: t.i(row, "look_forward") != 0,
                right_5e99ac: t.i(row, "look_right") != 0,
                behind_5e99b0: t.i(row, "look_behind") != 0,
                left_5e99b4: t.i(row, "look_left") != 0,
            };
            let g = CameraGlobals::default();
            if id == 0 {
                update_bumper(&mut rec, &c, &g, &keys, &params);
            } else {
                update_bumper2(&mut rec, &c, &g, &keys, &params);
            }
            let step = format!("{name}/{}", t.s(row, "step"));
            cmp.v(&format!("{step} o"), rec.offset_08, t.v(row, "o"));
            cmp.v(&format!("{step} aux"), rec.aux_14, t.v(row, "aux"));
            cmp.v(&format!("{step} eye"), rec.eye_24, t.v(row, "eye"));
            cmp.m(&format!("{step} m"), rec.matrix_7c, t.m(row, "m"));
            assert_eq!(rec.look_angle_c6 as i32, t.i(row, "angle"), "{step} angle");
            rows_seen += 1;
        }
    }
    assert_eq!(rows_seen, 1200);
    cmp.finish("camera-bumper");
}

#[test]
fn camera_params_match_exe_bytes() {
    let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../local/game/nfs5.exe");
    let Ok(data) = std::fs::read(&exe) else {
        eprintln!("skipped: {} not installed", exe.display());
        return;
    };
    // .data: VA 0x5cb000 at file offset 0x1cb000.
    let off = (CAMERA_PARAMS_VA - 0x5cb000 + 0x1cb000) as usize;
    for (i, p) in CAMERA_PARAMS.iter().enumerate() {
        let w = |k: usize| {
            let at = off + i * 16 + k * 4;
            u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
        };
        assert_eq!(p.offset.map(f32::to_bits), [w(0), w(1), w(2)], "row {i}");
        assert_eq!(p.flags, w(3), "row {i} flags");
    }
}

// ---------------------------------------------------------------------------
// Spec scenarios (openspec change original-camera, player-camera).

const HALF_LEN: f32 = 2.1;

fn flat_world() -> NodeStrip {
    NodeStrip {
        y: vec![0.0; NODE_COUNT as usize],
    }
}

/// Car with heading `yaw` (rad) and body roll `roll` (rad, about F).
fn car_at(pos: [f32; 3], yaw: f32, roll: f32) -> CarView {
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();
    // Roll about F, then yaw about world y.
    let r0 = [cr, sr, 0.0];
    let u0 = [-sr, cr, 0.0];
    let yawed = |v: [f32; 3]| [cy * v[0] + sy * v[2], v[1], -sy * v[0] + cy * v[2]];
    CarView {
        node_08: 10,
        pos_330: pos,
        axes_364: [yawed(r0), yawed(u0), [sy, 0.0, cy]],
        half_extents_3a8: [0.9, 0.65, HALF_LEN],
        direction_d30: 1,
        lateral_speed_d58: 0.0,
        forward_speed_d60: 30.0,
    }
}

fn tail_record() -> CameraRecord {
    CameraRecord {
        offset_08: [0.0, 0.0, -4.0],
        id_bc: 2,
        flags_c0: (CAMERA_PARAMS[2].flags << 1) as u8,
        settle_c1: 50,
        ..CameraRecord::default()
    }
}

fn len(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

#[test]
fn straight_driving() {
    let mut world = flat_world();
    let mut rec = tail_record();
    let g = CameraGlobals::default();
    let keys = LookKeys::default();
    let mut pos = [0.0, 0.5, 0.0];
    for _ in 0..600 {
        pos[2] += 30.0 / 64.0;
        let c = car_at(pos, 0.0, 0.0);
        update_tail(&mut rec, &c, &mut world, &g, &keys, &CAMERA_PARAMS, 0);
    }
    // 3.30 m behind the rear bumper, 1.15 m above the body centre.
    let eye = rec.eye_24;
    assert!((eye[0] - pos[0]).abs() < 1e-4);
    assert!((eye[1] - (pos[1] + 1.15)).abs() < 1e-4, "{eye:?}");
    assert!(
        (eye[2] - (pos[2] - HALF_LEN - 3.30)).abs() < 1e-4,
        "{eye:?}"
    );
}

#[test]
fn turn_in() {
    let mut world = flat_world();
    let mut rec = tail_record();
    let g = CameraGlobals::default();
    let keys = LookKeys::default();
    let mut pos = [0.0f32, 0.5, 0.0];
    let mut yaw = 0.0f32;
    for i in 0..400 {
        if i >= 200 {
            yaw += 1.2 / 64.0; // yaw into a corner at 1.2 rad/s
        }
        pos[0] += 25.0 / 64.0 * yaw.sin();
        pos[2] += 25.0 / 64.0 * yaw.cos();
        let c = car_at(pos, yaw, 0.0);
        let before = rec.offset_08;
        update_tail(&mut rec, &c, &mut world, &g, &keys, &CAMERA_PARAMS, 0);
        let o = rec.offset_08;
        if rec.settle_c1 == 0 {
            assert!(len(o) >= 4.94 - 1e-5, "step {i}: |o| = {}", len(o));
        }
        if i >= 200 {
            // Horizontal lag: 0.1 of the remaining difference per step (unless the minimum
            // distance pushed the offset out).
            let target = [
                -(HALF_LEN + 3.30) * yaw.sin(),
                1.15,
                -(HALF_LEN + 3.30) * yaw.cos(),
            ];
            let lerp = |k: usize| before[k] + (target[k] - before[k]) * 0.1;
            if (len([lerp(0), lerp(1), lerp(2)]) - len(o)).abs() < 1e-4 {
                assert!((o[0] - lerp(0)).abs() < 1e-4 && (o[2] - lerp(2)).abs() < 1e-4);
            }
            // The camera trails the heading: the offset points behind the old heading.
            let heading_of_eye = (-o[0]).atan2(-o[2]);
            assert!(heading_of_eye < yaw, "step {i}");
        }
    }
}

#[test]
fn body_roll() {
    let mut world = flat_world();
    let mut rec = tail_record();
    let g = CameraGlobals::default();
    let keys = LookKeys::default();
    let mut pos = [0.0, 0.5, 0.0];
    for i in 0..200 {
        pos[2] += 20.0 / 64.0;
        let roll = 0.17 * (i as f32 / 200.0); // up to ~10 degrees
        let c = car_at(pos, 0.0, roll);
        update_tail(&mut rec, &c, &mut world, &g, &keys, &CAMERA_PARAMS, 0);
        let eye = rec.eye_24;
        look_at(&mut rec, c.pos_330, eye);
        // World up reference: the right axis stays horizontal, so the horizon is level.
        assert_eq!(rec.matrix_7c[0][1], 0.0, "step {i}");
        assert!(rec.matrix_7c[1][1] > 0.9);
    }
}

/// Scenario "550 in-car view": BumperCam2 puts the eye at the 550's `drivercamposstyle0`.
#[test]
fn in_car_view_550() {
    use nfs_assets::camera::{DriverCamera, PlayerCamera};
    let drivercam = DriverCamera {
        position: [-0.239375, 0.425564, -0.121614],
        rotation_deg: [9.600019, 1.799999, 0.0],
    };
    // Cycle index 1 = BumperCam2.
    let mut cam = PlayerCamera::new(drivercam, 1);
    let mut world = flat_world();
    let mut car = car_at([12.0, 0.6, -40.0], 0.0, 0.0);
    for i in 0..128 {
        car = car_at([12.0, 0.6, -40.0 + i as f32 * 0.4], 0.0, 0.0);
        cam.advance(1.0 / 64.0, &car, &mut world);
    }
    assert_eq!(cam.name(), "BumperCam2");
    let eye = cam.view(car.pos_330).eye;
    for (i, (e, p)) in eye.iter().zip(car.pos_330).enumerate() {
        assert!((e - p - drivercam.position[i]).abs() < 1e-5, "{eye:?}");
    }
    let view = cam.view(car.pos_330);
    assert!((view.near - 0.15).abs() < 1e-6);
    // drivercamrot pitches the view: forward is no longer the car's forward.
    assert!(view.matrix[2][1].abs() > 0.1, "{:?}", view.matrix);
}

/// Scenario "Cycling": four presses from TailCam0 give TailCam1, BumperCam, BumperCam2, TailCam0.
#[test]
fn cycling() {
    use nfs_assets::camera::{DriverCamera, PlayerCamera};
    let mut cam = PlayerCamera::new(DriverCamera::DEFAULT, 2);
    let mut world = flat_world();
    let car = car_at([0.0, 0.6, 0.0], 0.0, 0.0);
    cam.advance(0.1, &car, &mut world);
    assert_eq!(cam.name(), "TailCam0");
    let mut views = Vec::new();
    for _ in 0..4 {
        cam.cycle(&car, &mut world);
        cam.advance(0.1, &car, &mut world);
        views.push(cam.name());
    }
    assert_eq!(views, ["TailCam1", "BumperCam", "BumperCam2", "TailCam0"]);
}
