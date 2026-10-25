//! Ground-contact adapter: `Car_GroundContactTick` `0x49bd40` until V6.
//!
//! The planar model (V5) reads its ground from fields the ground step writes:
//! `+0x3a0` (ground normal · body up), the ground frame `+0x490..` built by
//! `Car_BuildGroundFrame` `0x49ba30`, the height `+0x424`, and per wheel the
//! airborne flag `w+0x84` and the surface `w+0x8c`. The ground step also keeps
//! the car on the road (vertical position and velocity).
//!
//! What is the original here (ported from the pseudo-C, not yet replayed):
//! - The order and the formulas of `0x49bd40` around its call to
//!   `Car_WheelGroundAndAttitude` `0x499a70`: `+0x39c/+0x3a0/+0x3a4` as dots of
//!   the body axes with the ground normal, the ground frame, the slope
//!   gravity `0.30625·+0x84` per step when `+0x428 > 0.005`, the ground
//!   vertical speed from the change of the mean contact height, the
//!   speed `+0x35c`, the soft contact (height below 0.005: snap onto the
//!   road, take the road's vertical speed) and the hard landing (bounce from
//!   `[0x5d0b98]`).
//! - From `0x499a70`: the query point `+0x1dc[i] + sim+0x120·U.y`, the mean
//!   contact point, the airborne test `contact.y − query.y < −0.20416667`
//!   (`w+0x84`), the ground normal from the contact diagonals, and the
//!   surface copy with the handling-mode remap (1, 10 → 2).
//! - `0x49a900` (height `+0x424`/`+0x428` from the first four world points).
//!
//! What is approximated (docs/approximations.md, ground-contact adapter):
//! - The per-wheel ground comes from the engine's support query
//!   ([`GroundQuery`]: the original support tree of upstream's loader, or the
//!   legacy triangle grid), without the material height noise, the front
//!   wobble or the alternate plane `+0x3e8/+0x400` (`0x499a70`). A miss or a
//!   plane more than 4 m from `+0x334` reuses the wheel's last plane.
//! - Attitude: instead of the kinematic pitch/roll model (V6, `0x499a70`
//!   tail), on the ground the body up axis is set to the ground normal and
//!   the pitch/roll part of the angular velocity is dropped.
//! - `0x414b10`'s outputs are taken as 32 steps/s and one step; the rollover
//!   contact `0x496610` (tilted car on the ground) is replaced by the soft
//!   contact; the bounce surface `car+0x2c` is wheel 0's surface; the landing
//!   effects, the ω clamp after a landing, the wheel compressions `0x49aec0`
//!   and the track query `0x473330` are not run.

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use glam::Vec3;

use super::car::OriginalCar;
use super::step::{
    abs87, build_ground_frame, c, cross, dot, dot_ext, ground_speed, normalize,
    update_world_points, x,
};
use crate::physics::original_adapter::{from_original, to_original};
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::surface::RoadSurface;

/// Ground under one wheel's query point, in original world axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHit {
    /// Support plane height at the query point's x, z.
    pub height: f32,
    /// Plane normal.
    pub normal: [f32; 3],
    /// Support polygon flags word, copied to `w+0x8c`.
    pub surface: u16,
}

/// The engine's per-wheel support query, in original world axes.
pub trait GroundQuery {
    fn wheel_ground(&mut self, wheel: usize, point: [f32; 3]) -> Option<GroundHit>;
}

/// A horizontal plane (tests and the V5 setup).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatGround {
    pub height: f32,
    pub surface: u16,
}

impl GroundQuery for FlatGround {
    fn wheel_ground(&mut self, _wheel: usize, _point: [f32; 3]) -> Option<GroundHit> {
        Some(GroundHit {
            height: self.height,
            normal: [0.0, 1.0, 0.0],
            surface: self.surface,
        })
    }
}

/// Surface word for wheels on the legacy triangle grid (no material data):
/// 1, as the V5 recordings use.
pub const LEGACY_SURFACE: u16 = 1;

/// The road surface of the racing scene. With the original support backend
/// each wheel keeps its own support owner (polygon cache), as `0x499a70`
/// does; the legacy grid takes the support nearest to the query height.
pub struct RoadGround<'a> {
    pub surface: &'a RoadSurface,
    pub owners: &'a mut [OriginalSupportOwner; 4],
}

