//! Shared setup of the V6 fixtures (`vehicle-ground-*.tsv`, recorded by
//! `scripts/research/replay-vehicle-ground.py`): the recorder's synthetic
//! ground, its car geometry, and the named field values it dumps.
#![allow(dead_code)]

use nfs_assets::physics::original_vehicle::{GroundHit, GroundQuery, OriginalCar};

use super::fixture::{Row, V};
use crate::local_fixtures::Fixture;

pub static RUNS: Fixture = Fixture::new("vehicle-ground-runs.tsv");
pub static BODY: Fixture = Fixture::new("vehicle-ground-body.tsv");
pub static DYN: Fixture = Fixture::new("vehicle-ground-dyn.tsv");
pub static CALL: Fixture = Fixture::new("vehicle-ground-call.tsv");

/// Body points of the recorder (0x49abc0 order FL, FR, RL, RR, then roof).
pub const BODY_POINTS: [[f32; 3]; 8] = [
    [-0.75, -0.35, 1.2],
    [0.75, -0.35, 1.2],
    [-0.75, -0.35, -1.2],
    [0.75, -0.35, -1.2],
    [-0.8, 0.75, 1.9],
    [0.8, 0.75, 1.9],
    [-0.8, 0.75, -1.9],
    [0.8, 0.75, -1.9],
];
pub const HALF_EXTENTS: [f32; 3] = [0.85, 0.65, 2.1];

#[derive(Clone, Copy, Debug)]
pub struct Plane {
    pub normal: [f32; 3],
    pub base: [f32; 3],
    pub flags: u16,
}

/// The recorder's `Ground`: regions (plane or hole, x0, x1, z0, z1), first
/// match wins, else the default plane.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub default: Option<Plane>,
    pub regions: Vec<(Option<Plane>, [f32; 4])>,
}

fn plane(text: &str) -> Option<Plane> {
    if text == "none" {
        return None;
    }
    let v: Vec<&str> = text.split(',').collect();
    let f = |i: usize| v[i].parse::<f32>().unwrap();
    Some(Plane {
        normal: [f(0), f(1), f(2)],
        base: [f(3), f(4), f(5)],
        flags: v[6].parse().unwrap(),
    })
}

impl Terrain {
    pub fn parse(text: &str) -> Self {
        let (default, regions) = text.split_once('|').unwrap();
        let regions = regions
            .split(';')
            .filter(|r| !r.is_empty())
            .map(|r| {
                let (p, b) = r.split_once('@').unwrap();
                let b: Vec<f32> = b.split(',').map(|v| v.parse().unwrap()).collect();
                (plane(p), [b[0], b[1], b[2], b[3]])
            })
            .collect();
        Self {
            default: plane(default),
            regions,
        }
    }

    pub fn pick(&self, x: f32, z: f32) -> Option<Plane> {
        let (x, z) = (x as f64, z as f64);
        for (p, b) in &self.regions {
            let [x0, x1, z0, z1] = b.map(|v| v as f64);
            if x0 <= x && x < x1 && z0 <= z && z < z1 {
                return *p;
            }
        }
        self.default
    }
}

impl GroundQuery for Terrain {
    fn wheel_ground(&mut self, _wheel: usize, p: [f32; 3]) -> Option<GroundHit> {
        self.pick(p[0], p[2]).map(|pl| GroundHit {
            normal: pl.normal,
            base: pl.base,
            surface: pl.flags,
        })
    }
}

/// The ground/attitude fields the recorder dumps (`CAR_FIELDS`,
/// `WHEEL_FIELDS`), by column name. `full` adds the event records and the
/// world points (single calls).
pub fn ground_values(car: &OriginalCar, full: bool) -> Vec<(String, V)> {
    let mut out: Vec<(String, V)> = Vec::new();
    let mut f = |name: &str, v: &[f32]| {
        if v.len() == 1 {
            out.push((name.to_string(), V::F(v[0].to_bits())));
        } else {
            for (k, x) in v.iter().enumerate() {
                out.push((format!("{name}_{k}"), V::F(x.to_bits())));
            }
        }
    };
    let axes: Vec<f32> = car.axes_364.iter().flatten().copied().collect();
    let frame: Vec<f32> = car.ground_490.iter().flatten().copied().collect();
    f("pos_330", &car.position_330);
    f("vel_33c", &car.velocity_33c);
    f("dir_348", &car.vel_dir_348);
    f("speed_35c", &[car.speed_35c]);
    f("axes_364", &axes);
    f("ang_388", &car.angular_388);
    f("side_39c", &[car.ground_right_39c]);
    f("up_3a0", &[car.ground_up_3a0]);
    f("fwd_3a4", &[car.ground_fwd_3a4]);
    f("ay_40c", &[car.ground_ay_40c]);
    f("mean_41c", &[car.ground_41c]);
    f("vy_420", &[car.ground_vy_420]);
    f("height_424", &[car.height_424]);
    f("height_428", &[car.height_428]);
    f("frame_490", &frame);
    f("wg_d64", &car.angular_ground_d64);
    f("pitch_dc4", &[car.pitch_filter_dc4]);
    f("roll_dc8", &[car.roll_filter_dc8]);
    f("landing_ad0", &[car.landing_ad0]);
    f("flip_504", &[car.flip_504]);
    if full {
        let world: Vec<f32> = car.world_points_1dc.iter().flatten().copied().collect();
        f("world_1dc", &world);
    }
    for (k, w) in car.wheels.iter().enumerate() {
        let p = format!("w{k}_");
        f(&format!("{p}query_34"), &w.query_34);
        f(&format!("{p}point_40"), &w.point_40);
        f(&format!("{p}normal_4c"), &w.normal_4c);
        f(&format!("{p}disp_58"), &w.disp_58);
        f(&format!("{p}gap_64"), &[w.gap_64]);
        f(&format!("{p}comp_68"), &[w.compression_68]);
        f(&format!("{p}load_6c"), &[w.load_6c]);
        f(&format!("{p}angle_70"), &[w.angle_70]);
        f(&format!("{p}noise_74"), &w.noise_74);
        f(&format!("{p}spin_88"), &[w.spin_88]);
    }
    let mut i = |name: String, v: i64| out.push((name, V::I(v)));
    i("flags_418".into(), car.wheels_off_418 as i64);
    i("sched_88".into(), car.ground_step_88 as i64);
    i("start_db0".into(), car.start_db0 as i64);
    i("events_110c".into(), car.events_1110.len() as i64);
    if full {
        for k in 0..42 {
            let v = car.events_1110.get(k / 6).map_or(0, |e| e[k % 6]);
            i(format!("event_1110_{k}"), v as i32 as i64);
        }
    }
    for (k, w) in car.wheels.iter().enumerate() {
        i(format!("w{k}_air_84"), w.airborne_84 as i64);
        i(format!("w{k}_surf_8c"), w.tyre.surface_8c as i64);
    }
    out
}

