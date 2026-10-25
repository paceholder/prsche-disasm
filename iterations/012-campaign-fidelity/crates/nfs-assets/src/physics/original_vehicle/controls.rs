//! V4: controls. The input word to the car's control fields, the steering
//! rate, gear requests and bookkeeping, the automatic R/N/1 logic with
//! auto-reverse, and the race-start throttle/brake gating.
//!
//! Source: the instructions of
//! - `Car_ReadInputs` `0x4991e0`: cdecl `(car)`, 32 Hz queue prio 0x15.
//!   Reads `g_InputWord [0x5e9664]` (byte 0: bit 0 handbrake, bit 1 horn,
//!   bits 3..7 event; byte 1 steer, signed; byte 2 gas/2; byte 3 brake/2)
//!   into `d7c/d7d/d90/d85/d86`, the event bookkeeping `d8a/d8b/d8c`, the
//!   toggles `d87` with `+0x1230/+0x1232`, and the gear request `d7e`.
//! - `Input_GearRequest` `0x413890`: cdecl `(gear, sim+0x50)`.
//! - `Car_ApplyControls` `0x49dd20`: cdecl `(car)`, called by
//!   `Car_PlanarDynamics` `0x49f160` [VA 0x49f3d4]. Writes `d7e..d85`, `d94`,
//!   `d98`, `db0`, `g_Throttle [0x628fb0]`, `g_Brake [0x628fac]` and
//!   `g_SteerAbs [0x628fb4]`.
//!
//! Meanings: `docs/re/vehicle.md` sections 2 and 3 in the workspace root.
//! Not ported (the caller's business): the `0x516950` call with network
//! players (`[0x5e8e28] > 0`, player 0), the `+0xe10` callback when
//! `+0x52c` bit 2 is set, and the AI planners `0x40b650`/`0x40bfe0` (and
//! `0x4a0680`) that fill `d9c/d9d/da0` before the AI path copies them.
//!
//! Verified bit for bit against `scripts/research/replay-vehicle-controls.py`
//! (tests `tests/original_vehicle_controls.rs`).

// Negated comparisons keep the x87 branches on unordered (NaN) operands.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::sim::CarSpecs;

/// Event codes in bits 3..7 of the input word (`Input_PollControllers`
/// `0x412e70`: event index + 1).
pub const EVENT_SHIFT_UP: u8 = 2;
pub const EVENT_SHIFT_DOWN: u8 = 3;

/// Gear indexes (`d82`): R, N, first.
pub const GEAR_REVERSE: i8 = 0;
pub const GEAR_NEUTRAL: i8 = 1;
pub const GEAR_FIRST: i8 = 2;

/// Car fields read and written by the V4 functions. Names keep the car
/// struct offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControlsCar {
    /// Remote/replay source (`dac != 0`): steer, brake, handbrake/horn bits.
    pub remote_steer_38: i8,
    pub remote_brake_3a: u8,
    pub remote_bits_3b: u8,
    /// Speed `|vel|`.
    pub speed_35c: f32,
    /// Player index (selects `[0x658098 + player·0x580]`).
    pub player_520: i32,
    /// Flag byte; bit 2 with `+0xb50 ≥ 2` cuts the controls.
    pub flags_52c: u8,
    /// Override flags: forced half throttle (reversing) or half brake.
    pub field_540: i32,
    pub field_544: i32,
    /// Blocks the automatic logic when non-zero; ≥ 2 with `+0x52c` bit 2
    /// cuts the controls.
    pub field_b50: i32,
    /// 2 forces the throttle to 0.
    pub field_d2c: i32,
    /// Forward velocity in the ground frame.
    pub vel_fwd_d60: f32,
    /// Raw gas / brake 0..254 (from the input word).
    pub gas_d7c: u8,
    pub brake_d7d: u8,
    /// Requested gear.
    pub gear_req_d7e: i8,
    /// Effective gas / brake.
    pub gas_eff_d7f: u8,
    pub brake_eff_d80: u8,
    /// Previous / current gear (0 R, 1 N, 2 first).
    pub prev_gear_d81: i8,
    pub gear_d82: i8,
    /// 1 while a downshift is in progress.
    pub downshift_d83: u8,
    /// Shift timer, 32 Hz steps (signed byte).
    pub shift_timer_d84: i8,
    pub handbrake_d85: u8,
    pub horn_d86: u8,
    /// Toggle bits flipped by events 10..13 and 15.
    pub toggles_d87: u8,
    /// Byte 0 of the input word, its event code, and the last event 7..9.
    pub word0_d8a: u8,
    pub event_d8b: u8,
    pub event_d8c: u8,
    /// Raw / rate-limited steer (±127 range, ±254 from the word).
    pub steer_in_d90: i32,
    pub steer_d94: i32,
    /// Auto-reverse active (gas and brake swapped in R).
    pub reverse_d98: i32,
    /// AI planner outputs: gas, brake, steer.
    pub ai_gas_d9c: u8,
    pub ai_brake_d9d: u8,
    pub ai_steer_da0: i32,
    /// AI-driven / remote input source.
    pub ai_da8: i32,
    pub remote_dac: i32,
    /// Race-start counter: 0 = not started, counts up once started.
    pub start_db0: i32,
    /// Engine rpm.
    pub rpm_db4: f32,
    /// AI handbrake-turn side: sign of `eb0 · ed0`.
    pub ai_side_eb0: i32,
    pub ai_side_ed0: f32,
    /// Words with bit 7 set/cleared by the toggle events.
    pub lights_1230: u16,
    pub lights_1232: u16,
}