impl GroundQuery for RoadGround<'_> {
    fn wheel_ground(&mut self, wheel: usize, point: [f32; 3]) -> Option<GroundHit> {
        let scene = from_original(point);
        let hit = if self.surface.uses_original_support() {
            self.surface
                .original_support(scene.to_array(), &mut self.owners[wheel])?
        } else {
            self.surface.query(scene.x, scene.z, scene.y, 2.0, 4.0)?
        };
        Some(GroundHit {
            height: hit.height,
            normal: to_original(Vec3::from(hit.normal)),
            surface: hit.flags_word_a.unwrap_or(LEGACY_SURFACE),
        })
    }
}

const K_AIRBORNE: u32 = 0xbe51_1111; // -0.20416667
const K_GRAVITY_STEP: u32 = 0x3e9c_cccd; // 0.30625 = 9.8 / 32
const K_UPRIGHT: u32 = 0x3f70_624e; // 0.939
const K_ATTITUDE: u32 = 0x3f34_fdf4; // 0.707
const K_SNAP: u32 = 0x3ba3_d70a; // 0.005
const K_LANDING: u32 = 0xbd4c_cccd; // -0.05
const K_0_4: u32 = 0x3ecc_cccd;
/// `0x414b10` outputs: steps per second and steps since the last call.
const STEPS_PER_SECOND: f64 = 32.0;
const STEPS: f64 = 1.0;
/// Bounce per surface `[0x5d0b98 + 8·s]` (`nfs5.exe` .data).
const BOUNCE: [u32; 16] = [
    0x3f00_0000,
    0x3f33_3333,
    0x3f00_0000,
    0x3f00_0000,
    0x3f33_3333,
    0x3f00_0000,
    0x3f00_0000,
    0x3ecc_cccd,
    0x3f33_3333,
    0x3f00_0000,
    0x3f19_999a,
    0x3f00_0000,
    0x3f80_0000,
    0x3f80_0000,
    0x3f80_0000,
    0x3f80_0000,
];
/// A support plane more than this far from `+0x334` is not taken (as the
/// original's switch to the alternate plane, `0x499a70`).
const MAX_PLANE_DISTANCE: f64 = 4.0;

/// A plane through `base` with `normal`, evaluated at the query's x, z
/// (`0x499a70`).
fn plane(query: [f32; 3], normal: [f32; 3], base: [f32; 3]) -> f32 {
    ((-((x(query[0]) - x(base[0])) * x(normal[0])) - (x(query[2]) - x(base[2])) * x(normal[2]))
        / x(normal[1])
        + x(base[1])) as f32
}

/// Ground normal from the four contact points (`0x499a70`): the front−rear
/// and left−right diagonals, normalised, crossed, normalised, pointing up;
/// `(0, 1, 0)` if a diagonal is zero.
pub fn contact_normal(p: &[[f32; 3]; 4]) -> [f32; 3] {
    let along: [f32; 3] =
        [0, 1, 2].map(|k| (((x(p[1][k]) + x(p[0][k])) - (x(p[3][k]) + x(p[2][k]))) * 0.5) as f32);
    let across: [f32; 3] =
        [0, 1, 2].map(|k| (((x(p[3][k]) + x(p[1][k])) - (x(p[2][k]) + x(p[0][k]))) * 0.5) as f32);
    if across == [0.0; 3] || along == [0.0; 3] {
        return [0.0, 1.0, 0.0];
    }
    let n = normalize(cross(normalize(along), normalize(across)));
    if n[1] < 0.0 {
        n.map(|v| -v)
    } else {
        n
    }
}

