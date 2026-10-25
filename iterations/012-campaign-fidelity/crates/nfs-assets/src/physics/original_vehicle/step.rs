//! V5: the planar 32 Hz step and the 64 Hz integration.
//!
//! Source: the x87 instructions of
//! - `Car_DynamicsTick` `0x49fd30` (32 Hz queue, prio 0x1e): with
//!   `+0x3a0 ≥ 0.5` `Car_PlanarDynamics` `0x49f160`, else the airborne decay.
//! - `Car_PlanarDynamics` `0x49f160`: ground-frame velocities, the tyre
//!   "cancel" vectors with the yaw damping, `Car_ApplyControls` (V4), the
//!   steering target with the gravity drift and `Car_AlignmentNoise`
//!   `0x493bc0` wobble, force feedback, the wheel steer, `Car_EngineDriveForce`
//!   (V3), the brake, `Car_WheelGripLimits` and `Wheel_TyreForce` (V2), the
//!   sums and yaw torque, `Car_SlopeGravity` `0x49f0c0`, `v += a/32`,
//!   `ω.y += t/48`, and the damping cases.
//! - `Car_BuildGroundFrame` `0x49ba30`.
//! - `Car_IntegratePosition` `0x49b7a0` (with `Car_SpinWheels` `0x4a2790`;
//!   `Car_WheelEffects` `0x4a2880` only feeds the effect emitter) and
//!   `Car_IntegrateOrientation` `0x49b8e0` (rotation matrices
//!   `0x5325a0`/`0x532600`/`0x532660`, `Mat3_Mul` `0x532470`,
//!   `Car_UpdateWorldPoints` `0x49ae30`, every 32 calls the
//!   re-orthonormalisation `0x4a4810`).
//!
//! [`step_64hz`] runs one scheduler step in the original order
//! (`Sched_Step` `0x414ff0`, `docs/re/vehicle.md` section 1): on even steps
//! `Car_ReadInputs` then `Car_DynamicsTick` then the ground step, then
//! every step the two integrations, then the step counter.
//!
//! The ground step is a stand-in for `Car_GroundContactTick` `0x49bd40`
//! ([`ground_contact`]): it writes the given ground normal·up, height and
//! normal, builds the ground frame with `0x49ba30`, and computes the speed
//! `+0x35c` with the formula of `0x49bd40`. It does not run the wheel ground
//! query, pitch/roll or vertical velocity; the original ground step is
//! [`super::ground::ground_contact_tick`] (V6), which the engine runs through
//! [`scheduler_step`]. Not run either: the track
//! progress, collisions, and the `0x490140` path of
//! `0x49b8e0` (pair damping with a remote car, `[0x657408]` and `+0xdac`).
//!
//! Verified against `scripts/research/replay-vehicle-step.py` (tests
//! `tests/original_vehicle_step.rs`): every recorded closed-loop run matches
//! bit for bit, for every step.

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::car::{Axes, OriginalCar};
use super::controls::{apply_controls, read_inputs};
use super::engine::engine_drive_force;
use super::tyre::{grip_limits, tyre_force};
use crate::physics::original_lcg::OriginalLcg;

/// Inputs of one scheduler step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Inputs {
    /// `g_InputWord [0x5e9664]` (byte 0 bit 0 handbrake, bit 1 horn, bits
    /// 3..7 event; byte 1 steer/2 signed, positive = right; byte 2 gas/2;
    /// byte 3 brake/2). Read on even steps only.
    pub input_word: u32,
    /// Ground under the car for the ground step.
    pub ground: GroundContact,
}

/// What `Car_GroundContactTick` would supply (see [`ground_contact`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundContact {
    /// Ground normal (world).
    pub normal: [f32; 3],
    /// `+0x3a0`: ground normal · body up.
    pub up_3a0: f32,
    /// `+0x424`: height above the ground.
    pub height_424: f32,
}

impl GroundContact {
    /// A flat plane under an upright car: normal `+y`, `+0x3a0 = 1`, height 0.
    pub const FLAT: Self = Self {
        normal: [0.0, 1.0, 0.0],
        up_3a0: 1.0,
        height_424: 0.0,
    };
}

