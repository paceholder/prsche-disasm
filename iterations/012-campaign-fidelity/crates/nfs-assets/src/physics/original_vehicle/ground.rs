//! V6: `Car_GroundContactTick` `0x49bd40` and the engine's ground queries.
//!
//! Source: the x87 instructions of `0x49bd40` (32 Hz queue, prio 0x1e,
//! after `Car_DynamicsTick`), with `Car_WheelGroundAndAttitude` `0x499a70`
//! ([`super::attitude`]), `Car_BuildGroundFrame` `0x49ba30`, the landing
//! compression clamp `0x49aec0`, the event queue `0x40f020` and
//! `Car_ClampAngularVelocityB` `0x49bba0`:
//! 1. Gate: `+0x7f`, `+0x7c` set, `+0x7d` clear, and the scheduler-rate
//!    query `0x414b10(1, 3, +0x78, car, …, +0x10b8)`. For the player car it
//!    gives 32 steps/s, 1 step and runs on every call (row 3 index 6 of
//!    `[0x5cc770]`, recorded by the V6 recorder).
//! 2. `0x499a70`: wheel contacts, suspension, pitch/roll, height `+0x424`.
//! 3. Ground rate `+0x420 = 32·Δ(mean contact y)` and its filter `+0x40c`,
//!    `+0x39c/+0x3a0/+0x3a4` = body axes · ground normal, the ground frame.
//! 4. Gravity `0.30625·+0x84` per step when `+0x428 > 0.005` (or light
//!    mode), speed `+0x35c = max + 0.25·min` of `|v.x|, |v.z|`.
//! 5. Contact, when upright (`+0x3a0 ≥ 0.939`) or above the ground
//!    (`+0x424 ≥ 0.001`): with a closing speed above 6 m/s and the body
//!    more than 0.05 m in the ground, a hard landing (event, body to −0.05,
//!    bounce `0.4·[0x5d0b98 + 8·surface]·closing` from the body surface
//!    `car+0x2c`, ω clamp); else below 0.005 m the soft contact (body onto
//!    the ground, vertical speed at least the ground's).
//! 6. Velocity direction `+0x348`.
//!
//! What is approximated (docs/approximations.md):
//! - The wheels query the original support owners `w+4` through
//!   [`RoadGround`] (`0x474060`, `0x474420`, `0x4745f0` on the retained
//!   tree); the legacy triangle grid of CPU fixtures is the only stand-in.
//!   The alternate plane `+0x3e8/+0x400` and the body surface `car+0x2c`
//!   are written by `Car_TrackLocate` ([`super::locate`]).
//! - The rollover contact `0x496610` (tilted car on the ground) is replaced
//!   by the soft contact (A27). The overhead query `0x473330` (roof over the
//!   car, flips an upward vertical speed) is taken as "none" (A28).
//! - The `+0x52c` bit 7 path `0x4998c0` and the flag-1 call `0x493dd0` on a
//!   hard landing are not ported (never set for the player).
//!
//! Verified against `scripts/research/replay-vehicle-ground.py` (tests
//! `tests/original_vehicle_ground.rs`).

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use glam::Vec3;

use super::attitude::{wheel_ground_and_attitude, WheelGround};
use super::car::OriginalCar;
use super::step::{
    build_ground_frame, c, dot, ground_speed, normalize, scale, update_world_points, x,
};
use crate::physics::original_adapter::{from_original, to_original};
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::surface::RoadSurface;

/// Support plane under one wheel's query point, in original world axes
/// (what the owner methods `0x474420`/`0x4745f0` return).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundHit {
    /// Plane normal.
    pub normal: [f32; 3],
    /// A point of the plane (the polygon's vertex 0).
    pub base: [f32; 3],
    /// Support polygon flags word, copied to `w+0x8c`.
    pub surface: u16,
}

impl GroundHit {
    /// The plane through `(point.x, height, point.z)`: its height at
    /// `point` is exactly `height`.
    pub fn at(point: [f32; 3], height: f32, normal: [f32; 3], surface: u16) -> Self {
        Self {
            normal,
            base: [point[0], height, point[2]],
            surface,
        }
    }
}

/// The engine's per-wheel support query (`0x474060` on the wheel's owner
/// `w+4`), in original world axes. `None`: no support polygon.
pub trait GroundQuery {
    fn wheel_ground(&mut self, wheel: usize, point: [f32; 3]) -> Option<GroundHit>;
}

