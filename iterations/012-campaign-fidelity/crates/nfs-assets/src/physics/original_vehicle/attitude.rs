//! V6: wheel ground contact, suspension and the kinematic pitch/roll model.
//!
//! Source: the x87 instructions of `Car_WheelGroundAndAttitude` `0x499a70`
//! (both wheel loops and the tail), with its callees `0x494480` (support
//! plane distance test), `0x499730` (material height noise),
//! `Car_ClampLocalAngular` `0x49bc80`, `Car_UpdateWorldPoints` `0x49ae30`
//! and `0x49a900` (height above the ground). `Car_GroundContactTick`
//! `0x49bd40` calls it on the 32 Hz queue (`super::ground`).
//!
//! Per wheel `i` (body point `+0x1dc[i]`, wheel order FL, FR, RL, RR):
//! 1. Query point `q = p + (0, sim+0x120·U.y, 0)`, support plane under it
//!    (the engine's [`GroundQuery`]). The contact height is the plane at
//!    `q.x, q.z` plus the material noise; without a plane, with a zero
//!    surface nibble or a plane more than 4 m from `+0x334`, the alternate
//!    plane `+0x3e8/+0x400` instead.
//! 2. Compression `w+0x68` along U, clamped to the travel, and the arm angle
//!    `w+0x70 = ±asin(0.1·(ride + compression))`.
//! 3. Gap `contact.y − q.y`: below −0.20416667 the wheel is off the ground
//!    (`w+0x84`). The displacement `w+0x58..` is the gap along `(R.y, U.y,
//!    F.y)` times 1.25 when the compression exceeds twice the travel; on
//!    the ground and moving its `.y` gets a slide term along the contact
//!    plane.
//! 4. Ground normal from the contact diagonals (pointing up); a normal with
//!    y < 0.5 counts as four wheels off.
//!
//! Then, with `+0x3a0 ≥ 0.707` and fewer than four wheels off, the
//! attitude: pitch and roll rates in the ground-free body frame
//! (`d64 = ω·R`, `d6c = ω·F`) follow
//! `d64' = sim+0x114·0.925·d64 + clamp((rear − front)/8 − dc4, ±sim+0x134)`,
//! `d6c' = sim+0x118·d6c + clamp((right − left)/8 + (1 − gas/1275)·dc8, ±sim+0x138)`
//! with `dc4 = 0.8·dc4 + sim+0x128·d54·(0.125 accel | 0.175 brake)` and
//! `dc8 = 0.8·dc8 + 0.25·sim+0x12c·d4c`, clamped to ±0.75 turns/s and turned
//! back into the world `ω`. With wheels off on one side only the body is
//! moved by `2·half extent·(sin, cos)(roll)/32` (the side lift). Below 0.707
//! the filters halve.
//!
//! Verified against `scripts/research/replay-vehicle-ground.py` (tests
//! `tests/original_vehicle_ground.rs`).

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::car::OriginalCar;
use super::ground::GroundQuery;
use super::step::{abs87, c, cross, dot, dot_ext, normalize, scale, update_world_points, x};
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_wheel_state::{
    advance_material_height_unrounded, MaterialHeightState, MaterialRandomState,
};

const K_0_1: u32 = 0x3dcc_cccd; // 0x5b2604, 0x5b49b4
const K_0_3: u32 = 0x3e99_999a; // 0x5b25f0
const K_NEG_0_1: u32 = 0xbdcc_cccd; // 0x5b2444
const K_0_9: u32 = 0x3f66_6666; // 0x5b25e8
const K_NEG_1_65: u32 = 0xbfd3_3333; // 0x5b49fc
const K_NEG_1_6: u32 = 0xbfcc_cccd; // 0x5b49f8
const K_NEG_0_30625: u32 = 0xbe9c_cccd; // 0x5b49f4
const K_AIRBORNE: u32 = 0xbe51_1111; // 0x5b49f0, -0.20416667
const K_0_8: u32 = 0x3f4c_cccd; // 0x5b3f00, 0x5b24e8
const K_0_01: u32 = 0x3c23_d70a; // 0x5b27a8
const K_1_15: u32 = 0x3d88_8889; // 0x5b49ec, 0.06666667
const K_ATTITUDE: u32 = 0x3f34_fdf4; // 0x5b2698, 0.707
const K_0_175: u32 = 0x3e33_3333; // 0x5b49e4
const K_0_15: u32 = 0x3e19_999a; // 0x5b2638
const K_1_1275: u32 = 0x3a4d_9a67; // 0x5b49e0, 0.0007843137
const K_0_925: u32 = 0x3f6c_cccd; // 0x5b49dc
const K_UPRIGHT: u32 = 0x3f70_624e; // 0x5b4a0c, 0.939
const K_THIRD: u32 = 0x3eaa_aaab; // 0x5b4a10, 0.33333334
/// Front wobble: `6.28318` (`0x5b4a08`) and `1/1500` (f64 `0x5b4a00`).
const K_WOBBLE_TURN: u32 = 0x40c9_0fd0;
const K_WOBBLE: u64 = 0x3f45_d867_c3ec_e2a5;
/// `SinTurns`/`CosTurns` multiplier `0x5bb2c0` (f64, 2π rounded to f32).
const TWO_PI: u64 = 0x4019_21fb_2000_0000;

