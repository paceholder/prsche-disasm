//! V1: SIM load scaling and derived carSpecs fields.
//!
//! Source: the x87 instructions of
//! - `Sim_Load` `0x49c6c0` from `0x49c82f` (after the file read: carSpecs is
//!   zeroed and the 328 `.sim` bytes are copied to its start), unit scaling
//!   and the `res+0x518` light-mode factors,
//! - `Sim_DeriveFields` `0x49cb50`: tyre type, tyre circumference, fe.txt
//!   tuning (`res+0xf0..0x140`), rpm-per-m/s gear table, drive factors, cubic
//!   drag constant, yaw arm, 1/mu.
//!
//! Field meanings: `docs/re/vehicle.md` section 5 in the workspace root.
//!
//! Not ported: the part modifiers (`car+0x768`, 20 records, kinds 1..9) and
//! the damage they read; the port covers the default no-parts car. The car
//! field `car+0x10d8` (mu after parts / mu before) is only used inside the
//! function here (it is 1 without parts).
//!
//! The light-mode (`res+0x518`), cheat-string (`res+0x17c`), `res+0xc != 0`
//! and non-default tuning branches are ported from the instructions but not
//! covered by the recorded fixtures (default tuning only).
//!
//! Verified bit for bit against `scripts/research/replay-vehicle-sim.py`
//! (tests `tests/original_vehicle_sim.rs`, all 86 installed `.sim` files).

// Negated comparisons keep the x87 branches on unordered (NaN) operands:
// the original tests C0/C3 after `fcomp`, so "not greater" includes NaN.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

/// Size of a `.sim` file and of the loaded part of carSpecs.
pub const SIM_FILE_SIZE: usize = 0x148;
/// Size of the carSpecs struct (`car+0x760`).
pub const CAR_SPECS_SIZE: usize = 0x1c4;
const WORDS: usize = CAR_SPECS_SIZE / 4;

/// Resource/tuning inputs read by the derivation (`res = car+0x558`).
/// Tuning values are fe.txt 16.16 fixed-point integers.
#[derive(Clone, Debug, PartialEq)]
pub struct Tuning {
    /// `res+0x2a8`, model / AI-spec name (`<name>.ais`). Compared without
    /// case: GT1, GT2, GT3, 935 select tyre type 2; 959 sets front weight
    /// 0.4675.
    pub model_2a8: String,
    /// `res+0x17c`, compared with "SILSPD" / "ROLLEY" (case-sensitive).
    pub cheat_17c: String,
    /// `res+0xc`: when 0 (and no "SILSPD"), drive factors are x1.05.
    pub flag_c: i32,
    /// `res+0x518`, light mode: wheelbase x0.8, pitch/roll retention x0.833,
    /// mu x1.25 at load.
    pub light_mode_518: i32,
    /// `res+0xf0`, FINALRATIO, added to the final drive.
    pub final_ratio_f0: i32,
    /// `res+0xf4 + 4i`, added to gear ratio `sim+0x60 + 4i` (index 1,
    /// neutral, is skipped).
    pub gear_ratio_f4: [i32; 8],
    /// `res+0x114` / `res+0x118`, frontdownforce / reardownforce.
    pub front_downforce_114: i32,
    pub rear_downforce_118: i32,
    /// `res+0x11c`, brakebias (divided by 100 as an integer first).
    pub brake_bias_11c: i32,
    /// `res+0x120`, rideheight; replaces `sim+0x120`.
    pub ride_height_120: i32,
    /// `res+0x124`, suspensiontravel.
    pub suspension_travel_124: i32,
    /// `res+0x128`, steeringtoe.
    pub steering_toe_128: i32,
    /// `res+0x134`, suspensionstiffness.
    pub suspension_stiffness_134: i32,
    /// `res+0x13c` / `res+0x140`, front / rear tirepressure.
    pub front_tyre_pressure_13c: i32,
    pub rear_tyre_pressure_140: i32,
}

impl Default for Tuning {
    /// fe.txt `car[0].*` defaults; unknown resource fields zero, model empty.
    fn default() -> Self {
        Self {
            model_2a8: String::new(),
            cheat_17c: String::new(),
            flag_c: 0,
            light_mode_518: 0,
            final_ratio_f0: 0,
            gear_ratio_f4: [0; 8],
            front_downforce_114: 3_276_800,
            rear_downforce_118: 3_276_800,
            brake_bias_11c: 3_276_800,
            ride_height_120: 0,
            suspension_travel_124: 0,
            steering_toe_128: 0,
            suspension_stiffness_134: 65_536,
            front_tyre_pressure_13c: 2_293_760,
            rear_tyre_pressure_140: 2_293_760,
        }
    }
}

