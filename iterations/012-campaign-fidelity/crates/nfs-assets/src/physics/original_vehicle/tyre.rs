//! V2: per-wheel grip and tyre force (carries the handbrake slide).
//!
//! Source: the x87 instructions of
//! - `Car_WheelGripLimits` `0x4a1900`: grip `w+0xac` from the surface x tyre
//!   table, weight split, load transfer and downforce; `w+0x80`, `car+0x84`,
//! - `Wheel_TyreForce` `0x4a20d0`: steer rotation, braking clamp, lock
//!   decision (handbrake), slip-angle lateral force, then
//! - `Wheel_LockedSlide` `0x4a1c70`: a locked tyre gives `(1 - 1/k)·grip`
//!   against the contact-patch slip velocity,
//! - `Wheel_FrictionCircle` `0x4a1dc0`: combined-slip limit with the
//!   post-limit drop.
//!
//! Meanings: `docs/re/vehicle.md` sections 2 and 4 in the workspace root.
//! Every quantity is an acceleration (per unit mass). All four functions are
//! cdecl: `(car)` for the grip, `(car, wheel, unused)` for the others.
//!
//! The damage helpers `Car_DamageTyres` `0x493980` and `Car_DamageAero`
//! `0x493b30` are not ported; their results are inputs ([`TyreEnv`]). They
//! are 0 without parts. The autobahn spatial query `0x473330` is an input too.
//!
//! `SinTurns` / `CosTurns` / `Atan2Turns` (`0x5327e0`, `0x5327f0`,
//! `0x532860`) are `fsin` / `fcos` / `fpatan` on the argument times an f64
//! constant. The fixtures come from Unicorn, which evaluates these with the
//! host's f64 `sin`/`cos`/`atan2`; the port uses the same, so it matches the
//! recording exactly (a real x87 may differ in the last bits).
//!
//! Verified bit for bit against `scripts/research/replay-vehicle-tyre.py`
//! (tests `tests/original_vehicle_tyre.rs`).

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::sim::CarSpecs;

/// The V2 fields of a wheel record (`car+0x7b8 + i*0xc4`; wheels 0,1 front,
/// 2,3 rear). Names keep the record offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelRecord {
    /// 1 when the grip came from the autobahn table (`0x4a1900` writes it).
    pub alt_grip_80: i32,
    /// Surface code; the grip table uses the low 4 bits.
    pub surface_8c: i32,
    /// Lateral force out (wheel frame rotated back to the car frame).
    pub lat_90: f32,
    /// Zeroed on the rolling path.
    pub field_94: f32,
    /// Longitudinal force out (x mu).
    pub long_98: f32,
    /// Cancel accel, lateral (`-16·v_lat ± yaw term`); rotated into the
    /// wheel frame in place.
    pub cancel_lat_9c: f32,
    /// Cancel accel, longitudinal (`-16·v_fwd`); rotated in place.
    pub cancel_long_a4: f32,
    /// Longitudinal demand (drive/brake); clamped and multiplied by mu in
    /// place.
    pub demand_a8: f32,
    /// Grip (friction limit).
    pub grip_ac: f32,
    /// Steer (front), the wheel angle is `sim+0xfc·w+0xb4/17` /256 turn.
    pub steer_b4: f32,
    /// Skid 0..1.
    pub skid_b8: f32,
    /// 1 when the tyre took the locked-slide path.
    pub locked_bc: i32,
    /// 1 for the front wheels.
    pub is_front_c0: i32,
}

/// Car fields read (and `field_84` written) by the V2 functions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TyreCar {
    /// `car+0x84`: set by the grip to 1.16 (+ 8x rear downforce term when
    /// `ground_up_3a0 > 0.939`). Consumer not traced.
    pub field_84: f32,
    /// Ground normal · up.
    pub ground_up_3a0: f32,
    /// Ground-frame normal, y component (`+0x49c` frame vector).
    pub ground_normal_y_4a0: f32,
    /// Flag byte; bit 3 swaps tyre types 0/1 like the AI flag.
    pub flags_52c: u8,
    /// Lateral / longitudinal accel sums of the previous step.
    pub accel_lat_d4c: f32,
    pub accel_long_d54: f32,
    /// Forward velocity in the ground frame.
    pub vel_fwd_d60: f32,
    /// Effective gas / brake, 0..254.
    pub gas_d7f: u8,
    pub brake_d80: u8,
    /// Gear index: 0 R, 1 N, 2 first.
    pub gear_d82: i8,
    pub handbrake_d85: u8,
    pub ai_da8: i32,
    /// Rear / front wheelspin flags (2 = spinning).
    pub spin_rear_db8: i32,
    pub spin_front_dbc: i32,
    /// Slip ratio lateral/forward.
    pub slip_ratio_dc0: f32,
    /// `res+0x20` (rear-stability branch) and `res+0x24` (lock/ABS branch).
    pub res_20: i32,
    pub res_24: i32,
}