/// Result of [`wheel_ground_and_attitude`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelGround {
    /// Mean contact point of the four wheels (`0x499a70` returns its y).
    pub mean: [f32; 3],
    /// Ground normal (pointing up).
    pub normal: [f32; 3],
    /// Wheels off the ground (4 also when the normal is steeper than 60°).
    pub wheels_off: u32,
    /// Wheels whose height came from their support plane (not from the
    /// alternate plane).
    pub on_support: [bool; 4],
}

/// Plane through `base` with `normal`, at `q.x, q.z` (`0x499c40`, `0x494480`).
fn plane_ext(q: [f32; 3], normal: [f32; 3], base: [f32; 3]) -> f64 {
    ((-((x(q[0]) - x(base[0])) * x(normal[0]))) - (x(q[2]) - x(base[2])) * x(normal[2]))
        / x(normal[1])
        + x(base[1])
}

/// `0x499730`: material height noise of wheel `i` (draws from the LCG).
fn material_noise(car: &mut OriginalCar, i: usize, lcg: &mut OriginalLcg) -> f64 {
    let w = &mut car.wheels[i];
    let mut state = MaterialHeightState {
        previous: w.noise_74[0],
        current: w.noise_74[1],
        phase: w.noise_74[2],
    };
    let mut rng = MaterialRandomState {
        seed_5d1028: lcg.state_5d1028,
        multiplier_5d102c: lcg.multiplier_5d102c,
        product_655a0c: lcg.product_655a0c,
    };
    let v = advance_material_height_unrounded(
        &mut state,
        car.ground_up_3a0,
        car.speed_35c,
        w.tyre.surface_8c as u32,
        &mut rng,
    );
    w.noise_74 = [state.previous, state.current, state.phase];
    lcg.state_5d1028 = rng.seed_5d1028;
    lcg.product_655a0c = rng.product_655a0c;
    v
}

/// Ground normal from the four contact points: `(normal, raw y)`, where the
/// raw y is the cross product's before it is turned upwards (`0x49a201..`).
fn contact_normal(p: &[[f32; 3]; 4]) -> ([f32; 3], Option<f32>) {
    let along: [f32; 3] =
        [0, 1, 2].map(|k| (((x(p[1][k]) + x(p[0][k])) - (x(p[3][k]) + x(p[2][k]))) * 0.5) as f32);
    let across: [f32; 3] =
        [0, 1, 2].map(|k| (((x(p[3][k]) + x(p[1][k])) - (x(p[2][k]) + x(p[0][k]))) * 0.5) as f32);
    if across.iter().all(|&v| v == 0.0) || along.iter().all(|&v| v == 0.0) {
        return ([0.0, 1.0, 0.0], None);
    }
    let n = normalize(cross(normalize(along), normalize(across)));
    let out = if n[1] < 0.0 { n.map(|v| -v) } else { n };
    (out, Some(n[1]))
}

/// `Car_ClampLocalAngular` `0x49bc80`: `d64..d6c` to ±0.75 turns/s (an
/// unordered value becomes −0.75, as the x87 branches do).
#[allow(clippy::manual_clamp)]
pub fn clamp_local_angular(car: &mut OriginalCar) {
    for v in &mut car.angular_ground_d64 {
        if *v > 0.75 {
            *v = 0.75;
        } else if !(*v >= -0.75) {
            *v = -0.75;
        }
    }
}