impl Tuning {
    /// Default tuning for a model name (`res+0x2a8`).
    pub fn for_model(model: &str) -> Self {
        Self {
            model_2a8: model.to_string(),
            ..Self::default()
        }
    }
}

/// The carSpecs struct (`car+0x760`, 0x1c4 bytes) after derivation. Names
/// keep the original offsets. Gear arrays are indexed R, N, 1st..6th.
#[derive(Clone, Debug, PartialEq)]
pub struct CarSpecs {
    /// Car name, ASCII (not read by the vehicle code).
    pub name_00: [u8; 0x40],
    /// Mass kg.
    pub mass_40: f32,
    /// Wheelbase m.
    pub wheelbase_44: f32,
    /// Integer flag, consumer not found.
    pub flag_48: i32,
    /// Engine: 0 NA, 1 turbo, 2 supercharged.
    pub engine_type_4c: i32,
    /// Gear slots incl. R and N (forward gears + 2).
    pub gear_count_50: i32,
    /// Shift time in 32 Hz steps.
    pub shift_time_54: i32,
    /// Drive efficiency fraction.
    pub drive_efficiency_58: f32,
    /// Front torque share fraction.
    pub front_torque_share_5c: f32,
    /// Gear ratios, after derivation rpm per m/s (`ratio*final*60/circumference`).
    pub gear_ratio_60: [f32; 8],
    pub final_drive_80: f32,
    pub tyre_width_84: f32,
    pub tyre_aspect_88: f32,
    pub rim_8c: f32,
    /// Top speed m/s.
    pub top_speed_90: f32,
    pub redline_94: f32,
    /// Engine-braking fraction.
    pub engine_braking_98: f32,
    /// Torque curve Nm, every 500 rpm.
    pub torque_9c: [f32; 21],
    /// Front weight share fraction.
    pub front_weight_f0: f32,
    /// Tyre mu.
    pub tyre_mu_f4: f32,
    /// CoG height fraction for load transfer.
    pub cog_height_f8: f32,
    /// Steering lock, 1/256 turn.
    pub steer_lock_fc: f32,
    pub toe_100: f32,
    pub brake_decel_104: f32,
    pub front_brake_share_108: f32,
    /// Front / rear downforce per m/s (`0x4a1be7`, `0x4a1c0d`).
    pub front_downforce_10c: f32,
    pub rear_downforce_110: f32,
    /// Pitch / roll rate retention.
    pub pitch_retention_114: f32,
    pub roll_retention_118: f32,
    pub suspension_stiffness_11c: f32,
    pub ride_height_120: f32,
    pub suspension_travel_124: f32,
    /// Pitch / roll gain from acceleration.
    pub pitch_gain_128: f32,
    pub roll_gain_12c: f32,
    /// Rear/front lateral transfer split.
    pub lateral_split_130: f32,
    /// Max pitch / roll rate change per step.
    pub max_pitch_rate_134: f32,
    pub max_roll_rate_138: f32,
    pub field_13c: f32,
    pub field_140: f32,
    pub field_144: f32,
    /// Derived: tyre circumference m.
    pub tyre_circumference_148: f32,
    pub inv_redline_14c: f32,
    /// `1/(inv_ratio*redline)` per gear slot.
    pub ratio_per_redline_150: [f32; 8],
    /// `1/ratio_rpm` (0.1 for a zero ratio) per gear slot.
    pub inv_ratio_170: [f32; 8],
    /// Drive factor `ratio_rpm/mass*0.1*eff` (x1.05 / x1.2) per gear slot.
    pub drive_factor_190: [f32; 8],
    /// Cubic drag constant.
    pub drag_1b0: f32,
    /// Yaw arm `L/2*0.025*256` and its inverse.
    pub yaw_arm_1b4: f32,
    pub inv_yaw_arm_1b8: f32,
    pub inv_mu_1bc: f32,
    /// Tyre type 0 / 2.
    pub tyre_type_1c0: i32,
}

/// carSpecs as the original's dword array.
struct Words([u32; WORDS]);