/// `.rdata` f32 constants.
pub(crate) fn c(bits: u32) -> f64 {
    f32::from_bits(bits) as f64
}
const K_0_05: u32 = 0x3d4c_cccd; // 0x5b26a0
const K_NEG_9_8: u32 = 0xc11c_cccd; // literal in 0x49f338
const K_0_025: u32 = 0x3ccc_cccd; // 0x5b4a2c
const K_0_0125: u32 = 0x3c4c_cccd; // 0x5b4a84
const K_2_16: u32 = 0x3780_0000; // 0x5b253c, 1.5258789e-05
const K_1_15: u32 = 0x3d88_8889; // 0x5b49ec, 0.06666667
const K_0_8: u32 = 0x3f4c_cccd; // 0x5b24e8
const K_1_512: u32 = 0x3b00_0000; // 0x5b4a7c
const K_1_127: u32 = 0x3c01_0204; // 0x5b4a78, 0.007874016
const K_0_005: u32 = 0x3ba3_d70a; // 0x5b3284
const K_1_48: u32 = 0x3caa_aaab; // 0x5b4a74, 0.020833334
const K_0_9: u32 = 0x3f66_6666; // 0x5b25e8
const K_0_1: u32 = 0x3dcc_cccd; // 0x5b2604
const K_0_002: u32 = 0x3b03_126f; // 0x5b4a30
const K_44_64: u32 = 0x4232_8f5c; // 0x5b4a70
const K_0_995: u32 = 0x3f7e_b852; // pushed in 0x49fbac
const K_0_998: u32 = 0x3f7f_7cee; // 0x49fcb9
const K_0_99: u32 = 0x3f7d_70a4; // 0x49fcc2, also 0x5b3f80
const K_0_2: u32 = 0x3e4c_cccd; // 0x5b23f4
const K_0_95: u32 = 0x3f73_3333; // pushed in 0x49fdb6
const K_0_98: u32 = 0x3f7a_e148; // 0x5b25f4
const K_NEG_4_9: u32 = 0xc09c_cccd; // 0x5b2438
const K_0_833: u32 = 0x3f55_3f7d; // 0x5b4a6c
const K_NEG_19_6: u32 = 0xc19c_cccd; // 0x5b4a68
const K_0_01: u32 = 0x3c23_d70a; // 0x5b27a8
/// `SinTurns`/`CosTurns` multiplier `0x5bb2c0` (f64, 2π rounded to f32).
const TWO_PI: u64 = 0x4019_21fb_2000_0000;
/// Re-orthonormalisation series coefficients `0x5d1040..0x5d104c`.
const REORTHO: [f32; 3] = [-0.125, 0.09375, -0.078125];

pub(crate) fn x(v: f32) -> f64 {
    v as f64
}

/// `fabs` as the original writes it (`fcomp 0`, `fchs` if below).
pub(crate) fn abs87(v: f64) -> f64 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

/// `fistp` with the default rounding (nearest, ties to even); out of range
/// gives the integer indefinite.
fn fistp(v: f64) -> i32 {
    let r = v.round_ties_even();
    if (-2147483648.0..2147483648.0).contains(&r) {
        r as i32
    } else {
        i32::MIN
    }
}

// -- vector helpers (`0x532300..0x532b20`, `0x56e8d1`, `0x56e8f5`) ---------

/// `Vec3_Dot` `0x532910`: `(x·x' + y·y') + z·z'`, left on the x87 stack.
pub(crate) fn dot_ext(a: [f32; 3], b: [f32; 3]) -> f64 {
    (x(a[0]) * x(b[0]) + x(a[1]) * x(b[1])) + x(a[2]) * x(b[2])
}

pub(crate) fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    dot_ext(a, b) as f32
}

/// `Vec3_Add` `0x532300`.
pub(crate) fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| (x(a[i]) + x(b[i])) as f32)
}

/// `Vec3_Scale` `0x532360`: `s · v`.
pub(crate) fn scale(v: [f32; 3], s: f32) -> [f32; 3] {
    v.map(|c| (x(s) * x(c)) as f32)
}

/// `Vec3_Cross` `0x532880` (`0x56e8f5`): `a × b` (first argument `a`).
pub(crate) fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let [ax, ay, az] = a.map(x);
    let [bx, by, bz] = b.map(x);
    [
        (ay * bz - az * by) as f32,
        (az * bx - ax * bz) as f32,
        (ax * by - ay * bx) as f32,
    ]
}

/// `Vec3_Normalize` `0x5328d0`: `1/sqrt((z² + y²) + x²)` times each
/// component (no zero check).
pub(crate) fn normalize(v: [f32; 3]) -> [f32; 3] {
    let [vx, vy, vz] = v.map(x);
    let inv = 1.0 / ((vz * vz + vy * vy) + vx * vx).sqrt();
    v.map(|c| (inv * x(c)) as f32)
}

/// `Mat3_Transpose` `0x532a00`.
fn transpose(m: Axes) -> Axes {
    [0, 1, 2].map(|i| [0, 1, 2].map(|j| m[j][i]))
}

