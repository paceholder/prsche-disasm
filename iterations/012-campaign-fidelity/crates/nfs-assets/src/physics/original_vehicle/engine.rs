//! V3: engine rpm, drive force, clutch slip, rev limiter, engine braking,
//! cubic drag and the automatic gearbox.
//!
//! Source: the x87 instructions of
//! - `Car_EngineDriveForce` `0x49e440`: cdecl `(car)`, returns the drive
//!   force minus the drag in ST0 (an acceleration; the caller `0x49f6f9`
//!   stores it as f32). Writes rpm `+0xdb4`, the spin flags `+0xdb8/+0xdbc`,
//!   the limiter cut `+0x548`, boost `+0x54c`, `g_Throttle [0x628fb0]` (0
//!   during a cut) and, through the gearbox, the gear fields.
//! - `Car_AutoGearbox` `0x49daf0`: cdecl `(car)`, writes `d81..d84`.
//! - `Car_DraftFactor` `0x49d9b0`: cdecl `(car)`, 1 with no other cars.
//!
//! Meanings: `docs/re/vehicle.md` section 3 in the workspace root.
//! `Car_DamageEngine` `0x4938a0` and `Car_DamageGearbox` `0x493aa0` are not
//! ported; their results are inputs ([`EngineEnv`], 0 without parts). The
//! slipstream of other cars is not ported: [`draft_factor_alone`] is the
//! value with the car count `[0x5e8ae8] = 0`.
//!
//! Verified bit for bit against `scripts/research/replay-vehicle-engine.py`
//! (tests `tests/original_vehicle_engine.rs`).

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::sim::{CarSpecs, CAR_SPECS_SIZE};
use crate::physics::original_lcg::OriginalLcg;

/// Car fields read and written by the V3 functions. Names keep the car
/// struct offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineCar {
    /// Speed (`|vel|`, from `Car_GroundContactTick`).
    pub speed_35c: f32,
    /// Flag byte; bit 2 selects the short N limiter cut.
    pub flags_52c: u8,
    /// Rev-limiter throttle-cut steps.
    pub limiter_548: i32,
    /// Boost 0..1 (engine types 1 and 2).
    pub boost_54c: f32,
    /// Forward velocity in the ground frame.
    pub vel_fwd_d60: f32,
    /// Raw gas 0..254.
    pub gas_d7c: u8,
    /// Requested gear (indexes the downshift blip table).
    pub gear_req_d7e: i8,
    /// Effective brake 0..254.
    pub brake_d80: u8,
    /// Previous gear (0 R, 1 N, 2 first).
    pub prev_gear_d81: i8,
    /// Current gear.
    pub gear_d82: i8,
    /// 1 while a downshift is in progress.
    pub downshift_d83: u8,
    /// Shift timer, 32 Hz steps (signed byte).
    pub shift_timer_d84: i8,
    pub ai_da8: i32,
    /// Engine rpm.
    pub rpm_db4: f32,
    /// Rear / front wheelspin flags (2 = spinning); zeroed every call.
    pub spin_rear_db8: i32,
    pub spin_front_dbc: i32,
    /// Slip ratio lateral/forward.
    pub slip_ratio_dc0: f32,
    /// Wheel grips `w+0xac` (wheels 0,1 front, 2,3 rear).
    pub grip_ac: [f32; 4],
}

/// Globals and unported helper results.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EngineEnv {
    /// `g_Throttle [0x628fb0]`, 0..1; zeroed by a limiter cut.
    pub throttle_628fb0: f32,
    /// `g_RearContact [0x628f98]`.
    pub rear_contact_628f98: i32,
    /// `g_SchedStep [0x606ac4]`.
    pub sched_step_606ac4: i32,
    /// `g_HandlingModeFlag [0x6573f8]`.
    pub handling_mode_6573f8: i32,
    /// `[0x658098 + player·0x580]`: 1 = automatic.
    pub auto_box_658098: i32,
    pub rand: OriginalLcg,
    /// `Car_DamageEngine` `0x4938a0` / `Car_DamageGearbox` `0x493aa0`.
    pub damage_engine: f64,
    pub damage_gearbox: f64,
}