impl Words {
    fn f(&self, off: usize) -> f32 {
        f32::from_bits(self.0[off / 4])
    }
    fn x(&self, off: usize) -> f64 {
        self.f(off) as f64
    }
    fn set(&mut self, off: usize, v: f64) {
        self.0[off / 4] = (v as f32).to_bits();
    }
    fn i(&self, off: usize) -> i32 {
        self.0[off / 4] as i32
    }
    fn set_i(&mut self, off: usize, v: i32) {
        self.0[off / 4] = v as u32;
    }
    fn arr<const N: usize>(&self, off: usize) -> [f32; N] {
        std::array::from_fn(|i| self.f(off + 4 * i))
    }
}

/// f32 constants of `.rdata`, by address.
fn c(bits: u32) -> f64 {
    f32::from_bits(bits) as f64
}
const K_MPH: u32 = 0x3ee4_9249; // 0x5b4a28, 0.44642857
const K_001: u32 = 0x3c23_d70a; // 0x5b3ca0, 0.01
const K_0001: u32 = 0x3a83_126f; // 0x5b3060, 0.001
const K_08: u32 = 0x3f4c_cccd; // 0x5b24e8 / 0x5b3f00, 0.8
const K_5_6: u32 = 0x3f55_5555; // 0x5b4a24, 0.8333333
const K_125: u32 = 0x3fa0_0000; // 0x5b3f64, 1.25
const K_002: u32 = 0x3ca3_d70a; // 0x5b3714, 0.02
const K_254: u32 = 0x41cb_3333; // 0x5b4a40, 25.4
const K_PI: u32 = 0x3b4d_e34d; // 0x5b4a3c, 0.0031416
const K_FX: u32 = 0x3780_0000; // 0x5b253c, 1/65536
const K_THIRD: u32 = 0x3eaa_aaab; // 0x5b2680, 0.33333334
const K_02: u32 = 0x3e4c_cccd; // 0x5b2694, 0.2
const K_01: u32 = 0x3dcc_cccd; // 0x5b49b4, 0.1
const K_30TH: u32 = 0x3d08_8889; // 0x5b3f30, 0.033333335
const K_10_11: u32 = 0x3f68_ba2e; // 0x5b3354, 0.9090909
const K_12: u32 = 0x3f99_999a; // 0x5b2480, 1.2
const K_105: u32 = 0x3f86_6666; // 0x5b3f90, 1.05
const K_133: u32 = 0x3faa_3d71; // 0x5b4a34, 1.33
const K_0002: u32 = 0x3b03_126f; // 0x5b4a30, 0.002
const K_0025: u32 = 0x3ccc_cccd; // 0x5b4a2c, 0.025
const W_959: u32 = 0x3eef_5c29; // 0.4675, front weight of the 959

/// MSVC `__ftol` `0x5a0f98`: truncation toward zero (integer indefinite out
/// of range).
fn ftol(x: f64) -> i32 {
    if x.is_nan() || x >= 2_147_483_648.0 || x < -2_147_483_648.0 {
        i32::MIN
    } else {
        x as i32
    }
}

/// Fixed-point tuning value as the x87 `fild`, times 1/65536.
fn fx(v: i32) -> f64 {
    v as f64 * c(K_FX)
}

/// Scale a raw `.sim` file as `Sim_Load` does and derive the remaining
/// carSpecs fields as `Sim_DeriveFields` does (no parts).
///
/// Panics unless `raw_sim` is 328 bytes.
pub fn derive(raw_sim: &[u8], tuning: &Tuning) -> CarSpecs {
    assert_eq!(raw_sim.len(), SIM_FILE_SIZE, "a .sim file is 328 bytes");
    let mut s = Words([0; WORDS]);
    for (i, w) in raw_sim.as_chunks::<4>().0.iter().enumerate() {
        s.0[i] = u32::from_le_bytes(*w);
    }
    load_scaling(&mut s, tuning);
    derive_fields(&mut s, tuning);
    to_specs(&s)
}

/// `0x49c82f..0x49ca16`.
fn load_scaling(s: &mut Words, t: &Tuning) {
    s.set_i(0x50, s.i(0x50).wrapping_add(2));
    s.set(0x90, s.x(0x90) * c(K_MPH));
    for off in [0x58, 0x5c, 0x98, 0xf0, 0xf8, 0x108, 0x114, 0x118] {
        s.set(off, s.x(off) * c(K_001));
    }
    for off in [0x44, 0x10c, 0x110, 0x120, 0x124, 0x128, 0x12c] {
        s.set(off, s.x(off) * c(K_0001));
    }
    if t.light_mode_518 != 0 {
        s.set(0x44, s.x(0x44) * c(K_08));
        s.set(0x114, s.x(0x114) * c(K_5_6));
        s.set(0x118, s.x(0x118) * c(K_5_6));
        s.set(0xf4, s.x(0xf4) * c(K_125));
    }
}