/// Globals and resource fields.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ControlsEnv {
    /// `g_InputWord [0x5e9664]`.
    pub input_word_5e9664: u32,
    /// `[0x65742c]`: 1 negates the steer (mirrored track).
    pub mirror_65742c: i32,
    /// `g_Throttle [0x628fb0]`, `g_Brake [0x628fac]`, `g_SteerAbs
    /// [0x628fb4]` (outputs).
    pub throttle_628fb0: f32,
    pub brake_628fac: f32,
    pub steer_abs_628fb4: f32,
    /// `g_SchedStep [0x606ac4]` (64 Hz scheduler steps).
    pub sched_step_606ac4: i32,
    /// `[0x658098 + player·0x580]`: 1 = automatic.
    pub auto_box_658098: i32,
    /// `g_HandlingModeFlag [0x6573f8]`.
    pub handling_mode_6573f8: i32,
    /// `[0x5e67b0]`: AI handbrake turn.
    pub ai_handbrake_turn_5e67b0: i32,
    /// `[0x657404]`: > 0 divides gas/brake by 240 instead of 252.
    pub split_657404: i32,
    /// `res+0x14`: steering rate limit on.
    pub res_steer_rate_14: i32,
    /// `res+0x518`: light mode (rate 18).
    pub res_light_mode_518: i32,
}

/// The race-start step before which nothing shifts (`> 0x1c0`) and the
/// brake is held (`< 0x1c0`).
pub const RACE_START_STEP: i32 = 0x1c0;
/// Until this step the automatic box is forced to request first.
pub const RACE_START_FIRST_GEAR_STEP: i32 = 0x1c4;
/// `start_db0` below this holds the throttle at 0 and the brake full.
pub const START_COUNTER_GO: i32 = 0x40;

const K_0_2: f32 = 0.2; // 0x5b23f4 (0x3e4ccccd)

/// `fabs` as the original writes it (`fcomp 0`, `fchs` if below): keeps -0.
fn abs87(v: f64) -> f64 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

/// `Input_GearRequest` `0x413890`: SHIFTUP gives `gear+1` below the top
/// gear `gear_count−1`, SHIFTDOWN `gear−1` above R; else `gear`.
pub fn gear_request(gear: i8, gear_count: i32, input_word: u32) -> i8 {
    let code = (input_word & 0xf8) as u8;
    if code == EVENT_SHIFT_UP << 3 {
        if (gear as i32) < gear_count.wrapping_sub(1) {
            return gear.wrapping_add(1);
        }
    } else if code == EVENT_SHIFT_DOWN << 3 && gear > 0 {
        return gear - 1;
    }
    gear
}