/// Globals and unported helper results.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TyreEnv {
    /// `g_Throttle [0x628fb0]`, 0..1.
    pub throttle_628fb0: f32,
    /// `g_SchedStep [0x606ac4]` (the `res+0x24` lock pulses every 4th step).
    pub sched_step_606ac4: i32,
    /// `g_HandlingModeFlag [0x6573f8]`.
    pub handling_mode_6573f8: i32,
    /// `g_TrackIsAutobahn [0x606ae0]`.
    pub autobahn_606ae0: i32,
    /// Result of `0x473330(car+0x330, 0)` (only called on the autobahn; 0
    /// selects the alternate grip table).
    pub track_query_473330: i16,
    /// `Car_DamageTyres` `0x493980` / `Car_DamageAero` `0x493b30`.
    pub damage_tyres: f64,
    pub damage_aero: f64,
}

/// f32 constants of `.rdata`.
fn c(bits: u32) -> f64 {
    f32::from_bits(bits) as f64
}
const K_9_8: u32 = 0x411c_cccd; // 0x5b25bc
const K_HALF: u32 = 0x3f00_0000; // 0x5b23f0
const K_NEG_1_9_8: u32 = 0xbdd0_fac6; // 0x5b4a9c, -0.10204081
const K_NEG_0_2: u32 = 0xbe4c_cccd; // 0x5b4a98
const K_0_2: u32 = 0x3e4c_cccd; // 0x5b23f4
const K_1_1: u32 = 0x3f8c_cccd; // 0x5b27a4
const K_0_005: u32 = 0x3ba3_d70a; // 0x5b3284
const K_0_939: u32 = 0x3f70_624e; // 0x5b4a0c
const K_0_1: u32 = 0x3dcc_cccd; // 0x5b49b4
const K_0_01: u32 = 0x3c23_d70a; // 0x5b27a8
const K_0_9: u32 = 0x3f66_6666; // 0x5b25e8
const K_0_15: u32 = 0x3e19_999a; // 0x5b2638
const K_1_17: u32 = 0x3d70_f0f1; // 0x5b4aa4, 0.05882353
const K_1_256: u32 = 0x3b80_0000; // 0x5b2640
const K_NEG_1_256: u32 = 0xbb80_0000; // 0x5b4aa0
const K_1_12: u32 = 0x3daa_aaab; // 0x5b49b0, 0.083333336
const K_1_2: u32 = 0x3f99_999a; // 0x5b2480
/// `car+0x84` reset value 1.16 (`0x4a1ba3`).
const FIELD_84_RESET: u32 = 0x3f94_7ae1;
/// f64 constants of `SinTurns`/`CosTurns` (`0x5bb2c0`, 2π rounded to f32)
/// and `Atan2Turns` (`0x5bb2c8`, 1/2π rounded to f32).
const TWO_PI: u64 = 0x4019_21fb_2000_0000;
const INV_TWO_PI: u64 = 0x3fc4_5f30_8000_0000;

