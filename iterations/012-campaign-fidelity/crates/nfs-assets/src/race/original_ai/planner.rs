//! The AI planners called from `Car_ReadInputs` 0x4991e0 for AI cars
//! (`docs/re/ai.md` sections 5-6 in the workspace root): `AI_TargetSpeed`
//! 0x40b650 and `AI_Drive` 0x40bfe0.
//!
//! Floating point follows the x87 code at 53-bit precision: `f64`
//! intermediates, `f32` where the original stores. Vector helpers are the
//! original's (`0x56e6aa` sub, `0x56e660` add, `0x56e8d1` dot,
//! `0x532b20` length, `0x5328d0` normalise, `0x532970` normalise-and-scale,
//! `0x532360` scale, `0x56e8f5` cross).

#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::ais::{AisData, TrackTables};
use super::racing_line::RacingLine;
use super::track::TrackNodes;
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_vehicle::sim::CarSpecs;
use crate::physics::original_vehicle::tyre::{GRIP_TABLE, GRIP_TABLE_AUTOBAHN};

type V3 = [f32; 3];

fn x(v: f32) -> f64 {
    v as f64
}

/// `0x56e8d1`: `(x·x' + y·y') + z·z'`.
fn dot(a: V3, b: V3) -> f64 {
    x(a[0]) * x(b[0]) + x(a[1]) * x(b[1]) + x(a[2]) * x(b[2])
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// `0x532b20`.
fn length(a: V3) -> f64 {
    (x(a[0]) * x(a[0]) + x(a[1]) * x(a[1]) + x(a[2]) * x(a[2])).sqrt()
}

/// `0x5328d0`.
fn normalize(a: V3) -> V3 {
    let inv = 1.0 / length(a);
    [(inv * x(a[0])) as f32, (x(a[1]) * inv) as f32, (x(a[2]) * inv) as f32]
}

/// `0x532360` on one vector.
fn scale(a: V3, s: f32) -> V3 {
    [s * a[0], a[1] * s, a[2] * s]
}

/// `0x532970`: `normalize(a)·s`.
fn scale_to(a: V3, s: f32) -> V3 {
    scale(normalize(a), s)
}

/// `0x56e8f5` (via `0x532880`).
fn cross(a: V3, b: V3) -> V3 {
    [
        (x(a[1]) * x(b[2]) - x(a[2]) * x(b[1])) as f32,
        (x(a[2]) * x(b[0]) - x(a[0]) * x(b[2])) as f32,
        (x(a[0]) * x(b[1]) - x(a[1]) * x(b[0])) as f32,
    ]
}

/// CRT `acos` 0x5a1060: `atan2(sqrt((1+x)(1−x)), x)` below 1, 0 or π at
/// ±1, the indefinite NaN beyond (the error report is skipped).
fn acos87(v: f64) -> f64 {
    if v.abs() < 1.0 {
        ((1.0 + v) * (1.0 - v)).sqrt().atan2(v)
    } else if v == 1.0 {
        0.0
    } else if v == -1.0 {
        std::f64::consts::PI
    } else {
        f64::NAN
    }
}

/// `__ftol` 0x5a0f98: truncation toward zero through a 64-bit `fistp`,
/// low dword returned (NaN and overflow give the indefinite
/// 0x8000000000000000, so 0).
fn ftol(v: f64) -> i32 {
    if v.is_nan() || v.abs() >= 9.2e18 {
        0
    } else {
        v.trunc() as i64 as i32
    }
}

/// 32-bit `fistp` with the default rounding (to nearest, ties to even);
/// NaN and overflow give the integer indefinite 0x80000000.
fn fistp(v: f64) -> i32 {
    let r = v.round_ties_even();
    if r.is_nan() || r < i32::MIN as f64 || r > i32::MAX as f64 {
        i32::MIN
    } else {
        r as i32
    }
}

/// An x87 store of a value that may be a NaN the FPU generated (0·∞): the
/// x87 default NaN is negative (0xffc00000).
fn st32(v: f64) -> f32 {
    if v.is_nan() {
        f32::from_bits(0xffc0_0000)
    } else {
        v as f32
    }
}

/// The AI fields of a car (offset names as in the car struct).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiCar {
    /// `+0x8`: track node (`car+8` locator).
    pub node_08: i16,
    /// `+0xc`: section of the chosen branch (locator `+4`).
    pub section_0c: i32,
    /// `+0xe94`: last junction ahead (index into the junction table).
    pub junction_e94: Option<usize>,
    /// `+0xe98`: chosen end slot of that junction.
    pub branch_e98: i32,
    /// `+0xe9c`: speed class 0 corner / 1 bend / 2 straight.
    pub class_e9c: i32,
    /// `+0xea0`: skill factor.
    pub skill_ea0: f32,
    /// `+0xeb8`: target speed.
    pub target_eb8: f32,
    /// `+0xea4`: slow-step counter.
    pub stuck_ea4: i32,
    /// `+0x1078`: road-edge offset while out of the race.
    pub pull_1078: f32,
    pub speed_35c: f32,
    pub fwd_d60: f32,
    pub gear_d82: i8,
    pub gear_req_d7e: i8,
    pub shift_d84: i8,
    pub pos_330: V3,
    pub r_364: V3,
    pub f_37c: V3,
    pub half_w_3a8: f32,
    /// `+0x3d0`: forward of the current node (`0x499520`).
    pub node_fwd_3d0: V3,
    pub surface_464: i32,
    pub dmg_4e4: f32,
    pub dmg_4ec: f32,
    /// `+0x528`: index in the race order.
    pub order_528: i32,
    pub flags_52c: u32,
    pub progress_af8: f32,
    /// `+0xb50`: race state (2 finished, 3 out).
    pub state_b50: i32,
    pub start_db0: i32,
    pub spin_db8: i32,
    pub hit_1100: i32,
    pub event_d8b: u8,
    pub gas_d9c: u8,
    pub brake_d9d: u8,
    pub steer_da0: i32,
}