/// A horizontal plane (tests).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlatGround {
    pub height: f32,
    pub surface: u16,
}

impl GroundQuery for FlatGround {
    fn wheel_ground(&mut self, _wheel: usize, point: [f32; 3]) -> Option<GroundHit> {
        Some(GroundHit::at(
            point,
            self.height,
            [0.0, 1.0, 0.0],
            self.surface,
        ))
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
        if let Some((tree, polygons)) = self.surface.original_arena() {
            // `0x499b64..0x499c3b`: 0x474060 (reuse), 0x474420 normal,
            // the flags word `+0x24` and, for the height, 0x4745f0(0).
            let owner = &mut self.owners[wheel];
            owner.locate(tree, polygons, point, true).ok()?;
            owner.polygon?;
            let normal = owner.normal(polygons);
            return Some(GroundHit {
                normal,
                base: owner.vertex(polygons, 0),
                surface: owner.flags_24,
            });
        }
        let scene = from_original(point);
        let hit = self.surface.query(scene.x, scene.z, scene.y, 2.0, 4.0)?;
        if !hit.height.is_finite() {
            return None;
        }
        Some(GroundHit::at(
            point,
            hit.height,
            to_original(Vec3::from(hit.normal)),
            hit.flags_word_a.unwrap_or(LEGACY_SURFACE),
        ))
    }
}

const K_GRAVITY_STEP: u32 = 0x3e9c_cccd; // 0x5b4a20, 0.30625 = 9.8 / 32
const K_UPRIGHT: u32 = 0x3f70_624e; // 0x5b4a0c, 0.939
const K_ATTITUDE: u32 = 0x3f34_fdf4; // 0x5b2698, 0.707
const K_SNAP: u32 = 0x3ba3_d70a; // 0x5b2738, 0.005
const K_TOUCH: u32 = 0x3a83_126f; // 0x5b27b4, 0.001
const K_LANDING: u32 = 0xbd4c_cccd; // 0x5b4a18, -0.05
const K_0_4: u32 = 0x3ecc_cccd; // 0x5b261c
const K_0_9: u32 = 0x3f66_6666; // 0x5b25e8
const K_0_86: u32 = 0x3f5c_28f6; // 0x5b49d8
const K_NEG_0_86: u32 = 0xbf5c_28f6; // 0x5b4a1c
const K_0_95: u32 = 0x3f73_3333; // 0x5b2610
const K_ANG_LIGHT: u32 = 0x4010_0000; // 0x5b49c0, 2.25
const K_ANG_SIMPLE: u32 = 0x40d8_0000; // 0x5b49bc, 6.75
/// `0x414b10` for the player car: steps per second and steps per call.
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

/// `0x40f020`: queue an effect event (up to 7, `+0x110c`).
pub fn push_event(car: &mut OriginalCar, event: [u32; 6]) {
    if car.events_1110.len() < 7 {
        car.events_1110.push(event);
    }
}

/// `0x49aec0`: after the body moved up by `dh`, the compressions `w+0x68`
/// lose `dh·U.y`, clamped to `(travel − ride)·(0.9 front | 0.75 rear)` and
/// `−travel`.
pub fn clamp_compressions(car: &mut OriginalCar, dh: f32) {
    if car.flags_52c & 0x80 != 0 {
        return;
    }
    let travel = car.specs.suspension_travel_124;
    let ride = car.specs.ride_height_120;
    let uy = car.axes_364[1][1];
    for (i, w) in car.wheels.iter_mut().enumerate() {
        let v = -(x(dh) * x(uy)) + x(w.compression_68);
        let v32 = v as f32;
        w.compression_68 = if v > 0.0 {
            let factor = if i < 2 { c(K_0_9) } else { 0.75 };
            let lim = (x(travel) - x(ride)) * factor;
            if x(v32) > lim {
                lim as f32
            } else {
                v32
            }
        } else if !(x(v32) > -x(travel)) {
            -travel
        } else {
            v32
        };
    }
}

/// `Car_ClampAngularVelocityB` `0x49bba0`: ω to ±0.75 turns/s (2.25 in
/// light mode, 6.75 with `+0x52c` bit 7 below `+0x354 = 0.75`).
pub fn clamp_angular_b(car: &mut OriginalCar, field_354: f32) {
    let lim = if car.res_518 != 0 {
        f32::from_bits(K_ANG_LIGHT)
    } else if car.flags_52c & 0x80 != 0 && field_354 < 0.75 {
        f32::from_bits(K_ANG_SIMPLE)
    } else {
        0.75
    };
    for v in &mut car.angular_388 {
        if !(lim >= *v) {
            *v = lim;
        } else if -lim > *v {
            *v = -lim;
        }
    }
}