/// `Car_ReadInputs` `0x4991e0`. For an AI car the planners must have
/// filled `d9c/d9d/da0` already (not ported).
pub fn read_inputs(car: &mut ControlsCar, specs: &CarSpecs, env: &ControlsEnv) {
    let w = env.input_word_5e9664;
    let b0 = w as u8;
    car.word0_d8a = b0;
    if car.ai_da8 == 0 {
        if car.remote_dac == 0 {
            car.gas_d7c = ((w >> 16) as u8).wrapping_shl(1);
            car.brake_d7d = ((w >> 24) as u8).wrapping_shl(1);
            let steer = (w >> 8) as u8 as i8 as i32;
            car.steer_in_d90 = if env.mirror_65742c == 1 {
                steer.wrapping_neg().wrapping_shl(1)
            } else {
                steer.wrapping_shl(1)
            };
            car.handbrake_d85 = b0 & 1;
            car.horn_d86 = (b0 >> 1) & 1;
        } else {
            car.steer_in_d90 = car.remote_steer_38 as i32;
            car.brake_d7d = car.remote_brake_3a;
            car.handbrake_d85 = car.remote_bits_3b & 1;
            car.horn_d86 = (car.remote_bits_3b >> 1) & 1;
        }
    } else {
        car.gas_d7c = car.ai_gas_d9c;
        car.steer_in_d90 = car.ai_steer_da0;
        car.brake_d7d = car.ai_brake_d9d;
        car.horn_d86 = 0;
    }
    let event = b0 >> 3;
    car.event_d8b = event;
    match event {
        7..=9 => car.event_d8c = event,
        10 => {
            if car.toggles_d87 & 0x10 != 0 {
                car.toggles_d87 ^= 0x10;
                if car.toggles_d87 & 8 != 0 {
                    return;
                }
            }
            car.lights_1232 = 0;
            car.lights_1230 |= 0x80;
            car.toggles_d87 ^= 8;
        }
        11 => {
            if car.toggles_d87 & 8 != 0 {
                car.toggles_d87 ^= 8;
                if car.toggles_d87 & 0x10 != 0 {
                    return;
                }
            }
            car.lights_1230 = 0;
            car.lights_1232 |= 0x80;
            car.toggles_d87 ^= 0x10;
        }
        12 => {
            if car.toggles_d87 & 8 != 0 && car.toggles_d87 & 0x10 != 0 {
                car.toggles_d87 ^= 0x18;
                car.lights_1230 = 0;
                car.lights_1232 = 0;
            } else {
                car.toggles_d87 |= 0x18;
                car.lights_1230 |= 0x80;
                car.lights_1232 |= 0x80;
            }
        }
        13 => car.toggles_d87 ^= 3,
        15 => car.toggles_d87 ^= 4,
        _ => {}
    }
    // 0x499446..0x499475: a downshift request is dropped over redline.
    let req = gear_request(car.gear_d82, specs.gear_count_50, w);
    if !(car.rpm_db4 > specs.redline_94) || req >= car.gear_d82 {
        car.gear_req_d7e = req;
    }
}

/// `x / divisor` clamped to 1 (`fld 1; fcomp st1`: 1 when not `1 > x`).
fn ratio_clamped(value: u8, divisor: f64) -> f32 {
    let r = value as f64 / divisor;
    if !(1.0 > r) {
        1.0
    } else {
        r as f32
    }
}