/// `g_GripTable [0x5d0d50]`, f32[16 surface][3 tyre type].
#[rustfmt::skip]
pub(crate) const GRIP_TABLE: [u32; 48] = [
    0x3f00_0000, 0x3f00_0000, 0x3f00_0000, 0x3f80_0000, 0x3f66_6666, 0x3f8c_cccd,
    0x3f59_999a, 0x3f66_6666, 0x3f59_999a, 0x3f33_3333, 0x3f46_6666, 0x3f20_0000,
    0x3f73_3333, 0x3f66_6666, 0x3f86_6666, 0x3f59_999a, 0x3f60_0000, 0x3f4c_cccd,
    0x3f33_3333, 0x3f4c_cccd, 0x3f19_999a, 0x3f66_6666, 0x3f66_6666, 0x3f80_0000,
    0x3f19_999a, 0x3f40_0000, 0x3f00_0000, 0x3f2c_cccd, 0x3f40_0000, 0x3f19_999a,
    0x3f59_999a, 0x3f4c_cccd, 0x3f73_3333, 0x3f66_6666, 0x3f59_999a, 0x3f66_6666,
    0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000,
    0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000,
];
/// `g_GripTableAutobahn [0x5d0e4c]`, same layout.
#[rustfmt::skip]
pub(crate) const GRIP_TABLE_AUTOBAHN: [u32; 48] = [
    0x3f00_0000, 0x3f00_0000, 0x3f00_0000, 0x3f66_6666, 0x3f80_0000, 0x3f8c_cccd,
    0x3f66_6666, 0x3f59_999a, 0x3f59_999a, 0x3f46_6666, 0x3f33_3333, 0x3f20_0000,
    0x3f66_6666, 0x3f73_3333, 0x3f86_6666, 0x3f60_0000, 0x3f59_999a, 0x3f4c_cccd,
    0x3f4c_cccd, 0x3f33_3333, 0x3f19_999a, 0x3f66_6666, 0x3f66_6666, 0x3f80_0000,
    0x3f40_0000, 0x3f19_999a, 0x3f00_0000, 0x3f40_0000, 0x3f2c_cccd, 0x3f19_999a,
    0x3f4c_cccd, 0x3f59_999a, 0x3f73_3333, 0x3f59_999a, 0x3f66_6666, 0x3f66_6666,
    0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000,
    0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000, 0x3f80_0000,
];
/// `g_SlideFactor [0x5d0f58]`, k per tyre type.
pub const SLIDE_FACTOR: [f32; 3] = [8.0, 8.0, 6.0];

fn x(v: f32) -> f64 {
    v as f64
}

/// `fabs` as the original writes it (`fcomp 0`, `fchs` if below): keeps -0.
fn abs87(v: f64) -> f64 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

fn slide_factor(specs: &CarSpecs) -> f64 {
    x(SLIDE_FACTOR[specs.tyre_type_1c0 as usize])
}

/// `SinTurns` `0x5327e0` / `CosTurns` `0x5327f0` of an f32 argument.
fn sin_cos_turns(t: f32) -> (f64, f64) {
    let a = x(t) * f64::from_bits(TWO_PI);
    (a.sin(), a.cos())
}

/// `Atan2Turns(y, x)` `0x532860`.
fn atan2_turns(y: f32, xv: f32) -> f64 {
    x(y).atan2(x(xv)) * f64::from_bits(INV_TWO_PI)
}