/// `0x49a900`: height of the car above the ground plane through `mean`
/// with `normal`. Upright (`+0x3a0 > 0.939`, not flag 0x80): the mean of
/// the four lowest plane distances of the first four world points with the
/// lowest, halved, into `+0x424` and `+0x428`. Otherwise the lowest of all
/// world points into `+0x424` and the mean of the three lowest into
/// `+0x428`.
pub fn ground_height(car: &mut OriginalCar, normal: [f32; 3], mean: [f32; 3]) {
    let mut low = [1000.0f32; 4];
    let mut insert = |d: f64| {
        if !(d > x(low[0])) {
            low = [d as f32, low[0], low[1], low[2]];
        } else if !(d > x(low[1])) {
            low = [low[0], d as f32, low[1], low[2]];
        } else if !(d > x(low[2])) {
            low[3] = low[2];
            low[2] = d as f32;
        } else if !(d > x(low[3])) {
            low[3] = d as f32;
        }
    };
    let rel = |p: [f32; 3]| [0, 1, 2].map(|k| (x(p[k]) - x(mean[k])) as f32);
    let upright = car.ground_up_3a0 > f32::from_bits(K_UPRIGHT) && car.flags_52c & 0x80 == 0;
    if upright {
        for &p in car.world_points_1dc.iter().take(4) {
            insert(dot_ext(normal, rel(p)));
        }
        let h = (((x(low[3]) + x(low[2]) + x(low[1]) + x(low[0])) * 0.25 + x(low[0])) * 0.5) as f32;
        car.height_428 = h;
        car.height_424 = h;
        return;
    }
    for &p in &car.world_points_1dc {
        insert(dot_ext(normal, rel(p)));
    }
    car.height_424 = low[0];
    car.height_428 = ((x(low[2]) + x(low[1]) + x(low[0])) * c(K_THIRD)) as f32;
}