/// `Car_ApplyControls` `0x49dd20`.
pub fn apply_controls(car: &mut ControlsCar, specs: &CarSpecs, env: &mut ControlsEnv) {
    if car.start_db0 != 0 {
        car.start_db0 = car.start_db0.wrapping_add(1);
    }
    let cut = (car.flags_52c & 4 != 0 && car.field_b50 >= 2) || car.start_db0 == 0;
    if cut {
        car.gas_eff_d7f = 0;
        car.downshift_d83 = 0;
        car.shift_timer_d84 = 0;
        car.reverse_d98 = 0;
        car.brake_eff_d80 = 0xff;
    } else {
        let gas = car.gas_d7c;
        let brake = car.brake_d7d;
        car.gas_eff_d7f = gas;
        car.brake_eff_d80 = brake;
        if car.shift_timer_d84 > 0 {
            car.shift_timer_d84 -= 1;
        }
        let step = env.sched_step_606ac4;
        if step > RACE_START_STEP {
            let auto = env.auto_box_658098 == 1;
            if auto {
                if step < RACE_START_FIRST_GEAR_STEP {
                    car.gear_req_d7e = GEAR_FIRST;
                } else {
                    auto_reverse(car, gas, brake);
                }
            }
            let req = car.gear_req_d7e;
            let gear = car.gear_d82;
            if req != gear {
                let shift_time = specs.shift_time_54 as u8 as i8;
                if auto || car.ai_da8 != 0 {
                    if req < GEAR_FIRST {
                        car.downshift_d83 = 0;
                        car.prev_gear_d81 = gear;
                        car.gear_d82 = req;
                        car.shift_timer_d84 = shift_time;
                    } else if req == GEAR_FIRST && gear < GEAR_FIRST {
                        // 0x49df43: the highest gear whose redline speed is
                        // below d60 (first if none).
                        car.prev_gear_d81 = gear;
                        let red = specs.redline_94 as f64;
                        let mut pick = 2;
                        for i in 2..specs.gear_count_50.max(2) {
                            let speed = red * specs.inv_ratio_170[i as usize] as f64;
                            if !(speed >= car.vel_fwd_d60 as f64) {
                                pick = i;
                            }
                        }
                        car.downshift_d83 = 0;
                        car.gear_d82 = pick as i8;
                        car.shift_timer_d84 = shift_time;
                    }
                } else {
                    car.downshift_d83 = (gear > req && req > GEAR_NEUTRAL) as u8;
                    car.prev_gear_d81 = gear;
                    car.gear_d82 = req;
                    car.shift_timer_d84 = shift_time;
                }
            }
        }
        steer_rate(car, env);
    }
    // 0x49e006: race-start gating and overrides.
    if car.start_db0 < START_COUNTER_GO {
        car.gas_eff_d7f = 0;
        car.brake_eff_d80 = 0xff;
    } else if env.sched_step_606ac4 < RACE_START_STEP {
        car.brake_eff_d80 = 0xff;
    } else if car.field_540 != 0 || car.field_544 != 0 {
        if car.reverse_d98 != 0 {
            car.gas_eff_d7f = 0x80;
            car.brake_eff_d80 = 0;
        } else {
            car.gas_eff_d7f = 0;
            car.brake_eff_d80 = 0x80;
        }
        car.downshift_d83 = 0;
    }
    if car.field_d2c == 2 {
        car.gas_eff_d7f = 0;
    }
    if env.ai_handbrake_turn_5e67b0 != 0 && car.ai_da8 != 0 {
        let side = car.ai_side_eb0 as f64 * car.ai_side_ed0 as f64;
        car.steer_d94 = if !(side > 0.0) { 0x7c } else { -0x7c };
        car.handbrake_d85 = 1;
    }
    let divisor = if env.split_657404 > 0 { 240.0 } else { 252.0 };
    env.throttle_628fb0 = ratio_clamped(car.gas_eff_d7f, divisor);
    env.brake_628fac = ratio_clamped(car.brake_eff_d80, divisor);
    env.steer_abs_628fb4 = abs87(car.steer_d94 as f64 / (divisor * 0.5)) as f32;
    if env.auto_box_658098 == 1 && car.gear_d82 == GEAR_REVERSE && car.reverse_d98 == 1 {
        env.throttle_628fb0 = ratio_clamped(car.brake_eff_d80, divisor);
        env.brake_628fac = ratio_clamped(car.gas_eff_d7f, divisor);
    }
}

/// 0x49ddf2..0x49dec2 (automatic, after the race start): stopped with the
/// brake held and no gas in a forward gear → request R and set `d98`;
/// stopped in R with `d98` and gas → request first.
fn auto_reverse(car: &mut ControlsCar, gas: u8, brake: u8) {
    let gear = car.gear_d82;
    if car.gear_req_d7e != gear || car.field_b50 != 0 {
        car.reverse_d98 = 0;
        return;
    }
    if car.field_544 != 0 || car.field_540 != 0 {
        return;
    }
    let stopped = !(abs87(car.speed_35c as f64) >= K_0_2 as f64);
    if stopped && brake > 0x80 && gas == 0 && gear > GEAR_NEUTRAL && car.reverse_d98 == 0 {
        car.gear_req_d7e = GEAR_REVERSE;
        car.reverse_d98 = 1;
    } else if stopped && gas > 0x80 && gear == GEAR_REVERSE && car.reverse_d98 != 0 {
        car.gear_req_d7e = GEAR_FIRST;
        car.reverse_d98 = 0;
    }
}

/// 0x49df81..0x49e000: `d94` toward `d90` by at most 12 per call (18 with
/// `g_HandlingModeFlag` or `res+0x518`) when `res+0x14` is set, else a
/// copy; AI cars average the two.
fn steer_rate(car: &mut ControlsCar, env: &ControlsEnv) {
    if car.ai_da8 != 0 {
        car.steer_d94 = car.steer_in_d90.wrapping_add(car.steer_d94) / 2;
    } else if env.res_steer_rate_14 == 0 {
        car.steer_d94 = car.steer_in_d90;
    } else {
        let rate = if env.handling_mode_6573f8 != 0 || env.res_light_mode_518 != 0 {
            0x12
        } else {
            0xc
        };
        let diff = car.steer_in_d90.wrapping_sub(car.steer_d94);
        if diff < 0 {
            let n = diff.wrapping_neg();
            let n = if rate > n { n } else { rate };
            car.steer_d94 = car.steer_d94.wrapping_sub(n);
        } else {
            let n = if rate > diff { diff } else { rate };
            car.steer_d94 = car.steer_d94.wrapping_add(n);
        }
    }
}