/// `0x49a900`: height of the car above the ground plane through `mean` with
/// `normal`. Upright (`+0x3a0 > 0.939`, not flag 0x80): the mean of the four
/// lowest of the first four world points' plane distances with the lowest,
/// halved, into `+0x424` and `+0x428`. Otherwise the lowest of all world
/// points into `+0x424` and the mean of the three lowest into `+0x428`.
/// Returns `(+0x424, +0x428)`.
pub fn ground_height(car: &OriginalCar, normal: [f32; 3], mean: [f32; 3]) -> (f32, f32) {
    let mut low = [1000.0f32; 4];
    let mut insert = |d: f64| {
        if !(x(low[0]) < d) {
            low = [d as f32, low[0], low[1], low[2]];
        } else if !(x(low[1]) < d) {
            low = [low[0], d as f32, low[1], low[2]];
        } else if !(x(low[2]) < d) {
            low[3] = low[2];
            low[2] = d as f32;
        } else if d <= x(low[3]) {
            low[3] = d as f32;
        }
    };
    let rel = |p: [f32; 3]| [0, 1, 2].map(|k| (x(p[k]) - x(mean[k])) as f32);
    let upright = car.ground_up_3a0 > f32::from_bits(K_UPRIGHT) && car.flags_52c & 0x80 == 0;
    let points = &car.world_points_1dc;
    if upright {
        for &p in points.iter().take(4) {
            insert(dot_ext(normal, rel(p)));
        }
        let h = (((x(low[3]) + x(low[2]) + x(low[1]) + x(low[0])) * 0.25 + x(low[0])) * 0.5) as f32;
        return (h, h);
    }
    for &p in points {
        insert(dot_ext(normal, rel(p)));
    }
    let mean3 = ((x(low[2]) + x(low[1]) + x(low[0])) * f32::from_bits(0x3eaa_aaab) as f64) as f32;
    (low[0], mean3)
}

/// State the ground step keeps between calls (fields of the original car
/// struct that the V5 kernels don't read, and the adapter's own planes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroundAdapter {
    /// Last accepted support plane per wheel (point, normal): stand-in for
    /// the alternate plane `+0x3e8/+0x400`.
    pub planes: [Option<([f32; 3], [f32; 3])>; 4],
    /// `+0x41c`: mean contact height of the last step (`None` before the
    /// first step: the road is taken as not moving).
    pub ground_41c: Option<f32>,
    /// `+0x420`: vertical speed of the ground under the car.
    pub ground_vy_420: f32,
    /// `+0x40c`: filtered change of `+0x420`.
    pub ground_ay_40c: f32,
    /// `+0x428`: second height measure of `0x49a900` (gates the gravity).
    pub height_428: f32,
    /// `+0x39c`/`+0x3a4`: body right / forward · ground normal.
    pub right_39c: f32,
    pub forward_3a4: f32,
    /// Wheels off the ground in the last step (`0x499a70` `local_c8`; 4
    /// when the ground normal is steeper than 60°).
    pub wheels_off: u32,
    /// Ground normal of the last step.
    pub normal: [f32; 3],
}

impl GroundAdapter {
    /// The `0x499a70` part: per-wheel ground, contact points `w+0x40`,
    /// surfaces `w+0x8c`, airborne flags `w+0x84`, the ground normal, the
    /// attitude stand-in, the world points and the height `+0x424`. Returns
    /// the mean contact height.
    fn wheels(&mut self, car: &mut OriginalCar, query: &mut impl GroundQuery) -> f32 {
        let ride = x(car.specs.ride_height_120);
        let up_y = x(car.axes_364[1][1]);
        let pos = car.position_330;
        let handling = car.globals.handling_mode_6573f8 != 0;
        let mut queries = [[0.0f32; 3]; 4];
        let mut contacts = [[0.0f32; 3]; 4];
        let mut sum = [0.0f32; 3];
        for i in 0..4 {
            let mut q = car.world_points_1dc.get(i).copied().unwrap_or(pos);
            q[1] = (ride * up_y + x(q[1])) as f32;
            let hit = query.wheel_ground(i, q).filter(|h| {
                h.height.is_finite()
                    && abs87(x(h.height) - x(pos[1])) <= MAX_PLANE_DISTANCE
                    && h.normal[1] > 0.0
            });
            let height = match hit {
                Some(h) => {
                    self.planes[i] = Some(([q[0], h.height, q[2]], h.normal));
                    let w = &mut car.wheels[i].tyre;
                    w.surface_8c = if handling && matches!(h.surface & 15, 1 | 10) {
                        2
                    } else {
                        h.surface as i32
                    };
                    h.height
                }
                None => match self.planes[i] {
                    Some((base, normal)) => plane(q, normal, base),
                    None => q[1],
                },
            };
            let contact = [q[0], height, q[2]];
            for k in 0..3 {
                sum[k] = (x(sum[k]) + x(contact[k])) as f32;
            }
            car.wheels[i].point_40 = contact;
            queries[i] = q;
            contacts[i] = contact;
        }
        let mean = sum.map(|s| (x(s) * 0.25) as f32);
        let mut off = 0;
        for i in 0..4 {
            let d = (x(contacts[i][1]) - x(queries[i][1])) as f32;
            let airborne = !(x(f32::from_bits(K_AIRBORNE)) <= x(d));
            car.wheels[i].airborne_84 = airborne as i32;
            off += airborne as u32;
        }
        let normal = contact_normal(&contacts);
        if normal[1] < 0.5 {
            off = 4;
        }
        self.wheels_off = off;
        self.normal = normal;
        if off < 4 {
            align_to_ground(car, normal);
        }
        update_world_points(car);
        let (h424, h428) = ground_height(car, normal, mean);
        car.height_424 = h424;
        self.height_428 = h428;
        mean[1]
    }