/// Another car of the race order, as the planner reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiOther {
    pub pos_330: V3,
    pub half_w_3a8: f32,
    pub fwd_d60: f32,
    pub node_08: i16,
}

/// One slot of the race order `[0x5e8b88]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderSlot {
    /// The car being planned.
    Me,
    Other(AiOther),
}

/// Everything the planners read besides the car.
pub struct AiEnv<'a> {
    pub track: &'a TrackNodes,
    pub line: &'a RacingLine,
    pub specs: &'a CarSpecs,
    pub ais: &'a AisData,
    pub tables: &'a TrackTables,
    /// `res+0x154..0x170`: skill fast/medium/slow/default, then the ranges.
    pub res_skill_154: [f32; 8],
    /// `SKILL [0x6573f4]`.
    pub skill_6573f4: i32,
    /// `REVERSE [0x657430]`.
    pub reverse_657430: bool,
    /// `[0x6573e8]` (3 disables the catch-up).
    pub mode_6573e8: i32,
    /// `g_SchedStep [0x606ac4]`.
    pub step_606ac4: i32,
    /// `[0x606ac0]`: race running.
    pub running_606ac0: i32,
    /// `[0x606ae0]` autobahn: the alternate grip table where `0x473330`
    /// finds no support (`autobahn_alt_473330`).
    pub autobahn_606ae0: bool,
    pub autobahn_alt_473330: bool,
    /// Player `[0x5e8b34]` `+0xaf8` / `+0xb00`.
    pub player_af8: f32,
    pub player_b00: f32,
    /// `[0x5e8b88]`, count `[0x5e8b58]`.
    pub order: &'a [OrderSlot],
}

/// `[0x5cc390 + 4·SKILL]`: two-way branch threshold.
const BRANCH2: [f32; 2] = [0.55, 0.7];
/// `[0x5cc398 + 8·SKILL]`: three-way branch thresholds.
const BRANCH3: [[f32; 2]; 2] = [[0.4, 0.7], [0.5, 0.8]];
/// `[0x5cc384 + 4·SKILL]`: catch-up lead in laps.
const CATCHUP_LEAD: [f32; 3] = [1.0, 0.07, 0.03];
/// `[0x5cc378 + 4·SKILL]`: catch-up speed factor.
const CATCHUP_FACTOR: [f32; 3] = [1.0, 0.98, 0.95];
/// `0x5b253c`: 1/65536.
const K16: f32 = 1.525_878_9e-5;
/// `0x5b2688` (f64): degrees per radian as the original rounds it.
const DEG: f64 = 57.290_000_915_527_344;

fn rand_unit(lcg: &mut OriginalLcg) -> f64 {
    lcg.next_word() as f64 * x(K16)
}

/// `value ± range` from one draw: `r·2·range + (value − range)`.
fn rand_around(lcg: &mut OriginalLcg, value: f32, range: f32) -> f32 {
    let two_range = x(range) + x(range);
    let base = x(value) - x(range);
    let r = lcg.next_word() as f64;
    (base + r * two_range * x(K16)) as f32
}