/// `Car_WheelGripLimits` `0x4a1900`: grip `w+0xac` and `w+0x80` for all
/// wheels, and `car+0x84`.
///
/// Panics if the tyre type is outside 0..=2 (the original reads past the row).
pub fn grip_limits(
    car: &mut TyreCar,
    wheels: &mut [WheelRecord; 4],
    specs: &CarSpecs,
    env: &TyreEnv,
) {
    let load = (x(car.ground_normal_y_4a0) * x(specs.tyre_mu_f4) * c(K_9_8)) as f32;
    let alt = env.autobahn_606ae0 != 0 && env.track_query_473330 == 0;
    let swap = car.flags_52c & 8 != 0 || car.ai_da8 != 0;
    for (i, w) in wheels.iter_mut().enumerate() {
        let mut tyre = specs.tyre_type_1c0;
        let table = if alt {
            if swap && tyre == 0 {
                tyre = 1;
            }
            w.alt_grip_80 = 1;
            &GRIP_TABLE_AUTOBAHN
        } else {
            if swap && tyre == 1 {
                tyre = 0;
            }
            w.alt_grip_80 = 0;
            &GRIP_TABLE
        };
        let factor = c(table[(tyre + (w.surface_8c & 0xf) * 3) as usize]);
        w.grip_ac = if i < 2 {
            (factor * x(specs.front_weight_f0) * x(load) * c(K_HALF)) as f32
        } else {
            ((1.0 - x(specs.front_weight_f0)) * factor * x(load) * c(K_HALF)) as f32
        };
    }
    // Longitudinal transfer, kept on the x87 stack.
    let mut d =
        x(car.accel_long_d54) * x(specs.cog_height_f8) * c(K_NEG_1_9_8) * x(load) * c(K_NEG_0_2)
            / x(specs.suspension_stiffness_11c);
    if car.gas_d7f != 0 && car.vel_fwd_d60 > 0.0 {
        let mut f = 1.0 - abs87(x(car.slip_ratio_dc0)) * 10.0;
        if 0.0 > f {
            f = 0.0;
        }
        d *= f;
    }
    wheels[0].grip_ac = (x(wheels[0].grip_ac) - d) as f32;
    wheels[1].grip_ac = (x(wheels[1].grip_ac) - d) as f32;
    wheels[2].grip_ac = (d + x(wheels[2].grip_ac)) as f32;
    wheels[3].grip_ac = (d + x(wheels[3].grip_ac)) as f32;
    if env.handling_mode_6573f8 != 0 {
        wheels[0].grip_ac = (x(wheels[0].grip_ac) * c(K_1_1)) as f32;
        wheels[1].grip_ac = (x(wheels[1].grip_ac) * c(K_1_1)) as f32;
    }
    // Lateral transfer between wheels 0,2 and 1,3: the side losing load
    // loses |l|/sim+0x130, the other side gains |l|.
    let l = x(car.accel_lat_d4c) * x(specs.cog_height_f8) * c(K_NEG_1_9_8) * x(load) * c(K_0_2)
        / x(specs.suspension_stiffness_11c);
    let split = x((l / x(specs.lateral_split_130)) as f32);
    if l > 0.0 {
        wheels[0].grip_ac = (x(wheels[0].grip_ac) - split) as f32;
        wheels[2].grip_ac = (x(wheels[2].grip_ac) - split) as f32;
        wheels[1].grip_ac = (l + x(wheels[1].grip_ac)) as f32;
        wheels[3].grip_ac = (l + x(wheels[3].grip_ac)) as f32;
    } else {
        wheels[0].grip_ac = (x(wheels[0].grip_ac) - l) as f32;
        wheels[2].grip_ac = (x(wheels[2].grip_ac) - l) as f32;
        wheels[1].grip_ac = (split + x(wheels[1].grip_ac)) as f32;
        wheels[3].grip_ac = (split + x(wheels[3].grip_ac)) as f32;
    }
    car.field_84 = f32::from_bits(FIELD_84_RESET);
    // Downforce, linear in forward speed.
    if car.vel_fwd_d60 > 5.0 {
        let aero = 1.0 - env.damage_aero * c(K_0_005);
        let front = aero * x(specs.front_downforce_10c) * x(car.vel_fwd_d60);
        wheels[0].grip_ac = (front + x(wheels[0].grip_ac)) as f32;
        wheels[1].grip_ac = (front + x(wheels[1].grip_ac)) as f32;
        let rear = aero * x(specs.rear_downforce_110) * x(car.vel_fwd_d60);
        wheels[2].grip_ac = (rear + x(wheels[2].grip_ac)) as f32;
        wheels[3].grip_ac = (rear + x(wheels[3].grip_ac)) as f32;
        if x(car.ground_up_3a0) > c(K_0_939) {
            car.field_84 = (rear * 8.0 + x(car.field_84)) as f32;
        }
    }
}

/// `Wheel_LockedSlide` `0x4a1c70`: force `(g - g/k)` along the cancel
/// vector, normalised by its octagonal norm; skid `w+0xb8`.
pub fn locked_slide(w: &mut WheelRecord, specs: &CarSpecs, env: &TyreEnv) {
    let g = ((1.0 - env.damage_tyres * c(K_0_005)) * x(w.grip_ac)) as f32;
    let lat = abs87(x(w.cancel_lat_9c));
    let long = abs87(x(w.cancel_long_a4));
    let s = if lat > long {
        long * 0.25 + lat
    } else {
        lat * 0.25 + long
    };
    let s32 = s as f32;
    let target = (x(g) - x(g) / slide_factor(specs)) as f32;
    w.skid_b8 = if s > x(g) {
        ((s - x(g) + x(w.skid_b8) * 3.0) * 0.25) as f32
    } else {
        0.0
    };
    let skid = x(w.skid_b8) * c(K_0_1);
    w.skid_b8 = if skid > 1.0 { 1.0 } else { skid as f32 };
    let factor = if abs87(s) > c(K_0_01) {
        x(target) / x(s32)
    } else {
        lat
    };
    w.lat_90 = (factor * x(w.cancel_lat_9c)) as f32;
    w.long_98 = (factor * x(w.cancel_long_a4)) as f32;
}