    /// Stand-in for `Car_GroundContactTick` `0x49bd40` (see the module docs).
    pub fn ground_step(&mut self, car: &mut OriginalCar, query: &mut impl GroundQuery) {
        if car.active_7f == 0 || car.frozen_7d != 0 {
            return;
        }
        let ground = self.wheels(car, query);
        let normal = self.normal;
        let height = car.height_424;
        let prev = self.ground_41c.unwrap_or(ground);
        let rise = ((x(ground) - x(prev)) * STEPS_PER_SECOND) as f32;
        self.ground_41c = Some(ground);
        let [r, u, f] = car.axes_364;
        self.right_39c = dot(r, normal);
        car.ground_up_3a0 = dot(u, normal);
        self.forward_3a4 = dot(f, normal);
        build_ground_frame(car, normal, car.ground_up_3a0);
        let gravity = if self.height_428 > f32::from_bits(K_SNAP) || car.res_518 != 0 {
            (STEPS * c(K_GRAVITY_STEP) * x(car.field_84)) as f32
        } else {
            0.0
        };
        let vy = (x(car.velocity_33c[1]) - x(gravity)) as f32;
        car.velocity_33c[1] = vy;
        car.speed_35c = ground_speed(car.velocity_33c);
        let prev_rise = self.ground_vy_420;
        self.ground_vy_420 = rise;
        self.ground_ay_40c =
            ((x(self.ground_ay_40c) * 7.0 + (x(rise) - x(prev_rise))) * 0.125) as f32;
        let closing = (x(rise) - x(vy)) as f32;
        let contact = (car.ground_up_3a0 >= f32::from_bits(K_UPRIGHT) && car.flags_52c & 0x80 == 0)
            || height >= 0.001;
        // Not upright and touching: the original's rollover contact
        // `0x496610` is not ported; the soft contact stands in.
        if !(closing <= 6.0) && contact {
            if height < f32::from_bits(K_LANDING) {
                let landing = f32::from_bits(K_LANDING);
                car.height_424 = landing;
                car.position_330[1] = ((x(landing) - x(height)) + x(car.position_330[1])) as f32;
                let s = (car.wheels[0].tyre.surface_8c & 0xf) as usize;
                car.velocity_33c[1] = (c(BOUNCE[s]) * c(K_0_4) * x(closing) + x(rise)) as f32;
            }
        } else if height < f32::from_bits(K_SNAP) {
            car.height_424 = 0.0;
            car.position_330[1] = (x(car.position_330[1]) - x(height)) as f32;
            car.velocity_33c[1] = if vy <= 0.0 || rise <= 0.0 || vy < rise {
                rise
            } else {
                vy
            };
        }
    }

    /// Places the car on the ground at its current x, z (spawn, model
    /// switch): a ground step with the road taken as not moving, the body
    /// moved down onto the road if it is above it, and a second step there.
    /// The vertical velocity is kept.
    pub fn place(&mut self, car: &mut OriginalCar, query: &mut impl GroundQuery) {
        let vy = car.velocity_33c[1];
        update_world_points(car);
        self.ground_41c = None;
        self.ground_step(car, query);
        if car.height_424 > 0.0 {
            car.position_330[1] = (x(car.position_330[1]) - x(car.height_424)) as f32;
            update_world_points(car);
            self.ground_41c = None;
            self.ground_step(car, query);
        }
        car.velocity_33c[1] = vy;
    }
}