/// Road width factor `min(1, (half widths)/[0x5e6e34])` of a node.
fn width_factor(t: &TrackNodes, node: i32, tables: &TrackTables) -> f32 {
    let n = &t.nodes[node as usize];
    let w = (x(n.half_width_40[1]) + x(n.half_width_40[0])) / x(tables.width_ref_5e6e34);
    if 1.0 <= w {
        1.0
    } else {
        w as f32
    }
}

/// `AI_TargetSpeed` 0x40b650.
#[allow(clippy::assign_op_pattern)] // `g * x` kept: operand order of the original
pub fn target_speed(car: &mut AiCar, env: &AiEnv, lcg: &mut OriginalLcg) {
    let t = env.track;
    let node = car.node_08 as i32;
    let f0 = t.nodes[node as usize].forward_1c;
    let wf = width_factor(t, node, env.tables);
    let look = (x(car.speed_35c) * x(0.2f32) * x(env.specs.field_13c)) as f32;
    let look = if !(23.0 > look) { 23.0f32 } else { look };
    let look = env.ais.lookahead_20(fistp(x(look)) as usize);
    let look = fistp(x(look));
    let ahead = if env.reverse_657430 { -look } else { look };

    // Junction choice.
    if let Some(j) = t.junction_ahead(node, ahead) {
        if car.junction_e94 != Some(j) {
            let r = rand_unit(lcg) as f32;
            let dir = env.reverse_657430 as i32;
            let jn = &t.junctions[j];
            let end_of = |k: usize| t.sections[jn.ends[k].0].ends[jn.ends[k].1];
            let skill = env.skill_6573f4;
            let matching = if skill < 2 {
                (0..jn.ends.len()).filter(|&k| end_of(k).end == dir).count()
            } else {
                0
            };
            car.junction_e94 = Some(j);
            let mut cum = 0.0f32;
            for k in 0..jn.ends.len() {
                let e = end_of(k);
                if e.end != dir {
                    continue;
                }
                let w = e.weight;
                let take = if skill >= 2 {
                    cum = (x(cum) + x(w)) as f32;
                    r <= cum
                } else if skill == 0 || skill == 1 {
                    let s = skill as usize;
                    match matching {
                        1 => true,
                        2 => (r <= BRANCH2[s]) == !(w < 0.5),
                        3 => {
                            let [a, b] = BRANCH3[s];
                            (r <= a && !(w < 0.4))
                                || (a < r && r <= b && w < 0.4 && !(w < 0.3))
                                || (b < r && w < 0.3)
                        }
                        _ => false,
                    }
                } else {
                    false
                };
                if take {
                    car.branch_e98 = k as i32;
                    car.section_0c = e.section;
                    break;
                }
            }
        }
    }

    // Corner angle within the look-ahead.
    let last = t.nodes.len() as i32 - 1;
    let (mut left, mut right) = (0.0f32, 0.0f32);
    let mut k = 2;
    while k <= look {
        let d = if env.reverse_657430 { -k } else { k };
        let n = t.step(node, d, car.branch_e98).clamp(0, last);
        let rec = &t.nodes[n as usize];
        let a = acos87(dot(rec.forward_1c, f0)) * DEG;
        if 0.0 <= dot(rec.lateral_28, f0) {
            if a > x(right) {
                right = a as f32;
            }
        } else if a > x(left) {
            left = a as f32;
        }
        k += 1;
    }
    let mut seg = 1.0f32;
    for (i, &(a, b)) in env.tables.ranges_5e74ac.iter().enumerate() {
        if a < node && node < b {
            seg = env.tables.factors_5e74fc[i];
        }
    }
    let mut a = (x(left) + x(right)) * x(env.specs.field_144) * x(seg);
    if !(115.0 > a) {
        a = 115.0;
    }
    let a3 = (a * x(0.333_333_34f32)) as f32;
    car.target_eb8 = env.ais.corner_speed_80(fistp(x(a3)) as usize);
    let damage = (x(car.dmg_4ec) + x(car.dmg_4e4)) * 0.5;
    car.target_eb8 = (x(car.target_eb8) / (damage * x(0.006_666_667f32) + 1.0)) as f32;

    // Surface grip.
    let alt = env.autobahn_606ae0 && env.autobahn_alt_473330;
    let surface = car.surface_464 as usize & 0xf;
    let tyre = env.specs.tyre_type_1c0;
    let (table, min) = match (alt, tyre == 2) {
        (false, true) => (&GRIP_TABLE, 0.6f32),
        (false, false) => (&GRIP_TABLE, 0.8),
        (true, true) => (&GRIP_TABLE_AUTOBAHN, 0.5),
        (true, false) => (&GRIP_TABLE_AUTOBAHN, 0.7),
    };
    let col = if tyre == 2 { 2 } else { tyre.clamp(0, 2) as usize };
    let g = f32::from_bits(table[surface * 3 + col]);
    let g = if !(1.0 > g) {
        1.0
    } else if min <= g {
        g
    } else {
        min
    };
    car.target_eb8 = g * car.target_eb8;

    // Skill factor by speed class.
    let rs = env.res_skill_154;
    let def = rand_around(lcg, rs[3], rs[7]);
    let class = if !(a3 <= 20.0) {
        0
    } else if !(a3 <= 10.0) {
        1
    } else {
        2
    };
    if car.class_e9c != class {
        let value = rs[class as usize];
        let range = rs[4 + class as usize];
        car.skill_ea0 = if x(value) < rand_unit(lcg) {
            rand_around(lcg, value, range)
        } else {
            def
        };
    }
    car.class_e9c = class;
    let v = (x(car.skill_ea0) * x(car.target_eb8) * x(wf)) as f32;
    car.target_eb8 = v;

    // Catch-up: slow down when leading the player.
    if env.mode_6573e8 != 3 {
        let s = env.skill_6573f4.clamp(0, 2) as usize;
        let lead = x(car.progress_af8) - x(env.player_af8);
        let avg = (x(env.player_af8) - x(env.player_b00)) * 640000.0 / env.step_606ac4 as f64;
        if x(CATCHUP_LEAD[s]) < lead.abs() && 35.0 < avg && 0.0 < lead {
            car.target_eb8 = v * CATCHUP_FACTOR[s];
        }
    }
}