/// `Wheel_FrictionCircle` `0x4a1dc0`: limit `(w+0x90, w+0x98)` to the grip
/// with an octagonal norm; past the limit the force drops towards
/// `(1 - 1/k)·g`.
pub fn friction_circle(car: &TyreCar, w: &mut WheelRecord, specs: &CarSpecs, env: &TyreEnv) {
    let long_weight = if env.handling_mode_6573f8 != 0 {
        c(K_HALF)
    } else {
        c(K_0_9)
    };
    let long = abs87(x(w.long_98) * long_weight);
    let lat = abs87(x(w.lat_90));
    let combined = (if lat > long {
        long * 0.25 + lat
    } else {
        lat * 0.25 + long
    }) as f32;
    let g = ((1.0 - env.damage_tyres * c(K_0_005)) * x(w.grip_ac)) as f32;
    let front = w.is_front_c0 != 0;
    let over = abs87(x(w.long_98)) > x(g) && car.gas_d7f > 0x80;
    let spinning = if front {
        over || car.spin_front_dbc == 2
    } else {
        over || car.spin_rear_db8 == 2
    };
    let k = if !front && spinning {
        4.0
    } else {
        SLIDE_FACTOR[specs.tyre_type_1c0 as usize]
    };
    let res20 = car.res_20 != 0;
    if !(combined > g) {
        w.skid_b8 = 0.0;
    } else {
        let mut scale_only = res20 && !front && abs87(x(car.slip_ratio_dc0)) < c(K_0_15);
        if !scale_only && car.ai_da8 != 0 && (w.surface_8c > 1 || specs.tyre_type_1c0 == 2) {
            scale_only = true;
        }
        if scale_only {
            let q = x(g) / x(combined);
            w.skid_b8 = 0.0;
            if car.gear_d82 > 2 || car.ai_da8 != 0 {
                w.lat_90 = (q * x(w.lat_90)) as f32;
            }
            w.long_98 = (q * x(w.long_98)) as f32;
        } else {
            let excess = x(combined) - x(g);
            let a = excess / x(k);
            let b = x(g) / x(k);
            let drop = if b > a { a } else { x(b as f32) };
            let scale = x(((x(g) - drop) / x(combined)) as f32);
            let skid_gain = if front {
                excess * c(K_HALF)
            } else {
                excess * 12.0
            };
            w.skid_b8 = ((x(w.skid_b8) * 3.0 + skid_gain) * 0.25) as f32;
            w.lat_90 = (scale * x(w.lat_90)) as f32;
            w.long_98 = (scale * x(w.long_98)) as f32;
        }
    }
    if !front && spinning && !res20 {
        w.skid_b8 = combined;
    }
    if x(w.skid_b8) > 1.0 {
        w.skid_b8 = 1.0;
    }
    if res20 {
        w.skid_b8 = (x(w.skid_b8) * c(K_HALF)) as f32;
    }
}