/// Attitude stand-in until V6 (approximation): the body up axis becomes the
/// ground normal (heading kept: forward projected onto the ground plane), and
/// the angular velocity keeps only its part about the normal. Skipped when
/// the car is already aligned and has no pitch/roll rate, so a flat run is
/// untouched; and for a car tilted more than 45° against the ground.
fn align_to_ground(car: &mut OriginalCar, normal: [f32; 3]) {
    let [_, u, f] = car.axes_364;
    let un = dot_ext(u, normal);
    if !(un >= c(K_ATTITUDE)) {
        return;
    }
    let w = car.angular_388;
    let yaw = dot_ext(w, normal);
    let tilt = [0, 1, 2].map(|k| x(w[k]) - yaw * x(normal[k]));
    let aligned = un >= 1.0 - 1e-6;
    if aligned && tilt == [0.0; 3] {
        return;
    }
    if !aligned {
        let fd = dot_ext(f, normal);
        let fp = [0, 1, 2].map(|k| (x(f[k]) - fd * x(normal[k])) as f32);
        let nf = normalize(fp);
        let nr = normalize(cross(normal, nf));
        car.axes_364 = [nr, normal, nf];
    }
    car.angular_388 = normal.map(|n| (yaw * x(n)) as f32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::original_vehicle::sim::CarSpecs;

    fn car() -> OriginalCar {
        let mut car = OriginalCar::new(CarSpecs::from_bytes(&[0u8; 0x1c4]));
        car.position_330 = [0.0, 0.6, 0.0];
        car.body_points_8c = vec![
            [0.75, -0.6, 1.2],
            [-0.75, -0.6, 1.2],
            [0.75, -0.6, -1.2],
            [-0.75, -0.6, -1.2],
        ];
        update_world_points(&mut car);
        car
    }

    #[test]
    fn flat_normal_is_up() {
        let p = [
            [0.75, 0.0, 1.2],
            [-0.75, 0.0, 1.2],
            [0.75, 0.0, -1.2],
            [-0.75, 0.0, -1.2],
        ];
        let n = contact_normal(&p);
        assert_eq!(n.map(|v| v.abs()), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn car_settles_on_a_raised_plane() {
        let mut c = car();
        let mut g = GroundAdapter::default();
        let mut ground = FlatGround {
            height: 0.25,
            surface: 1,
        };
        g.place(&mut c, &mut ground);
        assert!(
            (c.position_330[1] - 0.85).abs() < 1e-6,
            "{:?}",
            c.position_330
        );
        assert_eq!(c.height_424, 0.0);
        assert!(c.wheels.iter().all(|w| w.airborne_84 == 0));
        assert_eq!(c.ground_up_3a0, 1.0);
    }

    #[test]
    fn car_above_the_road_falls_and_lands() {
        let mut c = car();
        c.position_330[1] = 2.0;
        c.field_84 = 1.16;
        update_world_points(&mut c);
        let mut g = GroundAdapter::default();
        let mut ground = FlatGround {
            height: 0.0,
            surface: 1,
        };
        g.ground_step(&mut c, &mut ground);
        assert!(c.wheels.iter().all(|w| w.airborne_84 == 1));
        assert!(c.height_424 > 1.3);
        let mut landed = false;
        for _ in 0..64 {
            g.ground_step(&mut c, &mut ground);
            for _ in 0..2 {
                c.position_330[1] += c.velocity_33c[1] / 64.0;
                update_world_points(&mut c);
            }
            if c.height_424 <= 0.0 && c.wheels.iter().all(|w| w.airborne_84 == 0) {
                landed = true;
                break;
            }
        }
        assert!(landed, "{:?} h {}", c.position_330, c.height_424);
        assert!(c.position_330[1] >= 0.6 - 0.06);
    }

    #[test]
    fn body_up_follows_a_slope() {
        let mut c = car();
        // 10 % uphill along +z.
        struct Slope;
        impl GroundQuery for Slope {
            fn wheel_ground(&mut self, _: usize, p: [f32; 3]) -> Option<GroundHit> {
                let n = Vec3::new(0.0, 1.0, -0.1).normalize();
                Some(GroundHit {
                    height: 0.1 * p[2],
                    normal: n.to_array(),
                    surface: 1,
                })
            }
        }
        let mut g = GroundAdapter::default();
        g.place(&mut c, &mut Slope);
        let u = Vec3::from(c.axes_364[1]);
        let n = Vec3::new(0.0, 1.0, -0.1).normalize();
        assert!(u.dot(n) > 0.9999, "{u:?}");
        assert!(Vec3::from(c.axes_364[2]).dot(n).abs() < 1e-5);
        assert!(c.ground_up_3a0 > 0.9999);
    }
}