/// Max of the 21 torque points as the loop at `0x49cc5e` / `0x49d223`
/// (start 0; take the point unless the running max is greater).
fn max_torque(s: &Words) -> f64 {
    let mut m = 0.0f32;
    for i in 0..21 {
        let t = s.f(0x9c + 4 * i);
        if !(m > t) {
            m = t;
        }
    }
    m as f64
}

/// `0x49cb50`.
fn derive_fields(s: &mut Words, t: &Tuning) {
    let model = t.model_2a8.as_str();
    let is = |name: &str| model.eq_ignore_ascii_case(name);
    s.set_i(
        0x1c0,
        if is("GT1") || is("GT2") || is("GT3") || is("935") {
            2
        } else {
            0
        },
    );
    s.set(0x14c, 1.0 / s.x(0x94));
    s.set(
        0x148,
        (s.x(0x88) * c(K_002) * s.x(0x84) + s.x(0x8c) * c(K_254)) * c(K_PI),
    );
    let torque_max_before = max_torque(s);
    // car+0x10d8: mu before the parts.
    let mu_before = s.f(0xf4);
    if is("959") {
        s.0[0xf0 / 4] = W_959;
    }
    // (Part modifiers 0x49cd0d..0x49d20c: not ported, no parts.)
    let torque_max = max_torque(s);
    s.set(
        0x90,
        (torque_max / torque_max_before + 3.0) * 0.25 * s.x(0x90),
    );
    // fst car+0x10d8, then the extended quotient goes on.
    let mu_ratio = s.x(0xf4) / mu_before as f64;
    s.set(0x140, s.x(0x140) / ((mu_ratio + 2.0) * c(K_THIRD)));

    // Downforce: the front factor stays on the x87 stack, the rear one is
    // stored to a f32 local [esp+0x20].
    let front_df = (fx(t.front_downforce_114) * c(K_002) + 1.0) * 0.5;
    let rear_df = ((fx(t.rear_downforce_118) * c(K_002) + 1.0) * 0.5) as f32 as f64;
    s.set(0x90, s.x(0x90) / ((rear_df + front_df + 3.0) * c(K_02)));
    s.set(0x10c, front_df * s.x(0x10c));
    s.set(0x110, rear_df * s.x(0x110));
    s.set(0x80, fx(t.final_ratio_f0) + s.x(0x80));
    for (i, off) in (0x60..0x80).step_by(4).enumerate() {
        if off != 0x64 {
            s.set(off, fx(t.gear_ratio_f4[i]) + s.x(off));
        }
    }
    s.set(0x108, (fx(t.brake_bias_11c / 100) - 0.5) * 0.5 + s.x(0x108));
    s.set(0x100, fx(t.steering_toe_128) + s.x(0x100));
    s.set(
        0x11c,
        (fx(t.suspension_stiffness_134) + 2.0) * s.x(0x11c) * c(K_THIRD),
    );
    s.set(0xf4, (s.x(0x11c) + 9.0) * c(K_01) * s.x(0xf4));
    s.set(
        0x114,
        (1.0 - fx(t.suspension_stiffness_134)) * c(K_02) + s.x(0x114),
    );
    s.set(
        0x118,
        (1.0 - fx(t.suspension_stiffness_134)) * c(K_02) + s.x(0x118),
    );
    s.set(0x12c, s.x(0x12c) / ((s.x(0x11c) + 1.0) * 0.5));
    let travel = fx(t.suspension_travel_124) * c(K_0001);
    s.set(0x124, travel + s.x(0x124));
    s.set(0xf4, (travel + 1.0) * s.x(0xf4));
    s.set(0x12c, s.x(0x12c) / (travel + 1.0));
    s.set(0x120, fx(t.ride_height_120) * c(K_0001));
    s.set(0x12c, s.x(0x12c) / (s.x(0x120) * 6.0 + 1.0));

    // Tyre pressures (0x49d591): below 1 they are scaled by 10/11.
    let mut front_p = fx(t.front_tyre_pressure_13c) * c(K_30TH);
    let mut rear_p = fx(t.rear_tyre_pressure_140) * c(K_30TH);
    if !(front_p >= 1.0) {
        front_p *= c(K_10_11);
    }
    if !(rear_p >= 1.0) {
        rear_p *= c(K_10_11);
    }
    let sum = rear_p + front_p;
    s.set(0xf4, (sum + 8.0) * s.x(0xf4) * c(K_01));
    s.set(0xf0, ((front_p + front_p) / sum + 1.0) * 0.5 * s.x(0xf0));

    // Gear ratios -> rpm per m/s.
    for off in (0x60..0x80).step_by(4) {
        s.set(off, s.x(0x80) * s.x(off) * 60.0 / s.x(0x148));
    }
    let silspd = t.cheat_17c == "SILSPD";
    let mut g = 0;
    while g < s.i(0x50) {
        let ratio = 0x60 + 4 * g as usize;
        let inv = 0x170 + 4 * g as usize;
        let drive = 0x190 + 4 * g as usize;
        if s.f(ratio) == 0.0 {
            s.0[inv / 4] = K_01;
        } else {
            s.set(inv, 1.0 / s.x(ratio));
        }
        s.set(drive, s.x(ratio) / s.x(0x40));
        s.set(drive, s.x(drive) * c(K_01));
        s.set(drive, s.x(0x58) * s.x(drive));
        if silspd {
            s.set(drive, s.x(drive) * c(K_12));
        } else if t.flag_c == 0 {
            s.set(drive, s.x(drive) * c(K_105));
        }
        s.set(0x150 + 4 * g as usize, 1.0 / (s.x(inv) * s.x(0x94)));
        g += 1;
    }
    if silspd {
        s.set(0xf4, s.x(0xf4) * c(K_12));
        s.set(0x12c, s.x(0x12c) * c(K_08));
        s.set(0x118, s.x(0x118) * c(K_10_11));
    } else if t.cheat_17c == "ROLLEY" {
        s.set(0xf4, s.x(0xf4) * 1.5);
        s.set(0x12c, s.x(0x12c) * c(K_133));
    }

    // Cubic drag (0x49d87b): engine force at top speed in top gear over v^3.
    let top = s.i(0x50);
    let top_ratio = (0x5c + 4 * top) as usize;
    let top_drive = (0x18c + 4 * top) as usize;
    let mut rpm = ftol(s.x(top_ratio) * s.x(0x90)) as f64;
    let redline = s.x(0x94);
    if !(redline > rpm) {
        rpm = redline;
    }
    if 0.0 > rpm {
        rpm = 0.0;
    }
    let idx = ftol(rpm * c(K_0002));
    let t0 = s.x((0x9c + 4 * idx) as usize);
    let t1 = s.x((0xa0 + 4 * idx) as usize);
    let frac = (rpm - (idx * 500) as f64) * c(K_0002);
    let v = s.x(0x90);
    let force = ((t1 - t0) * frac + t0) * s.x(top_drive);
    s.set(0x1b0, force / (v * v * v));

    s.set(0x1b4, s.x(0x44) * 0.5);
    s.set(0x1b4, s.x(0x1b4) * c(K_0025));
    s.set(0x1b4, s.x(0x1b4) * 256.0);
    s.set(0x1b8, 1.0 / s.x(0x1b4));
    s.set(0x1bc, 1.0 / s.x(0xf4));
}