/// `Wheel_TyreForce` `0x4a20d0`: the force of one wheel (`w+0x90` lateral,
/// `w+0x98` longitudinal), skid and locked flags.
pub fn tyre_force(car: &TyreCar, w: &mut WheelRecord, specs: &CarSpecs, env: &TyreEnv) {
    let grip = w.grip_ac;
    w.skid_b8 = 0.0;
    w.locked_bc = 0;
    w.long_98 = 0.0;
    w.lat_90 = 0.0;
    // 1. Into the wheel frame.
    let angle = (x(specs.steer_lock_fc) * x(w.steer_b4) * c(K_1_17)) as f32;
    let rotate = !(angle == 0.0 || angle.is_nan());
    if rotate {
        let (s, co) = sin_cos_turns((x(angle) * c(K_1_256)) as f32);
        let s = x(s as f32);
        let (lat, long) = (x(w.cancel_lat_9c), x(w.cancel_long_a4));
        w.cancel_lat_9c = (co * lat - s * long) as f32;
        w.cancel_long_a4 = (co * long + s * lat) as f32;
    }
    // 2. Braking clamp: a demand along the cancel term (opposing the
    //    motion) cannot exceed it.
    let mut clamped = false;
    let (demand, cancel) = (w.demand_a8, w.cancel_long_a4);
    if !(demand >= 0.0) && !(cancel >= 0.0) {
        if !(env.throttle_628fb0 > 0.25) || car.gear_d82 != 0 {
            w.demand_a8 = if demand > cancel { demand } else { cancel };
            clamped = true;
        }
    } else if demand > 0.0 && cancel > 0.0 && (!(env.throttle_628fb0 > 0.25) || car.gear_d82 <= 1) {
        w.demand_a8 = if demand > cancel { cancel } else { demand };
        clamped = true;
    }
    let demand = (x(specs.tyre_mu_f4) * x(w.demand_a8)) as f32;
    w.demand_a8 = demand;
    // 3. Lock decision.
    let over_grip = clamped && abs87(x(demand)) > x(w.grip_ac);
    let handbrake_rear =
        car.handbrake_d85 != 0 && w.is_front_c0 == 0 && abs87(x(car.vel_fwd_d60)) > 0.5;
    if over_grip || handbrake_rear {
        if car.handbrake_d85 != 0 {
            return lock(w, specs, env);
        }
        if car.res_24 == 0
            && (16.0 - x(w.grip_ac)) * 2.0 >= x(car.vel_fwd_d60)
            && car.brake_d80 >= 0xec
            && (abs87(x(car.vel_fwd_d60)) >= 5.0 || car.spin_rear_db8 != 0)
            && (car.ai_da8 == 0 || (w.surface_8c <= 1 && specs.tyre_type_1c0 != 2))
        {
            return lock(w, specs, env);
        }
        if demand > w.grip_ac {
            w.demand_a8 = w.grip_ac;
        } else if demand < -w.grip_ac {
            w.demand_a8 = -w.grip_ac;
        }
        if car.res_24 != 0 && car.vel_fwd_d60 < 25.0 {
            if env.sched_step_606ac4 % 4 == 0 {
                return lock(w, specs, env);
            }
            w.skid_b8 = 0.0;
        }
    }
    // 4. Slip angle in 1/256 turn: atan(-2·v_lat/v_fwd), mirrored backwards.
    let (lat, long) = (w.cancel_lat_9c, w.cancel_long_a4);
    let (alpha, alpha32) = if abs87(x(long)) == 0.0 || long.is_nan() {
        (0.0, 0.0f32)
    } else {
        let t = atan2_turns(lat, (x(long) * -0.5) as f32);
        let a = if long > 0.0 {
            if lat > 0.0 {
                (0.5 - t) * 256.0
            } else if lat < 0.0 {
                (-0.5 - t) * 256.0
            } else {
                t * 256.0
            }
        } else {
            t * 256.0
        };
        (a, a as f32)
    };
    // 5. Lateral force.
    let force = if w.is_front_c0 == 0 {
        // Rear: saturates at |alpha| >= 2.
        let a = abs87(alpha) as f32;
        let m = if !(a > 1.0) {
            1.0
        } else if a > 2.0 {
            2.0
        } else {
            x(a)
        };
        let r = m * x(grip) * c(K_HALF);
        Some(if alpha32 < 0.0 { -r } else { r })
    } else {
        // Front: linear up to 16, limited to the cancel term below ~2 m/s.
        let mut f = abs87(alpha);
        if f > 16.0 {
            f = 16.0;
        }
        f = f * x(grip) * c(K_1_12);
        if alpha32 < 0.0 {
            f = -f;
        }
        w.lat_90 = f as f32;
        (abs87(x(long)) + abs87(x(lat)) < 32.0).then_some(f)
    };
    if let Some(f) = force {
        let lat = x(w.cancel_lat_9c);
        w.lat_90 = if lat > 0.0 {
            let fa = abs87(f);
            if lat > fa {
                fa
            } else {
                lat
            }
        } else {
            let na = -abs87(f);
            if lat > na {
                lat
            } else {
                na
            }
        } as f32;
    }
    // 6./7. Longitudinal and the friction circle.
    w.field_94 = 0.0;
    w.long_98 = w.demand_a8;
    friction_circle(car, w, specs, env);
    if env.handling_mode_6573f8 != 0 && w.is_front_c0 != 0 {
        w.lat_90 = (x(w.lat_90) * c(K_1_2)) as f32;
    }
    // 8. Back to the car frame.
    if rotate {
        let (s, co) = sin_cos_turns((x(angle) * c(K_NEG_1_256)) as f32);
        let s = x(s as f32);
        let (lat, long) = (x(w.lat_90), x(w.long_98));
        w.lat_90 = (co * lat - s * long) as f32;
        w.long_98 = (co * long + s * lat) as f32;
    }
}

/// The locked path of `0x4a20d0` (`0x4a2494`): no rotation back.
fn lock(w: &mut WheelRecord, specs: &CarSpecs, env: &TyreEnv) {
    locked_slide(w, specs, env);
    w.locked_bc = 1;
}