const K_0_1: u32 = 0x3dcc_cccd; // 0x5b2604
const K_NEG_0_1: u32 = 0xbdcc_cccd; // 0x5b2444
const K_0_05: u32 = 0x3d4c_cccd; // 0x5b26a0
const K_0_3: u32 = 0x3e99_999a; // 0x5b25f0
const K_0_8: u32 = 0x3f4c_cccd; // 0x5b24e8
const K_0_9: u32 = 0x3f66_6666; // 0x5b25e8
const K_1_2: u32 = 0x3f99_999a; // 0x5b2480
const K_0_002: u32 = 0x3b03_126f; // 0x5b4a30
const K_0_005: u32 = 0x3ba3_d70a; // 0x5b3284
const K_0_02: u32 = 0x3ca3_d70a; // 0x5b3714
const K_1_6: u32 = 0x3e2a_aaab; // 0x5b49d4
const K_2_3: u32 = 0x3f2a_aaab; // 0x5b4a5c
const K_1_254: u32 = 0x3b81_0204; // 0x5b4a54
const K_1_300: u32 = 0x3b5a_740e; // 0x5b4a64
const K_2_16: f64 = 1.0 / 65536.0; // 0x5b253c

/// `g_DownshiftBlipRpm` `0x5d0c70` and `g_DownshiftBlipRpmBraking`
/// `0x5d0c90` (rpm per step while the clutch is open on a downshift),
/// indexed by `d7e`; the second table follows the first in `.data`.
pub const DOWNSHIFT_BLIP_RPM: [f32; 16] = [
    0.0, 0.0, 250.0, 200.0, 175.0, 150.0, 125.0, 0.0, // 0x5d0c70
    0.0, 0.0, 200.0, 175.0, 150.0, 125.0, 100.0, 0.0, // 0x5d0c90
];

fn c(bits: u32) -> f64 {
    f32::from_bits(bits) as f64
}

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

/// The f32 at `sim + base + 4·index` (the original indexes past the named
/// arrays, e.g. torque point 21 is `sim+0xf0`). Panics outside the struct.
fn word(specs: &CarSpecs, base: usize, index: i32) -> f32 {
    let off = base as i64 + 4 * index as i64;
    assert!(
        (0..=CAR_SPECS_SIZE as i64 - 4).contains(&off),
        "carSpecs read at {off:#x}"
    );
    let off = off as usize;
    let bytes = specs.to_bytes();
    f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
}

fn shift_time(specs: &CarSpecs) -> i8 {
    specs.shift_time_54 as u8 as i8
}

/// `Car_DraftFactor` `0x49d9b0` with no other cars (`[0x5e8ae8] = 0`):
/// `1 - 0`.
pub fn draft_factor_alone() -> f64 {
    1.0 - 0.0
}

/// `Car_AutoGearbox` `0x49daf0`: one random draw, then up/down shift by the
/// speed-derived rpm (engine rpm when that is below 1.2x it).
pub fn auto_gearbox(car: &mut EngineCar, specs: &CarSpecs, env: &mut EngineEnv) {
    let r = env.rand.next_word();
    let red = x(specs.redline_94);
    let up_at = (r as f64 * c(K_0_1) * red * K_2_16 + red * c(K_0_9)) as f32;
    let down_below = (red * c(K_0_8)) as f32;
    let g = car.gear_d82;
    if g < 2 {
        return;
    }
    let slip = abs87(x(car.slip_ratio_dc0)) as f32;
    let n = specs.gear_count_50;
    // The overdrive is skipped when the gear below reaches top speed.
    let skip = x(word(specs, 0x168, n)) * red > x(specs.top_speed_90);
    let top = n - skip as i32 - 1;
    let gi = g as i32;
    let up = if gi < top { gi + 1 } else { gi };
    let down = if g > 2 { gi - 1 } else { gi };
    let speed = if car.vel_fwd_d60 < 0.0 {
        -x(car.speed_35c)
    } else {
        x(car.speed_35c)
    };
    let product = speed * x(word(specs, 0x60, gi));
    let mut rpm = product as f32;
    if product * c(K_1_2) > x(car.rpm_db4) {
        rpm = car.rpm_db4;
    }
    let rpm_down = speed * x(word(specs, 0x60, down));
    let set = |car: &mut EngineCar, gear: i32, downshift: u8| {
        car.downshift_d83 = downshift;
        car.gear_d82 = gear as i8;
        car.shift_timer_d84 = shift_time(specs);
    };
    if !(env.throttle_628fb0 > 0.5) && !(abs87(x(rpm)) > red) {
        // Light throttle within redline: downshift only when not sliding,
        // d81 is left alone.
        if rpm_down >= x(down_below) || down <= 1 || x(slip) >= c(K_0_1) || down == gi {
            return;
        }
        set(car, down, 1);
        return;
    }
    if x(rpm) > x(up_at) && gi < top {
        if up == gi {
            return;
        }
        car.prev_gear_d81 = g;
        set(car, up, 0);
        return;
    }
    if rpm_down >= x(down_below) || down == gi {
        return;
    }
    car.prev_gear_d81 = g;
    set(car, down, 1);
}