fn get3(r: &Row, name: &str) -> [f32; 3] {
    [0, 1, 2].map(|k| r.f(&format!("{name}_{k}")))
}

/// Sets every recorded ground field from columns `{prefix}{name}`.
pub fn set_ground(car: &mut OriginalCar, r: &Row, prefix: &str) {
    let g = |n: &str| r.f(&format!("{prefix}{n}"));
    let g3 = |n: &str| get3(r, &format!("{prefix}{n}"));
    let gi = |n: &str| r.i(&format!("{prefix}{n}"));
    car.position_330 = g3("pos_330");
    car.velocity_33c = g3("vel_33c");
    car.vel_dir_348 = g3("dir_348");
    car.speed_35c = g("speed_35c");
    for i in 0..3 {
        for j in 0..3 {
            car.axes_364[i][j] = g(&format!("axes_364_{}", 3 * i + j));
            car.ground_490[i][j] = g(&format!("frame_490_{}", 3 * i + j));
        }
    }
    car.angular_388 = g3("ang_388");
    car.ground_right_39c = g("side_39c");
    car.ground_up_3a0 = g("up_3a0");
    car.ground_fwd_3a4 = g("fwd_3a4");
    car.ground_ay_40c = g("ay_40c");
    car.wheels_off_418 = gi("flags_418") as u16;
    car.ground_41c = g("mean_41c");
    car.ground_vy_420 = g("vy_420");
    car.height_424 = g("height_424");
    car.height_428 = g("height_428");
    car.angular_ground_d64 = g3("wg_d64");
    car.pitch_filter_dc4 = g("pitch_dc4");
    car.roll_filter_dc8 = g("roll_dc8");
    car.ground_step_88 = gi("sched_88") as i32;
    car.landing_ad0 = g("landing_ad0");
    car.start_db0 = gi("start_db0") as i32;
    car.flip_504 = g("flip_504");
    let n = gi("events_110c") as usize;
    car.events_1110 = (0..n)
        .map(|e| [0, 1, 2, 3, 4, 5].map(|k| gi(&format!("event_1110_{}", 6 * e + k)) as i32 as u32))
        .collect();
    car.world_points_1dc = (0..8)
        .map(|p| [0, 1, 2].map(|k| g(&format!("world_1dc_{}", 3 * p + k))))
        .collect();
    for (k, w) in car.wheels.iter_mut().enumerate() {
        let p = format!("w{k}_");
        w.query_34 = g3(&format!("{p}query_34"));
        w.point_40 = g3(&format!("{p}point_40"));
        w.normal_4c = g3(&format!("{p}normal_4c"));
        w.disp_58 = g3(&format!("{p}disp_58"));
        w.gap_64 = g(&format!("{p}gap_64"));
        w.compression_68 = g(&format!("{p}comp_68"));
        w.load_6c = g(&format!("{p}load_6c"));
        w.angle_70 = g(&format!("{p}angle_70"));
        w.noise_74 = g3(&format!("{p}noise_74"));
        w.airborne_84 = gi(&format!("{p}air_84")) as i32;
        w.spin_88 = g(&format!("{p}spin_88"));
        w.tyre.surface_8c = gi(&format!("{p}surf_8c")) as i32;
    }
}

/// Sets the tick's other inputs (`INPUT_FIELDS`) from `{prefix}{name}`.
pub fn set_inputs(car: &mut OriginalCar, r: &Row, prefix: &str) {
    let g = |n: &str| r.f(&format!("{prefix}{n}"));
    let g3 = |n: &str| get3(r, &format!("{prefix}{n}"));
    let gi = |n: &str| r.i(&format!("{prefix}{n}"));
    car.accel_d40 = g3("acc_d40");
    car.accel_d4c = g3("sums_d4c");
    car.gas_eff_d7f = gi("gas_d7f") as u8;
    car.field_84 = g("field_84");
    let flags = gi("flags_52c") as u32;
    car.flags_52c = flags as u8;
    car.flags_52d = (flags >> 8) as u8;
    car.half_extents_3a8 = g3("half_3a8");
    car.alt_normal_3e8 = g3("alt_n_3e8");
    car.alt_base_400 = g3("alt_b_400");
    car.body_surface_2c = gi("body_surf_2c") as u16;
    car.remote_dac = gi("remote_dac") as i32;
}