/// The soft contact of `0x49bd40` (`0x49c28e..`): the body onto the
/// ground, the vertical speed at least the ground's.
fn soft_contact(car: &mut OriginalCar, h: f32, rise: f32, vy: f32) {
    car.position_330[1] = (x(car.position_330[1]) - x(h)) as f32;
    car.height_424 = 0.0;
    if car.flags_52d & 0x04 == 0 && car.ground_up_3a0 > f32::from_bits(K_0_86) {
        car.wheels_off_418 = 0;
    }
    car.velocity_33c[1] = if vy > 0.0 && rise > 0.0 {
        if rise > vy {
            rise
        } else {
            vy
        }
    } else {
        rise
    };
    clamp_compressions(car, -h);
}

/// `Car_GroundContactTick` `0x49bd40` (see the module docs). Returns the
/// wheel ground result, or `None` when the tick is gated off.
pub fn ground_contact_tick(
    car: &mut OriginalCar,
    query: &mut impl GroundQuery,
    lcg: &mut OriginalLcg,
) -> Option<WheelGround> {
    if car.active_7f == 0 || car.ground_on_7c == 0 || car.frozen_7d != 0 {
        return None;
    }
    let wg = wheel_ground_and_attitude(car, query, lcg);
    let normal = wg.normal;
    let h = car.height_424;
    let rise = ((x(wg.mean[1]) - x(car.ground_41c)) * STEPS_PER_SECOND) as f32;
    car.ground_41c = wg.mean[1];
    let [r, u, f] = car.axes_364;
    car.ground_right_39c = dot(r, normal);
    car.ground_up_3a0 = dot(u, normal);
    car.ground_fwd_3a4 = dot(f, normal);
    build_ground_frame(car, normal, car.ground_up_3a0);
    let gravity = if car.height_428 > f32::from_bits(K_SNAP) || car.res_518 != 0 {
        STEPS * c(K_GRAVITY_STEP) * x(car.field_84)
    } else {
        0.0
    };
    let vy = (x(car.velocity_33c[1]) - gravity) as f32;
    car.velocity_33c[1] = vy;
    car.speed_35c = ground_speed(car.velocity_33c);
    car.ground_ay_40c =
        (((x(rise) - x(car.ground_vy_420)) + x(car.ground_ay_40c) * 7.0) * 0.125) as f32;
    car.ground_vy_420 = rise;
    let closing = (x(rise) - x(vy)) as f32;
    let upright = car.ground_up_3a0 >= f32::from_bits(K_UPRIGHT) && car.flags_52c & 0x80 == 0;
    if upright || h >= f32::from_bits(K_TOUCH) {
        if closing > 6.0 {
            if h < f32::from_bits(K_LANDING) {
                push_event(car, [1, u32::MAX, 0, 5, closing.to_bits(), 0]);
                let dh_ext = c(K_LANDING) - x(h);
                let dh = dh_ext as f32;
                car.height_424 = f32::from_bits(K_LANDING);
                car.position_330[1] = (dh_ext + x(car.position_330[1])) as f32;
                clamp_compressions(car, dh);
                if car.flags_52d & 0x04 == 0 && car.ground_up_3a0 > f32::from_bits(K_0_9) {
                    car.wheels_off_418 = 0;
                }
                let s = (car.body_surface_2c & 0xf) as usize;
                car.landing_ad0 = closing;
                car.velocity_33c[1] = (c(BOUNCE[s]) * c(K_0_4) * x(closing) + x(rise)) as f32;
                clamp_angular_b(car, 0.0);
            }
        } else if h < f32::from_bits(K_SNAP) {
            soft_contact(car, h, rise, vy);
        }
    } else {
        // Rollover contact 0x496610 (not ported): the soft contact stands
        // in (approximation A27). The rest of the branch is original.
        if h < f32::from_bits(K_SNAP) {
            soft_contact(car, h, rise, vy);
        }
        if car.ground_up_3a0 < 0.0 {
            car.start_db0 = 0;
        }
        if car.ground_up_3a0 < f32::from_bits(K_ATTITUDE) && vy < -2.0 {
            let s = (car.body_surface_2c & 0xf) as usize;
            let v = &mut car.velocity_33c;
            v[1] = (x(v[1]) - c(BOUNCE[s]) * x(vy)) as f32;
            v[0] = (x(v[0]) * c(K_0_95)) as f32;
            v[2] = (x(v[2]) * c(K_0_95)) as f32;
            car.angular_388 = scale(car.angular_388, f32::from_bits(K_0_95));
        }
        if car.globals.flip_events_657a2c != 0
            && car.ground_up_3a0 < f32::from_bits(K_NEG_0_86)
            && car.flip_504 < 40.0
            && car.field_b50 == 0
        {
            push_event(car, [1, u32::MAX, 4, 3, 40.0f32.to_bits(), 0]);
            car.flip_504 = 40.0;
        }
    }
    car.ground_step_88 = car.globals.sched_step_606ac4;
    // 0x473330: no support over the car (approximation A28).
    let v = car.velocity_33c;
    car.vel_dir_348 = if v[0] != 0.0 || v[1] != 0.0 || v[2] != 0.0 {
        normalize(v)
    } else {
        car.axes_364[2]
    };
    Some(wg)
}