/// `Car_EngineDriveForce` `0x49e440`: updates the engine state and returns
/// the drive force minus the cubic drag (f64, the x87 return value).
pub fn engine_drive_force(car: &mut EngineCar, specs: &CarSpecs, env: &mut EngineEnv) -> f64 {
    let red = specs.redline_94;
    let red8 = (x(red) * 0.125) as f32;
    let draw = (env.rand.next_word() as f64 * K_2_16) as f32;
    let misfire = env.damage_engine * c(K_1_300) > x(draw);
    let v = x(car.vel_fwd_d60);
    let drag0 = (v * x(specs.drag_1b0) * v * v) as f32;
    let drag = (draft_factor_alone() * x(drag0)) as f32;
    let g = car.gear_d82;
    let rear = env.rear_contact_628f98 != 0;

    // Target rpm.
    let base = x(red) + if g == 1 || !rear { 250.0 } else { 100.0 };
    let full = base * x(env.throttle_628fb0);
    let mut target = if full >= base {
        base as f32
    } else {
        full as f32
    };

    // Rev limiter.
    if x(car.rpm_db4) >= x(red) && x(red) + 250.0 > x(car.rpm_db4) {
        car.rpm_db4 = (x(red) + 50.0) as f32;
        car.limiter_548 = if g <= 1 || !rear {
            let r = env.rand.next_word();
            if car.flags_52c & 4 == 0 && env.sched_step_606ac4 < 0x1a0 {
                ((r * 20) as i32 >> 16) + 10
            } else {
                ((r << 1) as i32 >> 16) + 2
            }
        } else {
            let r = env.rand.next_word();
            ((r << 1) as i32 >> 16) + g as i32 * 3
        };
    }
    if car.limiter_548 > 0 {
        env.throttle_628fb0 = 0.0;
        target = 0.0;
        car.limiter_548 -= 1;
    }

    // Boost (engine types 1 and 2).
    let kind = specs.engine_type_4c;
    if kind == 1 || kind == 2 {
        let (lo, hi) = if kind == 2 {
            (x(red) * c(K_1_6), x(red) * c(K_2_3))
        } else {
            (x(red) * 0.25, x(red) * 0.5)
        };
        let d = x(car.rpm_db4) - lo;
        let level = if d > 0.0 {
            let q = d / (hi - lo);
            if 1.0 > q {
                q
            } else {
                1.0
            }
        } else {
            0.0
        };
        let mut floor = ((x(car.rpm_db4) / x(red)) * 0.5) as f32;
        if kind == 1 {
            floor = (x(env.throttle_628fb0) * x(floor)) as f32;
        }
        let want = level * x(env.throttle_628fb0);
        let mut b = if x(floor) > want { x(floor) } else { want };
        if car.shift_timer_d84 == 0 {
            let boost = x(car.boost_54c);
            if kind == 2 {
                if !(b > boost) {
                    let t = (boost - c(K_0_1)) as f32;
                    if !(b > x(t)) {
                        b = x(t);
                    }
                }
            } else if b > boost {
                let t = (boost + c(K_0_05)) as f32;
                if b > x(t) {
                    b = x(t);
                }
            } else {
                let t = (boost - c(K_0_05)) as f32;
                if !(b > x(t)) {
                    b = x(t);
                }
            }
            car.boost_54c = b as f32;
        }
    }

    car.spin_front_dbc = 0;
    car.spin_rear_db8 = 0;
    if g == 1 || car.shift_timer_d84 > 0 || !rear {
        free_rev(car, specs, target, misfire, g);
        return 0.0 - x(drag);
    }

    // Clutch engaged.
    if env.auto_box_658098 == 1 || car.ai_da8 != 0 {
        auto_gearbox(car, specs, env);
    }
    let ratio = if car.shift_timer_d84 != 0 && car.downshift_d83 == 0 {
        word(specs, 0x60, car.prev_gear_d81 as i32)
    } else {
        word(specs, 0x60, car.gear_d82 as i32)
    };
    let wheel = (x(ratio) * x(car.vel_fwd_d60)) as f32;
    let mut r = if x(red) > x(car.rpm_db4) {
        x(car.rpm_db4)
    } else {
        x(red)
    };
    if 0.0 > r {
        r = 0.0;
    }
    let idx = (r * c(K_0_002)) as i64 as i32;
    let t0 = x(word(specs, 0x9c, idx));
    let t1 = x(word(specs, 0x9c, idx + 1));
    let torque = (r - (idx * 500) as f64) * c(K_0_002) * (t1 - t0) + t0;
    let gear = car.gear_d82 as i32;
    let f0 = (torque * x(word(specs, 0x190, gear))) as f32;
    let f1 = ((1.0 - env.damage_engine * c(K_1_300)) * x(f0)) as f32;
    let full_force = (1.0 - env.damage_gearbox * c(K_0_005)) * x(f1);

    let wheel_abs = abs87(x(wheel));
    let mut lag = (x(target) - wheel_abs) as f32;
    if abs87(x(lag)) < 125.0 && x(red) - 300.0 > x(target) {
        lag = 0.0;
    }
    let slip = (x(car.rpm_db4) - wheel_abs) as f32;
    let g = car.gear_d82;
    let slipping = (x(slip) > 250.0 && car.shift_timer_d84 == 0 && g != 1)
        || (g > 1 && !(x(car.vel_fwd_d60) >= c(K_NEG_0_1)))
        || (g == 0 && x(car.vel_fwd_d60) > c(K_0_1))
        || (car.spin_rear_db8 == 1 && car.limiter_548 == 0);

    let mut force;
    if slipping {
        // Clutch slip / wheelspin: the full force is transmitted while the
        // rpm falls towards the wheels.
        let awd = specs.front_torque_share_5c > 0.0;
        let demand = abs87(full_force * 15.0);
        let [g0, g1, g2, g3] = car.grip_ac.map(x);
        let excess = if awd {
            120.0 - (demand - (g3 + g2 + g1 + g0))
        } else {
            100.0 - (demand - (g3 + g2))
        };
        let mut gas = car.gas_d7c as f64 * c(K_1_254);
        if c(K_0_3) > gas {
            gas = c(K_0_3);
        }
        let fall = (excess / gas + (1.0 - x(env.throttle_628fb0)) * 100.0) as f32;
        if specs.shift_time_54 <= 4 && g > 2 {
            car.rpm_db4 = (x(car.rpm_db4) - x(slip) * c(K_1_6)) as f32;
        }
        if !(x(env.throttle_628fb0) >= 0.5) || misfire {
            car.rpm_db4 = (x(car.rpm_db4) - x(slip) * c(K_1_6)) as f32;
        } else {
            if x(slip) > x(red8) {
                if awd {
                    car.spin_front_dbc = 2;
                }
                car.spin_rear_db8 = 2;
            }
            let step = if x(slip) > x(fall) { fall } else { slip };
            car.rpm_db4 = (x(car.rpm_db4) - x(step)) as f32;
        }
        force = full_force;
    } else if !(x(lag) >= 0.0) {
        // Over-run: wheels above the target rpm.
        let limit = (x(word(specs, 0x170, g as i32)) * 81920.0) as f32;
        let fall = -x(slip);
        let delta = if g == 0 {
            if fall > x(limit) {
                fall
            } else {
                x(limit)
            }
        } else if fall > x(limit) {
            x(limit)
        } else {
            fall
        };
        if wheel_abs > x(red) + 500.0 {
            force = -full_force;
            car.rpm_db4 = (delta + x(car.rpm_db4)) as f32;
        } else {
            // Engine braking.
            let mut f = full_force;
            if car.limiter_548 > 0 {
                f *= 4.0;
            }
            let braking = (-(f * x(specs.engine_braking_98))) as f32;
            let mut k = env.damage_gearbox * c(K_0_02) + 1.0;
            if !(5.0 > k) {
                k = 5.0;
            }
            force = k * x(braking);
            car.rpm_db4 = (delta + x(car.rpm_db4)) as f32;
            if !(car.rpm_db4 > target) {
                car.rpm_db4 = target;
            }
        }
    } else if lag == 0.0 {
        force = x(drag);
        car.rpm_db4 = wheel;
    } else {
        if misfire {
            car.rpm_db4 = (x(car.rpm_db4) - 100.0) as f32;
            force = 0.0;
        } else {
            if x(slip) > 200.0 {
                car.rpm_db4 = (x(car.rpm_db4) - 200.0) as f32;
            } else if !(x(slip) >= -200.0) {
                car.rpm_db4 = (x(car.rpm_db4) + 200.0) as f32;
            } else {
                car.rpm_db4 = wheel;
            }
            force = full_force * x(env.throttle_628fb0);
        }
        if car.rpm_db4 > target {
            car.rpm_db4 = target;
        }
        if env.handling_mode_6573f8 != 0 {
            let mut a = abs87(x(car.slip_ratio_dc0)) + 1.0;
            if !(2.0 > a) {
                a = 2.0;
            }
            force = force * 1.25 * a;
        }
    }

    // Negative rpm: the engine stalls against the gear unless the force
    // already moves the car in that direction.
    if !(car.rpm_db4 >= 0.0) {
        let stop = (-(x(car.vel_fwd_d60) * 32.0)) as f32;
        let stop = x(stop);
        let keep = (force > 0.0 && stop > 0.0 && force - stop > 0.0)
            || (!(force >= 0.0) && !(stop >= 0.0) && !(force - stop >= 0.0));
        if keep {
            return force - x(drag);
        }
        car.rpm_db4 = 0.0;
        return stop - x(drag);
    }
    let cap = x(red) * 1.25;
    if !(cap >= x(car.rpm_db4)) {
        car.rpm_db4 = cap as f32;
    }
    force - x(drag)
}