/// `Mat3_Mul` `0x532470`: `a·b` (rows of `a` times columns of `b`), with
/// the original's summation order per element.
fn mat_mul(a: Axes, b: Axes) -> Axes {
    let t = |i: usize, j: usize, k: usize| x(a[i][k]) * x(b[k][j]);
    let mut out = [[0.0f32; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, o) in row.iter_mut().enumerate() {
            *o = match (i, j) {
                (1, 1) => (t(i, j, 0) + t(i, j, 1)) + t(i, j, 2),
                (2, 2) => (t(i, j, 0) + t(i, j, 2)) + t(i, j, 1),
                _ => (t(i, j, 1) + t(i, j, 2)) + t(i, j, 0),
            } as f32;
        }
    }
    out
}

/// `0x532800`: `(SinTurns(a), CosTurns(a))` stored as f32.
fn sin_cos_turns(a: f32) -> (f32, f32) {
    let r = x(a) * f64::from_bits(TWO_PI);
    (r.sin() as f32, r.cos() as f32)
}

/// `0x5325a0`: rotation about x by `a` turns.
fn rot_x(a: f32) -> Axes {
    let (s, co) = sin_cos_turns(a);
    [[1.0, 0.0, 0.0], [0.0, co, s], [0.0, -s, co]]
}

/// `0x532600`: rotation about y by `a` turns.
fn rot_y(a: f32) -> Axes {
    let (s, co) = sin_cos_turns(a);
    [[co, 0.0, -s], [0.0, 1.0, 0.0], [s, 0.0, co]]
}

/// `0x532660`: rotation about z by `a` turns.
fn rot_z(a: f32) -> Axes {
    let (s, co) = sin_cos_turns(a);
    [[co, s, 0.0], [-s, co, 0.0], [0.0, 0.0, 1.0]]
}

/// `0x4a4810`: four rounds of `M = M·(I + c1·E + c2·E² + c3·E³)` with
/// `E = MᵀM − I`. The products of the first eight elements are stored as
/// f32 before the sum, the ninth stays on the x87 stack.
pub fn reorthonormalise(m: &mut Axes) {
    const I: Axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..4 {
        let p = mat_mul(transpose(*m), *m);
        let e: Axes = [0, 1, 2].map(|i| [0, 1, 2].map(|j| (x(p[i][j]) - x(I[i][j])) as f32));
        let mut q = I;
        let mut s = I;
        for k in REORTHO {
            q = mat_mul(q, e);
            for n in 0..9 {
                let (i, j) = (n / 3, n % 3);
                let term = x(q[i][j]) * x(k);
                let term = if n < 8 { x(term as f32) } else { term };
                s[i][j] = (x(s[i][j]) + term) as f32;
            }
        }
        *m = mat_mul(*m, s);
    }
}

// -- ground ---------------------------------------------------------------

/// `Car_BuildGroundFrame` `0x49ba30` `(car, normal, up)`: right / normal /
/// forward rows of `+0x490`.
pub fn build_ground_frame(car: &mut OriginalCar, normal: [f32; 3], up: f32) {
    let [r, _, f] = car.axes_364;
    if car.flags_52c & 1 == 0 && up > 0.9f32 {
        car.ground_490 = [r, normal, f];
        return;
    }
    car.ground_490[1] = normal;
    if !(up >= 0.5) {
        let d = dot_ext(r, normal);
        let a = if !(d >= 0.0) { -d } else { d };
        if a > 0.5 {
            let gr = normalize(cross(normal, f));
            car.ground_490[0] = gr;
            car.ground_490[2] = normalize(cross(gr, normal));
            return;
        }
    }
    let gf = normalize(cross(r, normal));
    car.ground_490[2] = gf;
    car.ground_490[0] = normalize(cross(normal, gf));
}

/// Speed `+0x35c` as `Car_GroundContactTick` computes it: `max + 0.25·min`
/// of `|v.x|`, `|v.z|` [VA 0x49bdd0..].
pub fn ground_speed(v: [f32; 3]) -> f32 {
    let a = abs87(x(v[0]));
    let b = abs87(x(v[2]));
    if a <= b {
        (a * 0.25 + b) as f32
    } else {
        (b * 0.25 + a) as f32
    }
}

/// Stand-in for `Car_GroundContactTick` `0x49bd40` (see the module docs):
/// ground fields, the ground frame (`0x49ba30`) and the speed.
pub fn ground_contact(car: &mut OriginalCar, ground: &GroundContact) {
    car.ground_up_3a0 = ground.up_3a0;
    car.height_424 = ground.height_424;
    build_ground_frame(car, ground.normal, ground.up_3a0);
    car.speed_35c = ground_speed(car.velocity_33c);
}

// -- 32 Hz ----------------------------------------------------------------