/// The engine side of the ground step: the original tick and the probe
/// placement (approximation A39). The alternate plane `+0x3e8/+0x400` and
/// the body surface `car+0x2c` come from `Car_TrackLocate`
/// ([`super::locate::track_locate`], prio 6).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroundAdapter {
    /// Result of the last tick.
    pub last: Option<WheelGround>,
}

impl GroundAdapter {
    /// One `Car_GroundContactTick`.
    pub fn ground_step(
        &mut self,
        car: &mut OriginalCar,
        query: &mut impl GroundQuery,
        lcg: &mut OriginalLcg,
    ) {
        self.last = ground_contact_tick(car, query, lcg);
    }

    /// Places the car on the ground at its current x, z (spawn, model
    /// switch): a trial tick on a copy measures the height, the body moves
    /// down (or up) by it, and a tick there with the ground taken as not
    /// moving. The velocity and angular velocity are kept.
    pub fn place(
        &mut self,
        car: &mut OriginalCar,
        query: &mut impl GroundQuery,
        lcg: &mut OriginalLcg,
    ) {
        let (v, w) = (car.velocity_33c, car.angular_388);
        update_world_points(car);
        let mut probe = car.clone();
        let mut probe_lcg = *lcg;
        if let Some(g) = ground_contact_tick(&mut probe, query, &mut probe_lcg) {
            if probe.height_424.is_finite() {
                car.position_330[1] = (x(car.position_330[1]) - x(probe.height_424)) as f32;
            }
            car.alt_normal_3e8 = g.normal;
            car.alt_base_400 = g.mean;
            car.ground_41c = g.mean[1];
            update_world_points(car);
        }
        self.ground_step(car, query, lcg);
        car.velocity_33c = v;
        car.angular_388 = w;
        car.pitch_filter_dc4 = 0.0;
        car.roll_filter_dc8 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::original_vehicle::sim::CarSpecs;

    fn car() -> OriginalCar {
        let mut specs = CarSpecs::from_bytes(&[0u8; 0x1c4]);
        specs.suspension_travel_124 = 0.12;
        specs.max_pitch_rate_134 = 1.0;
        specs.max_roll_rate_138 = 1.0;
        let mut car = OriginalCar::new(specs);
        car.position_330 = [0.0, 0.6, 0.0];
        car.body_points_8c = vec![
            [-0.75, -0.6, 1.2],
            [0.75, -0.6, 1.2],
            [-0.75, -0.6, -1.2],
            [0.75, -0.6, -1.2],
        ];
        car.field_84 = 1.16;
        car.globals.sched_step_606ac4 = 0x200;
        update_world_points(&mut car);
        car
    }

    #[test]
    fn car_settles_on_a_raised_plane() {
        let mut c = car();
        let mut g = GroundAdapter::default();
        let mut ground = FlatGround {
            height: 0.25,
            surface: 1,
        };
        g.place(&mut c, &mut ground, &mut OriginalLcg::default());
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
        update_world_points(&mut c);
        let mut g = GroundAdapter::default();
        let mut lcg = OriginalLcg::default();
        let mut ground = FlatGround {
            height: 0.0,
            surface: 1,
        };
        g.ground_step(&mut c, &mut ground, &mut lcg);
        assert!(c.wheels.iter().all(|w| w.airborne_84 == 1));
        assert!(c.height_424 > 1.3);
        let mut landed = false;
        for _ in 0..64 {
            g.ground_step(&mut c, &mut ground, &mut lcg);
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
}