/// `Car_WheelGroundAndAttitude` `0x499a70` (see the module docs). Draws
/// from `lcg` once per wheel when `+0x3a0 ≥ 0.86` (material noise).
pub fn wheel_ground_and_attitude(
    car: &mut OriginalCar,
    query: &mut impl GroundQuery,
    lcg: &mut OriginalLcg,
) -> WheelGround {
    let ride = car.specs.ride_height_120;
    let travel = car.specs.suspension_travel_124;
    let up = car.axes_364[1];
    let g = car.globals;
    let mut multipliers = [1.0f32; 4];
    let mut sum = [0.0f32; 3];
    let mut contacts = [[0.0f32; 3]; 4];
    let mut queries = [[0.0f32; 3]; 4];
    let mut on_support = [false; 4];
    for (i, q) in queries.iter_mut().enumerate() {
        // The original always has at least four body points (0x49abc0).
        let p = car
            .world_points_1dc
            .get(i)
            .copied()
            .unwrap_or(car.position_330);
        *q = [p[0], (x(ride) * x(up[1]) + x(p[1])) as f32, p[2]];
    }

    // First loop (0x499b2c..0x499f10).
    for i in 0..4 {
        let q = queries[i];
        car.wheels[i].query_34 = q;
        let hit = query.wheel_ground(i, q);
        let flags = hit.map_or(0, |h| h.surface);
        let remap = g.handling_mode_6573f8 != 0 && matches!(flags & 15, 1 | 10);
        car.wheels[i].tyre.surface_8c = if remap { 2 } else { flags as i32 };
        // 0x494480: 1 without a surface nibble, 2 for a plane > 4 m away.
        let polygon = hit.filter(|h| {
            h.surface & 15 != 0
                && !(abs87(plane_ext(q, h.normal, h.base) - x(car.position_330[1])) > 4.0)
        });
        let (normal, height) = match polygon {
            None => {
                let n = if car.alt_normal_3e8[1] > f32::from_bits(K_0_1) {
                    car.alt_normal_3e8
                } else {
                    [0.0, 1.0, 0.0]
                };
                (n, plane_ext(q, n, car.alt_base_400) as f32)
            }
            Some(h) => {
                on_support[i] = true;
                let base = plane_ext(q, h.normal, h.base) as f32;
                let noise = material_noise(car, i, lcg);
                let mut y = (noise + x(base)) as f32;
                if i < 2 {
                    let turn = (i as f64 * 0.5 + x(car.wheels[i].spin_88)) * c(K_WOBBLE_TURN);
                    y = (turn.sin() * car.damage.alignment * f64::from_bits(K_WOBBLE) + x(y))
                        as f32;
                }
                (h.normal, y)
            }
        };
        let contact = [q[0], height, q[2]];
        for k in 0..3 {
            sum[k] = (x(sum[k]) + x(contact[k])) as f32;
        }
        contacts[i] = contact;
        let front = i < 2;
        let w = &mut car.wheels[i];
        w.point_40 = contact;
        w.normal_4c = normal;
        if g.sched_step_606ac4 < 2 || (g.pair_damp_657408 != 0 && car.remote_dac != 0) {
            w.angle_70 = 0.0;
            w.compression_68 = 0.0;
            continue;
        }
        let delta: [f32; 3] = [0, 1, 2].map(|k| (x(contact[k]) - x(q[k])) as f32);
        let mut proj = dot_ext(delta, up);
        if up[1] < f32::from_bits(K_0_3) {
            proj = c(K_NEG_0_1);
        }
        if proj > 0.0 {
            let factor = if front { c(K_0_9) } else { 0.75 };
            let ceiling = (x(travel) - x(ride)) * factor;
            if proj > ceiling + ceiling {
                multipliers[i] = 1.25;
            }
            let c32 = ceiling as f32;
            if proj > x(c32) {
                proj = x(c32);
            }
            w.compression_68 = proj as f32;
        } else {
            let k = if w.airborne_84 != 0 {
                K_NEG_1_65
            } else {
                K_NEG_1_6
            };
            let floor = (x(travel) * c(k)) as f32;
            if !(proj > x(floor)) {
                proj = x(floor);
            }
            w.compression_68 = ((proj + x(w.compression_68)) * 0.5) as f32;
        }
        let mut arg = (x(ride) * c(K_0_1)) as f32;
        if proj > 0.0 {
            arg = (x(w.compression_68) * c(K_0_1) + x(arg)) as f32;
        }
        let a = x(arg).asin();
        w.angle_70 = if i == 1 || i == 3 { -a } else { a } as f32;
    }
    let mean = sum.map(|s| (x(s) * 0.25) as f32);

    // Second loop (0x499f60..0x49a1fb).
    let mut off = 0u32;
    let [r, _, f] = car.axes_364;
    for i in 0..4 {
        let w = &mut car.wheels[i];
        let gap = x(w.point_40[1]) - x(queries[i][1]);
        w.gap_64 = gap as f32;
        let k = c(K_NEG_0_30625);
        let d = if k > gap { k } else { gap };
        let (mut e4, mut e0, mut dc) = (
            (d * x(r[1])) as f32,
            (d * x(up[1])) as f32,
            (d * x(f[1])) as f32,
        );
        if !(car.ground_up_3a0 > 0.0) {
            e4 = (-(d * x(r[1]))) as f32;
            e0 = (-(d * x(up[1]))) as f32;
            dc = (-(d * x(f[1]))) as f32;
        }
        let mut extra = 0.0f64;
        if !(d >= c(K_AIRBORNE)) {
            w.airborne_84 = 1;
            off += 1;
            if car.res_518 == 0 {
                e0 = (x(e0) * c(K_0_8)) as f32;
            }
            w.load_6c = e0;
        } else {
            w.load_6c = (x(e0) / x(up[1])) as f32;
            let v = car.velocity_33c;
            if abs87(x(v[0])) > c(K_0_01) && abs87(x(v[2])) > c(K_0_01) {
                let dir = normalize([-v[0], 0.0, -v[2]]);
                let t = cross(w.normal_4c, dir);
                let slide = cross(t, w.normal_4c);
                let s = if w.airborne_84 != 0 {
                    abs87(dot_ext(v, r)) * c(K_1_15)
                } else {
                    abs87(dot_ext(car.accel_d40, r)) * 0.0625
                };
                extra = dot_ext(scale(slide, s as f32), up);
            }
            w.airborne_84 = 0;
        }
        let m = x(multipliers[i]);
        w.disp_58 = [
            (x(e4) * m) as f32,
            (x(e0) * m + extra) as f32,
            (x(dc) * m) as f32,
        ];
    }

    let (normal, raw_y) = contact_normal(&contacts);
    if raw_y.is_some_and(|y| y < 0.5) {
        off = 4;
    }

    // Tail (0x49a3ec..0x49a8aa).
    if !(car.ground_up_3a0 >= f32::from_bits(K_ATTITUDE)) {
        car.roll_filter_dc8 = (x(car.roll_filter_dc8) * 0.5) as f32;
        car.pitch_filter_dc4 = (x(car.pitch_filter_dc4) * 0.5) as f32;
    } else if off < 4 {
        attitude(car, off);
    }
    car.wheels_off_418 = (off > 1) as u16;
    update_world_points(car);
    ground_height(car, normal, mean);
    WheelGround {
        mean,
        normal,
        wheels_off: off,
        on_support,
    }
}

/// `0x5327e0` / `0x5327f0`: `SinTurns`, `CosTurns` (left on the x87 stack).
fn sin_turns(a: f32) -> f64 {
    (x(a) * f64::from_bits(TWO_PI)).sin()
}

fn cos_turns(a: f32) -> f64 {
    (x(a) * f64::from_bits(TWO_PI)).cos()
}