/// `Car_SlopeGravity` `0x49f0c0`: slope gravity into the accel sums.
pub fn slope_gravity(car: &mut OriginalCar) {
    let g = &car.globals;
    if car.flags_52c & 0x10 != 0 || (g.mode_6573e8 == 3 && g.pair_damp_657408 != 0) {
        return;
    }
    let [r, u, f] = car.axes_364;
    if car.brake_eff_d80 < 0x40 {
        car.accel_d4c[2] = (x(f[1]) * c(K_NEG_4_9) + x(car.accel_d4c[2])) as f32;
    }
    let lat = abs87(x(car.vel_ground_d58[0]));
    if lat > 1.0 || !(x(u[1]) >= c(K_0_833)) {
        car.accel_d4c[0] = (x(r[1]) * c(K_NEG_19_6) + x(car.accel_d4c[0])) as f32;
    }
}

/// `Car_DynamicsTick` `0x49fd30`.
pub fn dynamics_tick(car: &mut OriginalCar, lcg: &mut OriginalLcg) {
    if car.ground_up_3a0 >= 0.5 {
        return planar_dynamics(car, lcg);
    }
    car.spin_rear_db8 = 0;
    car.slip_ratio_dc0 = 0.0;
    for w in &mut car.wheels {
        w.tyre.skid_b8 = 0.0;
    }
    car.rpm_db4 = if !(car.rpm_db4 > 500.0) {
        0.0
    } else {
        (x(car.rpm_db4) - 500.0) as f32
    };
    if !(car.height_424 >= 0.5) {
        car.velocity_33c = scale(car.velocity_33c, f32::from_bits(K_0_95));
        if !(x(car.ground_up_3a0) >= c(K_0_2)) {
            let k = c(K_0_98);
            car.angular_388[1] = (x(car.angular_388[1]) * k) as f32;
            car.pitch_filter_dc4 = (x(car.pitch_filter_dc4) * k) as f32;
            car.roll_filter_dc8 = (x(car.roll_filter_dc8) * k) as f32;
        }
    }
}