/// Clutch open (N, shift timer running or rear axle airborne): no drive
/// force, the rpm revs freely [VA 0x49ef36..0x49f098].
fn free_rev(car: &mut EngineCar, specs: &CarSpecs, target: f32, misfire: bool, gear: i8) {
    let red = specs.redline_94;
    let rpm = x(car.rpm_db4);
    let t = x(target);
    let floor_at_zero = |v: f64| if v > 0.0 { v as f32 } else { 0.0 };
    let towards_target = |car: &mut EngineCar| {
        if !(x(car.rpm_db4) >= t) {
            return;
        }
        let v = x(car.rpm_db4) - 200.0;
        car.rpm_db4 = if v > t { v as f32 } else { target };
    };
    if misfire {
        car.rpm_db4 = floor_at_zero(rpm - 100.0);
    } else if !(rpm >= t) && car.shift_timer_d84 == 0 {
        let v = rpm + 250.0;
        car.rpm_db4 = if !(v > t) { v as f32 } else { target };
    } else if car.shift_timer_d84 <= 0 || car.prev_gear_d81 == 1 {
        towards_target(car);
    } else if car.downshift_d83 != 0 {
        let table = if car.brake_d80 > 0x40 { 8 } else { 0 };
        let blip = DOWNSHIFT_BLIP_RPM[table + car.gear_req_d7e as usize];
        car.rpm_db4 = (x(blip) + rpm) as f32;
        if car.rpm_db4 > red {
            car.rpm_db4 = red;
        }
    } else {
        let fall = if gear >= 4 { 200.0 } else { 100.0 };
        let v = ((rpm - fall) as f32) as f64;
        car.rpm_db4 = floor_at_zero(v);
    }
    let b = x(car.boost_54c) - c(K_0_1);
    car.boost_54c = if !(0.0 > b) { b as f32 } else { 0.0 };
}