/// The pitch/roll model of `0x499a70` (`0x49a432..0x49a8a4`).
fn attitude(car: &mut OriginalCar, off: u32) {
    let s = &car.specs;
    let light = car.res_518 != 0;
    let div = if light { 4.0 } else { 8.0 };
    let w = car.angular_388;
    let [r, u, f] = car.axes_364;
    car.angular_ground_d64[0] = dot(w, r);
    car.angular_ground_d64[1] = dot(w, u);
    let roll_ext = dot_ext(w, f);
    car.angular_ground_d64[2] = roll_ext as f32;
    let disp = |i: usize| x(car.wheels[i].disp_58[1]);
    let pitch0 = ((disp(2) + disp(3)) - (disp(0) + disp(1))) / div;
    let remote = car.remote_dac != 0;
    let simple = car.flags_52c & 0x20 != 0;
    let d54 = x(car.accel_d4c[2]);
    car.pitch_filter_dc4 = if remote {
        0.0
    } else if simple {
        #[allow(clippy::manual_clamp)] // an unordered d54 becomes -10 (x87)
        let a = if d54 > 10.0 {
            10.0
        } else if !(d54 >= -10.0) {
            -10.0
        } else {
            d54
        };
        (a * x(s.pitch_gain_128) * 0.125 + x(car.pitch_filter_dc4) * c(K_0_8)) as f32
    } else if light {
        (d54 * x(s.pitch_gain_128)) as f32
    } else {
        let k = if d54 > 0.0 { 0.125 } else { c(K_0_175) };
        (x(s.pitch_gain_128) * d54 * k + x(car.pitch_filter_dc4) * c(K_0_8)) as f32
    };
    let p = pitch0 - x(car.pitch_filter_dc4);
    let lim = s.max_pitch_rate_134;
    let pitch = if p > 0.0 {
        if p > x(lim) {
            lim
        } else {
            p as f32
        }
    } else if !(p > x(-lim)) {
        -lim
    } else {
        p as f32
    };
    let roll0 = ((disp(3) + disp(1)) - (disp(2) + disp(0))) / div;
    let d4c = x(car.accel_d4c[0]);
    car.roll_filter_dc8 = if remote {
        0.0
    } else if simple {
        (x(s.roll_gain_12c) * d4c * c(K_0_15) + x(car.roll_filter_dc8) * c(K_0_8)) as f32
    } else if light {
        (x(s.roll_gain_12c) * d4c) as f32
    } else {
        (x(s.roll_gain_12c) * d4c * 0.25 + x(car.roll_filter_dc8) * c(K_0_8)) as f32
    };
    let gas = (1.0 - car.gas_eff_d7f as f64 * c(K_1_1275)) * x(car.roll_filter_dc8);
    let mut roll = roll0 + gas;
    let rlim = s.max_roll_rate_138;
    if roll > 0.0 {
        if !(roll <= x(rlim)) {
            roll = x(rlim);
        }
    } else if !(roll > x(-rlim)) {
        roll = x(-rlim);
    }
    let d = &mut car.angular_ground_d64;
    d[0] = ((x(s.pitch_retention_114) * x(d[0])) * c(K_0_925) + x(pitch)) as f32;
    d[2] = (roll_ext * x(s.roll_retention_118) + roll) as f32;
    clamp_local_angular(car);
    // Mat3_Transpose 0x532a00, then the local rates back into the world.
    let t = [0, 1, 2].map(|i| [r[i], u[i], f[i]]);
    let d = car.angular_ground_d64;
    car.angular_388 = [dot(d, t[0]), dot(d, t[1]), dot(d, t[2])];
    if off == 0 {
        return;
    }
    let side_a = car.wheels[0].airborne_84 != 0 || car.wheels[2].airborne_84 != 0;
    let side_b = car.wheels[1].airborne_84 != 0 || car.wheels[3].airborne_84 != 0;
    if side_a && side_b {
        return;
    }
    let roll = car.angular_ground_d64[2];
    let lx = (sin_turns(roll) * x(car.half_extents_3a8[0]) * -2.0) as f32;
    let mut ly = (cos_turns(roll) * x(car.half_extents_3a8[1]) * -2.0) as f32;
    if (side_a && roll < 0.0) || (side_b && roll > 0.0) {
        ly = -ly;
    }
    let lift = [lx, ly, 0.0];
    let wx = dot(lift, t[0]);
    let wy = dot(lift, t[1]);
    let wz = dot_ext(lift, t[2]);
    let p = &mut car.position_330;
    p[0] = (x(wx) * 0.03125 + x(p[0])) as f32;
    p[1] = (x(wy) * 0.03125 + x(p[1])) as f32;
    p[2] = (wz * 0.03125 + x(p[2])) as f32;
}