/// `Car_PlanarDynamics` `0x49f160`.
pub fn planar_dynamics(car: &mut OriginalCar, lcg: &mut OriginalLcg) {
    let specs = car.specs.clone();
    // Axle contact.
    let airborne = |a: usize, b: usize, car: &OriginalCar| {
        car.wheels[a].airborne_84 != 0
            && car.wheels[b].airborne_84 != 0
            && x(car.height_424) > c(K_0_05)
    };
    let front = !airborne(0, 1, car);
    let rear = !airborne(2, 3, car);
    car.globals.front_contact_628f9c = front as i32;
    car.globals.rear_contact_628f98 = rear as i32;

    // 1. Ground-frame velocities and the slip ratio.
    let [gr, gn, gf] = car.ground_490;
    let (v, w) = (car.velocity_33c, car.angular_388);
    car.vel_ground_d58 = [dot(v, gr), dot(v, gn), dot(v, gf)];
    car.angular_ground_d64 = [dot(w, gr), dot(w, gn), dot(w, gf)];
    let [d58, d5c, d60] = car.vel_ground_d58;
    car.slip_ratio_dc0 = if d60 > 5.0 {
        (x(d58) / x(d60)) as f32
    } else {
        0.0
    };

    // 2. Cancel vectors and the yaw damping through the tyres.
    let lat = (x(d58) * 32.0 * -0.5) as f32;
    let norm = (x(d5c) * 32.0 * -0.5) as f32;
    let fwd = (x(d60) * 32.0 * -0.5) as f32;
    let y = x(specs.yaw_arm_1b4) * x(car.angular_388[1]) * 32.0 * -0.5;
    let speed = x(car.speed_35c);
    let y = if !(speed >= 10.0) {
        (speed + 30.0) * y * c(K_0_025)
    } else {
        (speed + 70.0) * y * c(K_0_0125)
    } as f32;
    let gravity = [0.0f32, f32::from_bits(K_NEG_9_8), 0.0];
    car.globals.gravity_local_628fa0 = [dot(gravity, gr), dot(gravity, gn), dot(gravity, gf)];
    for (i, wheel) in car.wheels.iter_mut().enumerate() {
        wheel.tyre.cancel_lat_9c = if i < 2 {
            (x(y) + x(lat)) as f32
        } else {
            (x(lat) - x(y)) as f32
        };
        wheel.cancel_normal_a0 = norm;
        wheel.tyre.cancel_long_a4 = fwd;
    }

    // 3. Controls.
    // Car_ApplyControls doesn't read the input word.
    let (mut cc, mut ce) = car.controls_view(0);
    apply_controls(&mut cc, &specs, &mut ce);
    car.store_controls(&cc, &ce);

    // 4. Steering target: drift from the gravity's lateral component, the
    //    damage pull, and every 4th step the alignment wobble.
    let gx8 = (x(car.globals.gravity_local_628fa0[0]) * 8.0) as f32;
    let drift = fistp(x(gx8)).clamp(-0x40, 0x40);
    let mut pull = (x(car.damage_4e4[2]) * 0.25 - x(car.damage_4e4[0]) * 0.25) as f32;
    let mut wobble = 0.0f32;
    if car.globals.sched_step_606ac4 % 4 == 0 {
        let r = lcg.next_word();
        let noise = car.damage.alignment;
        wobble = ((r as f64 * c(K_2_16) - 0.5) * noise * 0.5) as f32;
        if !(speed >= 15.0) {
            wobble = (x(wobble) * speed * c(K_1_15)) as f32;
        }
        pull = (x(wobble) + x(pull)) as f32;
    }
    let target = fistp(x(pull))
        .wrapping_add(car.steer_d94)
        .wrapping_add(drift)
        .clamp(-0x7f, 0x7f);

    // 5. Force feedback.
    if !front {
        car.feedback_55c = [0.0; 3];
    } else {
        let mut t = (x(car.wheels[1].tyre.skid_b8) + x(car.wheels[0].tyre.skid_b8)) * 4.0
            / x(specs.tyre_mu_f4);
        if !(c(K_0_8) > t) {
            t = c(K_0_8);
        }
        car.feedback_55c[2] = t as f32;
        car.feedback_55c[0] = (1.0 - t) as f32;
        let mut f = x(car.feedback_55c[2]) * 2000.0 + x(wobble) * 200.0;
        if !(f >= 0.0) {
            f = -f;
        }
        car.feedback_55c[2] = f as f32;
        car.feedback_55c[1] = ((drift as f64 + x(pull)) * x(car.feedback_55c[0]) * 100.0) as f32;
        car.feedback_55c[0] = if !(speed > 50.0) {
            (x(car.feedback_55c[0]) * speed * 200.0) as f32
        } else {
            (x(car.feedback_55c[0]) * 10000.0) as f32
        };
    }

    // 6. Front wheel steer with toe, damage and the speed-dependent lock.
    let mut lock = x(specs.steer_lock_fc);
    if !(speed >= 30.0) {
        lock *= 1.75 - speed * c(K_0_025);
    }
    if !(20.0 > lock) {
        lock = 20.0;
    }
    let lock = lock * (17.0 / x(specs.steer_lock_fc));
    let st = target as f64;
    car.wheels[0].tyre.steer_b4 =
        ((st + x(specs.toe_100)) * (1.0 - x(car.damage_4e4[0]) * c(K_1_512)) * lock * c(K_1_127))
            as f32;
    car.wheels[1].tyre.steer_b4 =
        ((st - x(specs.toe_100)) * (1.0 - x(car.damage_4e4[2]) * c(K_1_512)) * lock * c(K_1_127))
            as f32;
    if d60 > 80.0 {
        let q = x(d60) * c(K_0_0125);
        for w in &mut car.wheels[..2] {
            w.tyre.steer_b4 = (x(w.tyre.steer_b4) / q) as f32;
        }
    }
    for i in 0..2 {
        car.steer_smooth_ac8[i] =
            (x(car.wheels[i].tyre.steer_b4) * 0.5 + x(car.steer_smooth_ac8[i]) * 0.5) as f32;
    }

    // 7. Engine.
    let (mut ec, mut ee) = car.engine_view(*lcg);
    let drive = engine_drive_force(&mut ec, &specs, &mut ee) as f32;
    car.store_engine(&ec, &ee);
    *lcg = ee.rand;
    if !front && !rear {
        car.accel_d4c = [0.0; 3];
        for w in &mut car.wheels {
            w.tyre.skid_b8 = 0.0;
        }
        return;
    }
    car.field_d2c = 0;

    // 8. Brake.
    let mut brake = ((1.0 - car.damage.brakes * c(K_0_005))
        * x(specs.brake_decel_104)
        * x(car.globals.brake_628fac)) as f32;
    let cap = abs87(x(d60)) * 32.0;
    if x(brake) > cap {
        brake = cap as f32;
    }
    if d60 > 0.0 {
        brake = -brake;
    }
    let brake_front = (x(brake) * x(specs.front_brake_share_108)) as f32;

    // 9. Grip.
    let (mut tc, te, mut tw) = car.tyre_view();
    grip_limits(&mut tc, &mut tw, &specs, &te);
    car.store_tyres(&tc, tw);
    car.accel_d4c = [0.0; 3];

    // 10. Demand: drive split by grip per axle, plus the brake.
    let share = x(specs.front_torque_share_5c);
    let g = car.wheels.map(|w| x(w.tyre.grip_ac));
    let f = x(drive);
    let sf = g[1] + g[0];
    car.wheels[0].tyre.demand_a8 = (f * share * g[0] / sf) as f32;
    car.wheels[1].tyre.demand_a8 = (g[1] * share * f / sf) as f32;
    let sr = g[2] + g[3];
    car.wheels[2].tyre.demand_a8 = ((1.0 - share) * g[2] * f / sr) as f32;
    car.wheels[3].tyre.demand_a8 = ((1.0 - share) * g[3] * f / sr) as f32;
    for w in &mut car.wheels {
        w.tyre.locked_bc = 0;
        w.tyre.long_98 = 0.0;
        w.tyre.lat_90 = 0.0;
    }
    let hf = x(brake_front) * 0.5;
    let hr = (x(brake) - x(brake_front)) * 0.5;
    for (i, w) in car.wheels.iter_mut().enumerate() {
        let h = if i < 2 { hf } else { hr };
        w.tyre.demand_a8 = (h + x(w.tyre.demand_a8)) as f32;
    }

    // 11. Tyre forces per axle in contact.
    let (tc, te, mut tw) = car.tyre_view();
    for (i, w) in tw.iter_mut().enumerate() {
        if (i < 2 && front) || (i >= 2 && rear) {
            tyre_force(&tc, w, &specs, &te);
        }
    }
    car.store_tyres(&tc, tw);

    // 12. Sums and the yaw torque.
    let lat_of = |i: usize| x(car.wheels[i].tyre.lat_90);
    let long_of = |i: usize| x(car.wheels[i].tyre.long_98);
    let long = ((long_of(3) + long_of(2)) + long_of(1)) + long_of(0);
    let rear_lat = (lat_of(3) + lat_of(2)) as f32;
    let front_lat = lat_of(1) + lat_of(0);
    let arm = x(specs.inv_yaw_arm_1b8);
    let torque = ((front_lat - x(rear_lat)) * arm
        - (((long_of(2) + long_of(0)) - long_of(1)) - long_of(3)) * arm) as f32;
    car.accel_d4c = [
        (front_lat + x(rear_lat)) as f32,
        0.0,
        (long * x(specs.inv_mu_1bc)) as f32,
    ];
    let t = transpose(car.ground_490);
    slope_gravity(car);

    // 13. Integrate at 32 Hz; yaw at 1/48 (1/32 with res+0x518).
    car.accel_d40 = [
        dot(car.accel_d4c, t[0]),
        dot(car.accel_d4c, t[1]),
        dot(car.accel_d4c, t[2]),
    ];
    let tv = [0.0, torque, 0.0];
    let wz = dot_ext(tv, t[2]);
    car.angular_accel_d34 = [dot(tv, t[0]), dot(tv, t[1]), wz as f32];
    for i in 0..3 {
        car.velocity_33c[i] = (x(car.accel_d40[i]) * 0.03125 + x(car.velocity_33c[i])) as f32;
    }
    car.angular_388[0] = (x(car.angular_accel_d34[0]) * 0.03125 + x(car.angular_388[0])) as f32;
    car.angular_388[2] = (wz * 0.03125 + x(car.angular_388[2])) as f32;
    let k = if car.res_518 == 0 { c(K_1_48) } else { 0.03125 };
    car.angular_388[1] = (x(car.angular_accel_d34[1]) * k + x(car.angular_388[1])) as f32;
    car.filter_e08 = (x(car.accel_d4c[0]) * 0.25 + x(car.filter_e08) * 0.75) as f32;
    car.filter_e04 = (x(car.filter_e04) * c(K_0_9) + x(car.accel_d4c[2]) * c(K_0_1)) as f32;

    // 14. Damping.
    if car.globals.handling_mode_6573f8 != 0 {
        let mut s = abs87(x(car.slip_ratio_dc0));
        if s > 5.0 {
            s = 5.0;
        }
        car.velocity_33c = scale(car.velocity_33c, (1.0 - s * c(K_0_002)) as f32);
    }
    if car.surface_464 == 3 && x(car.speed_35c) > c(K_44_64) {
        car.velocity_33c = scale(car.velocity_33c, f32::from_bits(K_0_995));
        return;
    }
    let speed = x(car.speed_35c);
    if car.gear_d82 > 1 {
        if !(x(car.globals.throttle_628fb0) >= 0.25) && !(speed >= 30.0) && specs.tyre_mu_f4 > 2.5 {
            let f = 1.0 - abs87(x(car.angular_388[1])) * c(K_0_05);
            car.velocity_33c[0] = (f * x(car.velocity_33c[0])) as f32;
            car.velocity_33c[2] = (f * x(car.velocity_33c[2])) as f32;
            return;
        }
    } else if car.gear_d82 == 1 && !(abs87(x(car.globals.gravity_local_628fa0[2])) >= 0.5) {
        let s = if !(abs87(x(d60)) >= 20.0) || car.steer_d94.wrapping_abs() > 0x20 {
            K_0_99
        } else {
            K_0_998
        };
        let s = f32::from_bits(s);
        car.velocity_33c = scale(car.velocity_33c, s);
        car.angular_388 = scale(car.angular_388, s);
        return;
    }
    if car.handbrake_d85 != 0 && !(speed >= 1.0) {
        car.velocity_33c = scale(car.velocity_33c, 0.5);
    }
}