fn to_specs(s: &Words) -> CarSpecs {
    let mut name_00 = [0u8; 0x40];
    for (i, w) in s.0[..0x10].iter().enumerate() {
        name_00[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    CarSpecs {
        name_00,
        mass_40: s.f(0x40),
        wheelbase_44: s.f(0x44),
        flag_48: s.i(0x48),
        engine_type_4c: s.i(0x4c),
        gear_count_50: s.i(0x50),
        shift_time_54: s.i(0x54),
        drive_efficiency_58: s.f(0x58),
        front_torque_share_5c: s.f(0x5c),
        gear_ratio_60: s.arr(0x60),
        final_drive_80: s.f(0x80),
        tyre_width_84: s.f(0x84),
        tyre_aspect_88: s.f(0x88),
        rim_8c: s.f(0x8c),
        top_speed_90: s.f(0x90),
        redline_94: s.f(0x94),
        engine_braking_98: s.f(0x98),
        torque_9c: s.arr(0x9c),
        front_weight_f0: s.f(0xf0),
        tyre_mu_f4: s.f(0xf4),
        cog_height_f8: s.f(0xf8),
        steer_lock_fc: s.f(0xfc),
        toe_100: s.f(0x100),
        brake_decel_104: s.f(0x104),
        front_brake_share_108: s.f(0x108),
        front_downforce_10c: s.f(0x10c),
        rear_downforce_110: s.f(0x110),
        pitch_retention_114: s.f(0x114),
        roll_retention_118: s.f(0x118),
        suspension_stiffness_11c: s.f(0x11c),
        ride_height_120: s.f(0x120),
        suspension_travel_124: s.f(0x124),
        pitch_gain_128: s.f(0x128),
        roll_gain_12c: s.f(0x12c),
        lateral_split_130: s.f(0x130),
        max_pitch_rate_134: s.f(0x134),
        max_roll_rate_138: s.f(0x138),
        field_13c: s.f(0x13c),
        field_140: s.f(0x140),
        field_144: s.f(0x144),
        tyre_circumference_148: s.f(0x148),
        inv_redline_14c: s.f(0x14c),
        ratio_per_redline_150: s.arr(0x150),
        inv_ratio_170: s.arr(0x170),
        drive_factor_190: s.arr(0x190),
        drag_1b0: s.f(0x1b0),
        yaw_arm_1b4: s.f(0x1b4),
        inv_yaw_arm_1b8: s.f(0x1b8),
        inv_mu_1bc: s.f(0x1bc),
        tyre_type_1c0: s.i(0x1c0),
    }
}

impl CarSpecs {
    /// The struct from the original's 0x1c4 bytes (little endian), e.g. a
    /// recorded carSpecs dump.
    pub fn from_bytes(bytes: &[u8; CAR_SPECS_SIZE]) -> Self {
        let mut w = Words([0; WORDS]);
        for (i, b) in bytes.as_chunks::<4>().0.iter().enumerate() {
            w.0[i] = u32::from_le_bytes(*b);
        }
        to_specs(&w)
    }

    /// The struct as the original's 0x1c4 bytes (little endian).
    pub fn to_bytes(&self) -> [u8; CAR_SPECS_SIZE] {
        let mut w = Words([0; WORDS]);
        for (i, b) in self.name_00.as_chunks::<4>().0.iter().enumerate() {
            w.0[i] = u32::from_le_bytes(*b);
        }
        let mut put = |off: usize, values: &[f32]| {
            for (i, v) in values.iter().enumerate() {
                w.0[off / 4 + i] = v.to_bits();
            }
        };
        put(0x40, &[self.mass_40, self.wheelbase_44]);
        put(
            0x58,
            &[self.drive_efficiency_58, self.front_torque_share_5c],
        );
        put(0x60, &self.gear_ratio_60);
        put(
            0x80,
            &[
                self.final_drive_80,
                self.tyre_width_84,
                self.tyre_aspect_88,
                self.rim_8c,
                self.top_speed_90,
                self.redline_94,
                self.engine_braking_98,
            ],
        );
        put(0x9c, &self.torque_9c);
        put(
            0xf0,
            &[
                self.front_weight_f0,
                self.tyre_mu_f4,
                self.cog_height_f8,
                self.steer_lock_fc,
                self.toe_100,
                self.brake_decel_104,
                self.front_brake_share_108,
                self.front_downforce_10c,
                self.rear_downforce_110,
                self.pitch_retention_114,
                self.roll_retention_118,
                self.suspension_stiffness_11c,
                self.ride_height_120,
                self.suspension_travel_124,
                self.pitch_gain_128,
                self.roll_gain_12c,
                self.lateral_split_130,
                self.max_pitch_rate_134,
                self.max_roll_rate_138,
                self.field_13c,
                self.field_140,
                self.field_144,
                self.tyre_circumference_148,
                self.inv_redline_14c,
            ],
        );
        put(0x150, &self.ratio_per_redline_150);
        put(0x170, &self.inv_ratio_170);
        put(0x190, &self.drive_factor_190);
        put(
            0x1b0,
            &[
                self.drag_1b0,
                self.yaw_arm_1b4,
                self.inv_yaw_arm_1b8,
                self.inv_mu_1bc,
            ],
        );
        w.set_i(0x48, self.flag_48);
        w.set_i(0x4c, self.engine_type_4c);
        w.set_i(0x50, self.gear_count_50);
        w.set_i(0x54, self.shift_time_54);
        w.set_i(0x1c0, self.tyre_type_1c0);
        let mut out = [0u8; CAR_SPECS_SIZE];
        for (i, v) in w.0.iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
        out
    }
}