/// `AI_Drive` 0x40bfe0.
pub fn drive(car: &mut AiCar, env: &AiEnv, lcg: &mut OriginalLcg) {
    let t = env.track;
    let s = env.specs;
    let gear = car.gear_d82;
    let g = if gear > 1 { gear as usize } else { 2 };
    let ratio = x(s.gear_ratio_60[g]);
    let a = ratio * x(car.target_eb8);
    let e = (a - ratio * x(car.fwd_d60)) as f32;
    let gas = ftol(a / x(s.redline_94) * 256.0);
    car.gas_d9c = if 0xfd < gas { 0xfe } else { gas as u8 };
    car.brake_d9d = 0;
    if gear < 2 && 0x1c0 < env.step_606ac4 {
        let p = lcg.next_word();
        car.shift_d84 = ((((p << 3) as i32) >> 16) as i8).wrapping_add(4);
        car.gear_req_d7e = 2;
        car.gear_d82 = 2;
    }
    if e < 0.0 {
        let b = ftol((x(e) + x(e)).abs() / x(s.redline_94) * 256.0);
        let b = if 0xfd < b { 0xfe } else { b as u8 };
        car.brake_d9d = b;
        if b != 0 {
            car.gas_d9c = 0;
        }
    }

    // Look-ahead distance in nodes.
    let node = car.node_08 as i32;
    let wf = width_factor(t, node, env.tables);
    let mut d = (x(s.field_140) * x(car.fwd_d60)).abs() * x(0.190_476_2_f32);
    if wf < 1.0 {
        d *= x(wf);
        if !(2.0 <= d) {
            d = 2.0;
        }
    } else if !(4.0 <= d) {
        d = 4.0;
    }
    if car.speed_35c < 16.0 && 0x380 < env.step_606ac4 {
        d *= x(car.speed_35c) * 0.0625;
    }
    let look = ftol(d);

    // Target: mean of four racing-line points.
    let last = t.nodes.len() as i32 - 1;
    let pos = car.pos_330;
    let mut sum = [0.0f32; 3];
    for i in 0..4 {
        let delta = if env.reverse_657430 { -look - i } else { i + look };
        let n = t.step(node, delta, car.branch_e98).clamp(0, last);
        let p = env.line.point(n);
        sum[0] = (x(p[0]) + x(sum[0])) as f32;
        sum[1] = (x(t.nodes[n as usize].pos_04[1]) + x(sum[1]) + 1.0) as f32;
        sum[2] = (x(p[1]) + x(sum[2])) as f32;
    }
    let mut target = scale(sum, 0.25);

    // Cars just ahead in the race order.
    let count = env.order.len() as i32;
    let mut k = (car.order_528 + 2).min(count - 1);
    while car.order_528 < k && k < count {
        if let OrderSlot::Other(o) = env.order[k as usize] {
            let rel = sub(o.pos_330, pos);
            let aim = normalize(sub(target, pos));
            let dd = dot(rel, aim) as f32;
            let racer = car.flags_52c & 1 != 0;
            if racer || !(6.0 < dd) || !(dd < 100.0) {
                if -1.0 < dd && (dd < 11.0 || (dd < 20.0 && x(car.fwd_d60) - 3.0 > x(o.fwd_d60))) {
                    let along = scale_to(aim, dd);
                    let off = sub(along, rel);
                    let mut f = x(dd) * x(0.05f32);
                    let (add_k, cap) = if racer { (1.15f32, 2.0) } else { (2.0, 3.0) };
                    f += x(add_k);
                    if !(cap > f) {
                        f = cap;
                    }
                    let need = ((x(o.half_w_3a8) + x(car.half_w_3a8)) * f) as f32;
                    if length(off) < x(need) {
                        target = add(o.pos_330, scale_to(normalize(off), need));
                    }
                }
            } else {
                let on = o.node_08 as i32;
                let p = env.line.point(on);
                let line_pt = [p[0], o.pos_330[1], p[1]];
                let to_line = sub(line_pt, o.pos_330);
                let lat = t.nodes[on as usize].lateral_28;
                let d2e = dot(to_line, lat);
                let d2 = d2e as f32;
                let mut f = d2e * x(0.05f32) + 4.0;
                if !(8.0 > f) {
                    f = 8.0;
                }
                let need = (x(o.half_w_3a8) + x(car.half_w_3a8)) * f;
                if x(d2) < need {
                    let off = scale_to(lat, (need - x(d2)) as f32);
                    target = if (look * 6) as f32 <= d2 {
                        add(target, off)
                    } else {
                        add(line_pt, off)
                    };
                }
            }
        }
        k -= 1;
    }

    // Race state.
    if car.state_b50 == 2 {
        car.gas_d9c = 0;
        car.brake_d9d = 0xff;
    } else if car.state_b50 == 3 {
        let r = fistp(x(car.speed_35c)) / 2;
        let mut steps = r.clamp(1, 15);
        if env.reverse_657430 {
            steps = -steps;
        }
        let n2 = t.step(node, steps, car.branch_e98);
        let rec = &t.nodes[n2 as usize];
        let param;
        if car.pull_1078 > 0.0 {
            let v = x(rec.lane_width_48[1]) * rec.lanes_38[1] as f64;
            if v > x(car.pull_1078) {
                car.pull_1078 = v as f32;
            }
            param = car.pull_1078;
        } else if car.pull_1078 < 0.0 {
            let v = -(x(rec.lane_width_48[0]) * rec.lanes_38[0] as f64);
            if v < x(car.pull_1078) {
                car.pull_1078 = v as f32;
            }
            param = car.pull_1078;
        } else {
            let c = cross(t.nodes[node as usize].lateral_28, rec.lateral_28);
            let sign = if env.reverse_657430 { -1.0f32 } else { 1.0 };
            let y = sign * c[1];
            let v = if y < 0.0 {
                x(rec.lane_width_48[1]) * rec.lanes_38[1] as f64
            } else {
                -(x(rec.lane_width_48[0]) * rec.lanes_38[0] as f64)
            };
            let f = if v.abs() <= 5.0 { 10.0 / v.abs() } else { 2.0 };
            param = st32(v * f);
            car.pull_1078 = param;
        }
        target = add(rec.pos_04, scale_to(rec.lateral_28, param));
        car.gas_d9c = 0;
        car.brake_d9d = 0x28;
    }

    // Steering.
    let aim = normalize(sub(target, pos));
    let steer = (dot(aim, car.r_364) * 256.0) as f32;
    let steer = fistp(x(steer));
    car.steer_da0 = steer.clamp(-0x7e, 0x7e);
    if car.spin_db8 == 1 && car.steer_da0.abs() > 0x78 {
        car.gas_d9c = ((car.gas_d9c as i8) / 2) as u8;
    }

    // Slow-step counter and reset requests.
    if car.fwd_d60 < 2.0
        && env.running_606ac0 != 0
        && !(car.flags_52c & 1 != 0 && car.state_b50 >= 2)
    {
        car.stuck_ea4 += 1;
    } else {
        car.stuck_ea4 = 0;
    }
    if car.event_d8b == 5 {
        car.event_d8b = 0;
    }
    if car.start_db0 == 0 {
        if car.hit_1100 < 0x40 {
            car.event_d8b = 5;
        }
        return;
    }
    let along = dot(car.f_37c, car.node_fwd_3d0);
    let wrong = if env.reverse_657430 {
        along > x(0.707f32)
    } else {
        along < x(-0.707f32)
    };
    if wrong && car.speed_35c < 10.0 {
        car.event_d8b = 5;
        return;
    }
    if car.stuck_ea4 >= 0xa0 {
        car.stuck_ea4 = 0;
        car.event_d8b = 5;
    }
}