// -- 64 Hz ----------------------------------------------------------------

/// `Car_SpinWheels` `0x4a2790`: wheel spin phase from road speed (remote
/// cars, undriven wheels, neutral) or engine rpm / gear ratio; locked
/// wheels don't spin. Wraps above 1 turn.
pub fn spin_wheels(car: &mut OriginalCar) {
    let s = &car.specs;
    for i in 0..4 {
        let w = &car.wheels[i];
        let rate = if car.remote_dac == 1 {
            Some(x(car.speed_35c) / x(s.tyre_circumference_148))
        } else if w.tyre.locked_bc != 0 {
            None
        } else {
            let front = i < 2;
            let undriven = (!(s.front_torque_share_5c >= f32::from_bits(K_0_01)) && front)
                || (s.front_torque_share_5c > f32::from_bits(K_0_99) && !front);
            Some(if undriven || car.gear_d82 == 1 {
                x(car.vel_fwd_d60()) / x(s.tyre_circumference_148)
            } else {
                // `sim+0x60 + 4·d82` (movsx): R, N, 1st.. ratios.
                let ratio = usize::try_from(car.gear_d82)
                    .ok()
                    .and_then(|g| s.gear_ratio_60.get(g).copied())
                    .unwrap_or_else(|| panic!("gear ratio read for gear {}", car.gear_d82));
                x(car.rpm_db4) / (x(ratio) * x(s.tyre_circumference_148))
            })
        };
        let w = &mut car.wheels[i];
        if let Some(rate) = rate {
            w.spin_88 = (rate * 0.015625 + x(w.spin_88)) as f32;
        }
        if w.spin_88 > 1.0 {
            w.spin_88 = (x(w.spin_88) - fistp(x(w.spin_88)) as f64) as f32;
        }
    }
}

/// `Car_IntegratePosition` `0x49b7a0`: `pos += vel/64`, wheel points
/// likewise, wheel spin. `Car_WheelEffects` `0x4a2880` only passes wheel
/// data to the effect emitter `0x478f30` (rendering/sound) and is left out.
pub fn integrate_position(car: &mut OriginalCar) {
    if car.active_7f == 0 || car.frozen_7d != 0 {
        return;
    }
    let d = car.velocity_33c.map(|v| (x(v) * 0.015625) as f32);
    car.position_330 = [0, 1, 2].map(|i| (x(d[i]) + x(car.position_330[i])) as f32);
    if car.flags_52c & 0x80 == 0 {
        for w in &mut car.wheels {
            w.point_40 = add(w.point_40, d);
        }
        spin_wheels(car);
    }
}

/// `Car_UpdateWorldPoints` `0x49ae30`.
pub fn update_world_points(car: &mut OriginalCar) {
    let t = transpose(car.axes_364);
    let pos = car.position_330;
    car.world_points_1dc = car
        .body_points_8c
        .iter()
        .map(|&p| [0, 1, 2].map(|i| (dot_ext(p, t[i]) + x(pos[i])) as f32))
        .collect();
}

/// `Car_IntegrateOrientation` `0x49b8e0`: `axes = axes·((Rx·Ry)·Rz)` with
/// the angles `ω/64` in turns, world points, and every 32 calls
/// [`reorthonormalise`].
pub fn integrate_orientation(car: &mut OriginalCar) {
    if car.active_7f == 0 || car.frozen_7d != 0 {
        return;
    }
    let a = car.angular_388.map(|w| (x(w) * 0.015625) as f32);
    let ry = rot_y(a[1]);
    let rx = rot_x(a[0]);
    let t = mat_mul(mat_mul(rx, ry), rot_z(a[2]));
    car.axes_364 = mat_mul(car.axes_364, t);
    update_world_points(car);
    car.reortho_80 = car.reortho_80.wrapping_sub(1);
    if car.reortho_80 == 0 {
        reorthonormalise(&mut car.axes_364);
        car.reortho_80 = 0x20;
    }
}

/// A phase of the scheduler step, in the order [`scheduler_step`] runs them
/// (`Sched_Step` `0x414ff0`: the even queue, then the every-step queue, each
/// sorted by priority; `docs/re/vehicle.md` section 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Even queue, prio 6: `Car_TrackLocate` `0x49af70`.
    TrackLocate,
    /// Even queue, prio 0x15: `Car_ReadInputs` `0x4991e0`.
    ReadInputs,
    /// Even queue, prio 0x1e: `Car_DynamicsTick` `0x49fd30`.
    Dynamics,
    /// Even queue, prio 0x1e (after the dynamics): `Car_GroundContactTick`
    /// `0x49bd40`.
    Ground,
    /// Even queue, prio 0x28: `Car_CollideTick` `0x498ff0`.
    Collide,
    /// Every-step queue, prio 0x1e: `Car_IntegratePosition` `0x49b7a0`.
    IntegratePosition,
    /// Every-step queue, prio 0x1e: `Car_IntegrateOrientation` `0x49b8e0`.
    IntegrateOrientation,
    /// Every-step queue, prio 100: `Camera_UpdateAll` `0x463c30`.
    Camera,
}

/// The parts of the scheduler queues that live outside the car model: the
/// ground step, the collision pass and the camera. Each runs at its place in
/// [`scheduler_step`]. `trace` is called before every phase (tests).
pub trait StepQueue {
    /// `Car_TrackLocate` `0x49af70` (even steps, prio 6, first in the
    /// queue). Default: not run.
    fn track_locate(&mut self, _car: &mut OriginalCar) {}
    /// `Car_GroundContactTick` `0x49bd40` (even steps). The material height
    /// noise of `0x499730` draws from the shared random generator.
    fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg);
    /// `Car_CollideTick` `0x498ff0` (even steps). The pair response draws
    /// from the shared random generator.
    fn collide(&mut self, _car: &mut OriginalCar, _lcg: &mut OriginalLcg) {}
    /// `Camera_UpdateAll` `0x463c30` (every step).
    fn camera(&mut self, _car: &OriginalCar) {}
    fn trace(&mut self, _phase: Phase, _car: &OriginalCar) {}
}

/// One 64 Hz scheduler step for the player car in the original order: on
/// even steps (`g_SchedStep` even) the 32 Hz queue `Car_TrackLocate` (prio
/// 6, [`StepQueue::track_locate`]), `Car_ReadInputs` (prio 0x15), `Car_DynamicsTick` and `Car_GroundContactTick` (prio 0x1e, in
/// insertion order), `Car_CollideTick` (prio 0x28); then on every step
/// `Car_IntegratePosition` and `Car_IntegrateOrientation` (prio 0x1e) and the
/// camera (prio 100); then `g_SchedStep += 1`. Not run:
/// `Car_TrackProgressTick` (prio 0x19) and
/// `Car_ContactEffectsTick` (prio 0x32).
pub fn scheduler_step(
    car: &mut OriginalCar,
    input_word: u32,
    lcg: &mut OriginalLcg,
    queue: &mut impl StepQueue,
) {
    if car.globals.sched_step_606ac4 & 1 == 0 {
        queue.trace(Phase::TrackLocate, car);
        queue.track_locate(car);
        queue.trace(Phase::ReadInputs, car);
        let specs = car.specs.clone();
        let (mut cc, ce) = car.controls_view(input_word);
        read_inputs(&mut cc, &specs, &ce);
        car.store_controls(&cc, &ce);
        queue.trace(Phase::Dynamics, car);
        dynamics_tick(car, lcg);
        queue.trace(Phase::Ground, car);
        queue.ground(car, lcg);
        queue.trace(Phase::Collide, car);
        queue.collide(car, lcg);
    }
    queue.trace(Phase::IntegratePosition, car);
    integrate_position(car);
    queue.trace(Phase::IntegrateOrientation, car);
    integrate_orientation(car);
    queue.trace(Phase::Camera, car);
    queue.camera(car);
    car.globals.sched_step_606ac4 = car.globals.sched_step_606ac4.wrapping_add(1);
}

/// One 64 Hz scheduler step with the flat ground stand-in
/// ([`ground_contact`]) the V5 recorder used, and no collision or camera.
pub fn step_64hz(car: &mut OriginalCar, inputs: &Inputs, lcg: &mut OriginalLcg) {
    struct StandIn<'a>(&'a GroundContact);
    impl StepQueue for StandIn<'_> {
        fn ground(&mut self, car: &mut OriginalCar, _lcg: &mut OriginalLcg) {
            ground_contact(car, self.0);
        }
    }
    scheduler_step(car, inputs.input_word, lcg, &mut StandIn(&inputs.ground));
}
