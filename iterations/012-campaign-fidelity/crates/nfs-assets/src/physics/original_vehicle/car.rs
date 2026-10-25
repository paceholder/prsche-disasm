//! `OriginalCar`: the original car state the V5 step runs on.
//!
//! Field names keep the car struct offsets (`docs/re/vehicle.md` "Car struct
//! fields" and `collision.md` in the workspace root). Wheel records are
//! `car+0x7b8 + i*0xc4`. The V2..V4 kernels take their own field views
//! ([`TyreCar`], [`EngineCar`], [`ControlsCar`]); this struct holds each
//! field once and builds those views for every call.
//!
//! Wheel order: wheel 0 front left, 1 front right, 2 rear left, 3 rear right,
//! where "right" is the body axis `R` (`+0x364`, the driver's right). The
//! body point setup `0x49abc0` sorts the model's points by height and puts
//! the four lowest in this quadrant order (`−x +z`, `+x +z`, `−x −z`,
//! `+x −z`), and wheel `i` uses body point `i`; with it the contact normal
//! of `0x499a70` points up (V6). Wheels 0,1 are front (`w+0xc0`). Positive
//! steer input (`(right − left)` in the input word) turns the car towards
//! `R`, and positive yaw rate `ω.y` (`+0x38c`, turns/s) rotates `F` towards
//! `R`.
//!
//! Globals that the original keeps process-wide (scheduler step, throttle,
//! contact flags, options) live in [`OriginalGlobals`]; the random
//! generator is passed separately.

use super::controls::{ControlsCar, ControlsEnv};
use super::engine::{EngineCar, EngineEnv};
use super::sim::CarSpecs;
use super::tyre::{TyreCar, TyreEnv, WheelRecord};
use crate::physics::original_lcg::OriginalLcg;

/// Body-axis rows `R`, `U`, `F` (`+0x364`, `+0x370`, `+0x37c`).
pub type Axes = [[f32; 3]; 3];

/// One wheel record (`car+0x7b8 + i*0xc4`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OriginalWheel {
    /// `w+0x34`: ground query point (body point `+0x1dc[i]` raised by
    /// `sim+0x120·U.y`), written by `0x499a70`.
    pub query_34: [f32; 3],
    /// `w+0x40`: contact point under the wheel (`0x499a70`), moved with the
    /// body at 64 Hz.
    pub point_40: [f32; 3],
    /// `w+0x4c`: support normal at the contact point.
    pub normal_4c: [f32; 3],
    /// `w+0x58..0x60`: gap along the body axes `(R.y, U.y, F.y)·gap` times
    /// the overtravel multiplier; `.y` plus the slide term is the
    /// compression the pitch/roll model uses.
    pub disp_58: [f32; 3],
    /// `w+0x64`: contact y − query y.
    pub gap_64: f32,
    /// `w+0x68`: suspension compression along U, clamped to the travel
    /// (`0x499a70` first loop, `0x49aec0`).
    pub compression_68: f32,
    /// `w+0x6c`: `gap·U.y/U.y` on the ground, `0.8·gap·U.y` in the air.
    pub load_6c: f32,
    /// `w+0x70`: suspension arm angle (asin), negated on wheels 1, 3.
    pub angle_70: f32,
    /// `w+0x74/0x78/0x7c`: material height noise previous / current /
    /// phase (`0x499730`).
    pub noise_74: [f32; 3],
    /// `w+0x84`: wheel off the ground (both wheels of an axle and height
    /// `+0x424 > 0.05` make the axle airborne).
    pub airborne_84: i32,
    /// `w+0x88`: spin phase, turns (cosmetic).
    pub spin_88: f32,
    /// `w+0xa0`: cancel accel along the ground normal (`−16·d5c`).
    pub cancel_normal_a0: f32,
    /// The V2 fields (`w+0x80`, `+0x8c..+0xc0`).
    pub tyre: WheelRecord,
}

/// Process globals read and written by the vehicle step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OriginalGlobals {
    /// `g_SchedStep [0x606ac4]`: 64 Hz scheduler step.
    pub sched_step_606ac4: i32,
    /// `g_Throttle [0x628fb0]`, `g_Brake [0x628fac]`, `g_SteerAbs
    /// [0x628fb4]`.
    pub throttle_628fb0: f32,
    pub brake_628fac: f32,
    pub steer_abs_628fb4: f32,
    /// `g_FrontContact [0x628f9c]` / `g_RearContact [0x628f98]`.
    pub front_contact_628f9c: i32,
    pub rear_contact_628f98: i32,
    /// `[0x628fa0..0x628fa8]`: gravity `(0, −9.8, 0)` in the ground frame.
    pub gravity_local_628fa0: [f32; 3],
    /// `g_HandlingModeFlag [0x6573f8]`.
    pub handling_mode_6573f8: i32,
    /// `[0x658098 + player·0x580]`: 1 = automatic.
    pub auto_box_658098: i32,
    /// `[0x5e67b0]`: AI handbrake turn.
    pub ai_handbrake_turn_5e67b0: i32,
    /// `[0x657404]`: split screen (gas/brake divisor 240).
    pub split_657404: i32,
    /// `[0x65742c]`: 1 mirrors the steering.
    pub mirror_65742c: i32,
    /// `g_TrackIsAutobahn [0x606ae0]` and the spatial query `0x473330`
    /// result used with it (0 selects the alternate grip table).
    pub autobahn_606ae0: i32,
    pub track_query_473330: i16,
    /// `[0x6573e8]` game mode and `[0x657408]`: mode 3 with `0x657408` set
    /// skips the slope gravity.
    pub mode_6573e8: i32,
    pub pair_damp_657408: i32,
    /// `[0x657a2c]`: enables the flip event of `0x49bd40` (writer unknown).
    pub flip_events_657a2c: i32,
}

/// Results of the damage helpers for a car without installed parts (Quick Race, A30).
/// `Car_DamageEngine` `0x4938a0` starts from zone 1 (`car+0x4e8`) and divides by
/// 1 + the number of kind-1 parts, so with no parts it returns the zone 1 damage;
/// the other helpers sum over parts only and return 0 (docs/re/damage.md).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamageHelpers {
    /// `Car_DamageBrakes` `0x493a10`.
    pub brakes: f64,
    /// `Car_DamageEngine` `0x4938a0` / `Car_DamageGearbox` `0x493aa0`.
    pub engine: f64,
    pub gearbox: f64,
    /// `Car_DamageTyres` `0x493980` / `Car_DamageAero` `0x493b30`.
    pub tyres: f64,
    pub aero: f64,
    /// `Car_AlignmentNoise` `0x493bc0` (mean of the suspension parts'
    /// modifiers; scales the steering wobble).
    pub alignment: f64,
}

impl DamageHelpers {
    /// The helpers of a car with no parts, from its zone damage `car+0x4e4`.
    pub fn no_parts(zones: &[f32; 10]) -> Self {
        Self { engine: f64::from(zones[1]), ..Self::default() }
    }
}

/// The original car state used by the planar step and the integration.
/// Car fields of the track locate step (`Car_TrackLocate` `0x49af70`,
/// `0x417190`, `0x499520`); docs/re/support.md section 4.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrackFields {
    /// `+0x78`: distance to the nearest race car (`max + 0.25·min` of
    /// |dx|, |dz|, at most 1000), 0 for race participants (`0x417190`).
    pub dist_78: f32,
    /// `+0x7e`: cleared by `0x417190` (set by `Car_PlaceOnNode`).
    pub flag_7e: u8,
    /// Node frame lateral `+0x3b8`, up `+0x3c4`, forward `+0x3d0` (`0x499520`).
    pub node_lat_3b8: [f32; 3],
    pub node_up_3c4: [f32; 3],
    pub node_fwd_3d0: [f32; 3],
    /// `+0x3dc = +0x3e8 × +0x3f4` (`0x499520`).
    pub alt_side_3dc: [f32; 3],
    /// `+0x3f4`: the body polygon's edge direction (owner `+0x18`).
    pub alt_tangent_3f4: [f32; 3],
    /// `+0x414`: heading against the node, radians in (−π, π].
    pub heading_414: f32,
    /// `+0x460`: the body polygon's whole flags word.
    pub surface_460: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OriginalCar {
    /// carSpecs (`car+0x760`).
    pub specs: CarSpecs,
    /// Resource flags (`res = car+0x558`): `+0x14` steering rate limit,
    /// `+0x20` rear-stability branch, `+0x24` lock/ABS branch, `+0x518`
    /// light mode (yaw at 1/32).
    pub res_14: i32,
    pub res_20: i32,
    pub res_24: i32,
    pub res_518: i32,
    pub damage: DamageHelpers,

    /// `car+0x2c`: flags word of the body support owner (`car+8`); its low
    /// nibble picks the bounce of a hard landing.
    pub body_surface_2c: u16,
    /// Remote/replay input source fields (`dac != 0`).
    pub remote_steer_38: i8,
    pub remote_brake_3a: u8,
    pub remote_bits_3b: u8,
    /// `+0x7c`: enables `Car_GroundContactTick` (with `+0x7f`).
    pub ground_on_7c: u8,
    /// `+0x7d` frozen (no integration) / `+0x7f` active.
    pub frozen_7d: u8,
    pub active_7f: u8,
    /// `+0x80`: integration calls until the next re-orthonormalisation.
    pub reortho_80: u8,
    /// `+0x84`: written by the grip (`0x4a1900`); scales the gravity step of
    /// `0x49bd40`.
    pub field_84: f32,
    /// `+0x88`: scheduler step of the last ground tick.
    pub ground_step_88: i32,
    /// Body points (`+0x8c`, count `+0x32c`) and their world positions
    /// (`+0x1dc`), from `Car_UpdateWorldPoints` `0x49ae30`.
    pub body_points_8c: Vec<[f32; 3]>,
    pub world_points_1dc: Vec<[f32; 3]>,

    pub position_330: [f32; 3],
    pub velocity_33c: [f32; 3],
    /// `+0x348`: unit velocity (forward axis at rest), `0x49bd40`.
    pub vel_dir_348: [f32; 3],
    /// Speed (`max + 0.25·min` of `|v.x|`, `|v.z|`, from the ground step).
    pub speed_35c: f32,
    pub axes_364: Axes,
    /// Angular velocity, turns/s.
    pub angular_388: [f32; 3],
    /// Body right / forward · ground normal (`+0x39c`, `+0x3a4`).
    pub ground_right_39c: f32,
    /// Ground normal · up; ≥ 0.5 enables the tyres.
    pub ground_up_3a0: f32,
    pub ground_fwd_3a4: f32,
    /// Collision box half extents (`+0x3a8`); x and y lift the car on one
    /// side in `0x499a70`.
    pub half_extents_3a8: [f32; 3],
    /// Alternate support plane: normal `+0x3e8`, point `+0x400`, written by
    /// `0x499520` from the body support owner `car+8` ([`super::locate`]).
    pub alt_normal_3e8: [f32; 3],
    pub alt_base_400: [f32; 3],
    /// The other fields `Car_TrackLocate` `0x49af70` writes.
    pub track: TrackFields,
    /// `+0x40c`: filtered change of `+0x420`.
    pub ground_ay_40c: f32,
    /// `+0x418`: two or more wheels off the ground.
    pub wheels_off_418: u16,
    /// `+0x41c`: mean contact height; `+0x420`: its rate (m/s).
    pub ground_41c: f32,
    pub ground_vy_420: f32,
    /// Height above the ground (`0x49a900`).
    pub height_424: f32,
    /// `+0x428`: second height measure of `0x49a900` (gates the gravity).
    pub height_428: f32,
    /// Track surface (3 adds the high-speed damping).
    pub surface_464: i32,
    /// Ground frame rows right / normal / forward (`+0x490`, `+0x49c`,
    /// `+0x4a8`).
    pub ground_490: Axes,
    /// Damage zones `+0x4e4..` (`+0x4e4`/`+0x4ec` pull the steering).
    pub damage_4e4: [f32; 10],
    /// `+0x504`: flip event timer (`0x49bd40` rollover branch).
    pub flip_504: f32,
    pub player_520: i32,
    pub flags_52c: u8,
    /// Second byte of `+0x52c` (bit 2 = 0x400 keeps `+0x418` on landing).
    pub flags_52d: u8,
    pub field_540: i32,
    pub field_544: i32,
    /// Rev-limiter cut steps / boost.
    pub limiter_548: i32,
    pub boost_54c: f32,
    /// Force feedback `+0x55c..+0x564`.
    pub feedback_55c: [f32; 3],
    pub wheels: [OriginalWheel; 4],
    /// `+0xac8/+0xacc`: 50/50 smoothed front steer (rendering).
    pub steer_smooth_ac8: [f32; 2],
    /// `+0xad0`: closing speed of the last hard landing.
    pub landing_ad0: f32,
    pub field_b50: i32,
    pub field_d2c: i32,
    /// World angular / linear accel of the last step (`+0xd34`, `+0xd40`).
    pub angular_accel_d34: [f32; 3],
    pub accel_d40: [f32; 3],
    /// Accel sums in the ground frame: lateral `d4c`, normal `d50`,
    /// longitudinal `d54`.
    pub accel_d4c: [f32; 3],
    /// Velocity in the ground frame: lateral `d58`, normal `d5c`, forward
    /// `d60`.
    pub vel_ground_d58: [f32; 3],
    /// Angular velocity in the ground frame (`d64..d6c`).
    pub angular_ground_d64: [f32; 3],

    // Controls and gearbox (`docs/re/vehicle.md` sections 2, 3).
    pub gas_d7c: u8,
    pub brake_d7d: u8,
    pub gear_req_d7e: i8,
    pub gas_eff_d7f: u8,
    pub brake_eff_d80: u8,
    pub prev_gear_d81: i8,
    pub gear_d82: i8,
    pub downshift_d83: u8,
    pub shift_timer_d84: i8,
    pub handbrake_d85: u8,
    pub horn_d86: u8,
    pub toggles_d87: u8,
    pub word0_d8a: u8,
    pub event_d8b: u8,
    pub event_d8c: u8,
    pub steer_in_d90: i32,
    pub steer_d94: i32,
    pub reverse_d98: i32,
    pub ai_gas_d9c: u8,
    pub ai_brake_d9d: u8,
    pub ai_steer_da0: i32,
    pub ai_da8: i32,
    pub remote_dac: i32,
    pub start_db0: i32,
    pub rpm_db4: f32,
    pub spin_rear_db8: i32,
    pub spin_front_dbc: i32,
    pub slip_ratio_dc0: f32,
    /// Pitch / roll accel filters (V6; decayed by the airborne branch).
    pub pitch_filter_dc4: f32,
    pub roll_filter_dc8: f32,
    /// Filtered longitudinal / lateral accel.
    pub filter_e04: f32,
    pub filter_e08: f32,
    pub ai_side_eb0: i32,
    pub ai_side_ed0: f32,
    /// Effect events queued by `0x40f020` (`+0x110c` count, up to 7
    /// records of 6 dwords at `+0x1110`): landing, flip. Nothing in the
    /// ported code consumes them.
    pub events_1110: Vec<[u32; 6]>,
    pub lights_1230: u16,
    pub lights_1232: u16,

    pub globals: OriginalGlobals,
}

impl OriginalCar {
    /// A car at rest at the origin: identity axes and ground frame, upright
    /// on the ground (`+0x3a0 = 1`), front flags on wheels 0,1 (as the
    /// reset `0x49e220`), active with the ground tick on (`+0x7c`),
    /// re-orthonormalisation counter 0x20, gear N, body surface 1, the
    /// alternate plane `y = 0`, velocity direction `+z`. Everything else is
    /// zero.
    pub fn new(specs: CarSpecs) -> Self {
        const IDENTITY: Axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let mut wheels = [OriginalWheel::default(); 4];
        for (i, w) in wheels.iter_mut().enumerate() {
            w.tyre.is_front_c0 = (i < 2) as i32;
        }
        Self {
            specs,
            res_14: 0,
            res_20: 0,
            res_24: 0,
            res_518: 0,
            damage: DamageHelpers::default(),
            body_surface_2c: 1,
            remote_steer_38: 0,
            remote_brake_3a: 0,
            remote_bits_3b: 0,
            ground_on_7c: 1,
            frozen_7d: 0,
            active_7f: 1,
            reortho_80: 0x20,
            field_84: 0.0,
            ground_step_88: 0,
            body_points_8c: Vec::new(),
            world_points_1dc: Vec::new(),
            position_330: [0.0; 3],
            velocity_33c: [0.0; 3],
            vel_dir_348: [0.0, 0.0, 1.0],
            speed_35c: 0.0,
            axes_364: IDENTITY,
            angular_388: [0.0; 3],
            ground_right_39c: 0.0,
            ground_up_3a0: 1.0,
            ground_fwd_3a4: 0.0,
            half_extents_3a8: [0.0; 3],
            alt_normal_3e8: [0.0, 1.0, 0.0],
            alt_base_400: [0.0; 3],
            track: TrackFields::default(),
            ground_ay_40c: 0.0,
            wheels_off_418: 0,
            ground_41c: 0.0,
            ground_vy_420: 0.0,
            height_424: 0.0,
            height_428: 0.0,
            surface_464: 0,
            ground_490: IDENTITY,
            damage_4e4: [0.0; 10],
            flip_504: 0.0,
            player_520: 0,
            flags_52c: 0,
            flags_52d: 0,
            field_540: 0,
            field_544: 0,
            limiter_548: 0,
            boost_54c: 0.0,
            feedback_55c: [0.0; 3],
            wheels,
            steer_smooth_ac8: [0.0; 2],
            landing_ad0: 0.0,
            field_b50: 0,
            field_d2c: 0,
            angular_accel_d34: [0.0; 3],
            accel_d40: [0.0; 3],
            accel_d4c: [0.0; 3],
            vel_ground_d58: [0.0; 3],
            angular_ground_d64: [0.0; 3],
            gas_d7c: 0,
            brake_d7d: 0,
            gear_req_d7e: 1,
            gas_eff_d7f: 0,
            brake_eff_d80: 0,
            prev_gear_d81: 1,
            gear_d82: 1,
            downshift_d83: 0,
            shift_timer_d84: 0,
            handbrake_d85: 0,
            horn_d86: 0,
            toggles_d87: 0,
            word0_d8a: 0,
            event_d8b: 0,
            event_d8c: 0,
            steer_in_d90: 0,
            steer_d94: 0,
            reverse_d98: 0,
            ai_gas_d9c: 0,
            ai_brake_d9d: 0,
            ai_steer_da0: 0,
            ai_da8: 0,
            remote_dac: 0,
            start_db0: 0,
            rpm_db4: 0.0,
            spin_rear_db8: 0,
            spin_front_dbc: 0,
            slip_ratio_dc0: 0.0,
            pitch_filter_dc4: 0.0,
            roll_filter_dc8: 0.0,
            filter_e04: 0.0,
            filter_e08: 0.0,
            ai_side_eb0: 0,
            ai_side_ed0: 0.0,
            events_1110: Vec::new(),
            lights_1230: 0,
            lights_1232: 0,
            globals: OriginalGlobals::default(),
        }
    }

    /// Forward velocity in the ground frame (`d60`).
    pub fn vel_fwd_d60(&self) -> f32 {
        self.vel_ground_d58[2]
    }

    fn tyres(&self) -> [WheelRecord; 4] {
        self.wheels.map(|w| w.tyre)
    }

    fn set_tyres(&mut self, t: [WheelRecord; 4]) {
        for (w, t) in self.wheels.iter_mut().zip(t) {
            w.tyre = t;
        }
    }

    // -- V4 view ------------------------------------------------------------
    pub(crate) fn controls_view(&self, input_word: u32) -> (ControlsCar, ControlsEnv) {
        let g = &self.globals;
        let car = ControlsCar {
            remote_steer_38: self.remote_steer_38,
            remote_brake_3a: self.remote_brake_3a,
            remote_bits_3b: self.remote_bits_3b,
            speed_35c: self.speed_35c,
            player_520: self.player_520,
            flags_52c: self.flags_52c,
            field_540: self.field_540,
            field_544: self.field_544,
            field_b50: self.field_b50,
            field_d2c: self.field_d2c,
            vel_fwd_d60: self.vel_fwd_d60(),
            gas_d7c: self.gas_d7c,
            brake_d7d: self.brake_d7d,
            gear_req_d7e: self.gear_req_d7e,
            gas_eff_d7f: self.gas_eff_d7f,
            brake_eff_d80: self.brake_eff_d80,
            prev_gear_d81: self.prev_gear_d81,
            gear_d82: self.gear_d82,
            downshift_d83: self.downshift_d83,
            shift_timer_d84: self.shift_timer_d84,
            handbrake_d85: self.handbrake_d85,
            horn_d86: self.horn_d86,
            toggles_d87: self.toggles_d87,
            word0_d8a: self.word0_d8a,
            event_d8b: self.event_d8b,
            event_d8c: self.event_d8c,
            steer_in_d90: self.steer_in_d90,
            steer_d94: self.steer_d94,
            reverse_d98: self.reverse_d98,
            ai_gas_d9c: self.ai_gas_d9c,
            ai_brake_d9d: self.ai_brake_d9d,
            ai_steer_da0: self.ai_steer_da0,
            ai_da8: self.ai_da8,
            remote_dac: self.remote_dac,
            start_db0: self.start_db0,
            rpm_db4: self.rpm_db4,
            ai_side_eb0: self.ai_side_eb0,
            ai_side_ed0: self.ai_side_ed0,
            lights_1230: self.lights_1230,
            lights_1232: self.lights_1232,
        };
        let env = ControlsEnv {
            input_word_5e9664: input_word,
            mirror_65742c: g.mirror_65742c,
            throttle_628fb0: g.throttle_628fb0,
            brake_628fac: g.brake_628fac,
            steer_abs_628fb4: g.steer_abs_628fb4,
            sched_step_606ac4: g.sched_step_606ac4,
            auto_box_658098: g.auto_box_658098,
            handling_mode_6573f8: g.handling_mode_6573f8,
            ai_handbrake_turn_5e67b0: g.ai_handbrake_turn_5e67b0,
            split_657404: g.split_657404,
            res_steer_rate_14: self.res_14,
            res_light_mode_518: self.res_518,
        };
        (car, env)
    }

    /// Writes back every field of the V4 view.
    pub(crate) fn store_controls(&mut self, c: &ControlsCar, env: &ControlsEnv) {
        self.remote_steer_38 = c.remote_steer_38;
        self.remote_brake_3a = c.remote_brake_3a;
        self.remote_bits_3b = c.remote_bits_3b;
        self.speed_35c = c.speed_35c;
        self.player_520 = c.player_520;
        self.flags_52c = c.flags_52c;
        self.field_540 = c.field_540;
        self.field_544 = c.field_544;
        self.field_b50 = c.field_b50;
        self.field_d2c = c.field_d2c;
        self.vel_ground_d58[2] = c.vel_fwd_d60;
        self.gas_d7c = c.gas_d7c;
        self.brake_d7d = c.brake_d7d;
        self.gear_req_d7e = c.gear_req_d7e;
        self.gas_eff_d7f = c.gas_eff_d7f;
        self.brake_eff_d80 = c.brake_eff_d80;
        self.prev_gear_d81 = c.prev_gear_d81;
        self.gear_d82 = c.gear_d82;
        self.downshift_d83 = c.downshift_d83;
        self.shift_timer_d84 = c.shift_timer_d84;
        self.handbrake_d85 = c.handbrake_d85;
        self.horn_d86 = c.horn_d86;
        self.toggles_d87 = c.toggles_d87;
        self.word0_d8a = c.word0_d8a;
        self.event_d8b = c.event_d8b;
        self.event_d8c = c.event_d8c;
        self.steer_in_d90 = c.steer_in_d90;
        self.steer_d94 = c.steer_d94;
        self.reverse_d98 = c.reverse_d98;
        self.ai_gas_d9c = c.ai_gas_d9c;
        self.ai_brake_d9d = c.ai_brake_d9d;
        self.ai_steer_da0 = c.ai_steer_da0;
        self.ai_da8 = c.ai_da8;
        self.remote_dac = c.remote_dac;
        self.start_db0 = c.start_db0;
        self.rpm_db4 = c.rpm_db4;
        self.ai_side_eb0 = c.ai_side_eb0;
        self.ai_side_ed0 = c.ai_side_ed0;
        self.lights_1230 = c.lights_1230;
        self.lights_1232 = c.lights_1232;
        let g = &mut self.globals;
        g.mirror_65742c = env.mirror_65742c;
        g.throttle_628fb0 = env.throttle_628fb0;
        g.brake_628fac = env.brake_628fac;
        g.steer_abs_628fb4 = env.steer_abs_628fb4;
    }

    // -- V3 view ------------------------------------------------------------
    pub(crate) fn engine_view(&self, rand: OriginalLcg) -> (EngineCar, EngineEnv) {
        let g = &self.globals;
        let car = EngineCar {
            speed_35c: self.speed_35c,
            flags_52c: self.flags_52c,
            limiter_548: self.limiter_548,
            boost_54c: self.boost_54c,
            vel_fwd_d60: self.vel_fwd_d60(),
            gas_d7c: self.gas_d7c,
            gear_req_d7e: self.gear_req_d7e,
            brake_d80: self.brake_eff_d80,
            prev_gear_d81: self.prev_gear_d81,
            gear_d82: self.gear_d82,
            downshift_d83: self.downshift_d83,
            shift_timer_d84: self.shift_timer_d84,
            ai_da8: self.ai_da8,
            rpm_db4: self.rpm_db4,
            spin_rear_db8: self.spin_rear_db8,
            spin_front_dbc: self.spin_front_dbc,
            slip_ratio_dc0: self.slip_ratio_dc0,
            grip_ac: self.wheels.map(|w| w.tyre.grip_ac),
        };
        let env = EngineEnv {
            throttle_628fb0: g.throttle_628fb0,
            rear_contact_628f98: g.rear_contact_628f98,
            sched_step_606ac4: g.sched_step_606ac4,
            handling_mode_6573f8: g.handling_mode_6573f8,
            auto_box_658098: g.auto_box_658098,
            rand,
            damage_engine: self.damage.engine,
            damage_gearbox: self.damage.gearbox,
        };
        (car, env)
    }

    /// Writes back every field of the V3 view.
    pub(crate) fn store_engine(&mut self, e: &EngineCar, env: &EngineEnv) {
        self.speed_35c = e.speed_35c;
        self.flags_52c = e.flags_52c;
        self.limiter_548 = e.limiter_548;
        self.boost_54c = e.boost_54c;
        self.vel_ground_d58[2] = e.vel_fwd_d60;
        self.gas_d7c = e.gas_d7c;
        self.gear_req_d7e = e.gear_req_d7e;
        self.brake_eff_d80 = e.brake_d80;
        self.prev_gear_d81 = e.prev_gear_d81;
        self.gear_d82 = e.gear_d82;
        self.downshift_d83 = e.downshift_d83;
        self.shift_timer_d84 = e.shift_timer_d84;
        self.ai_da8 = e.ai_da8;
        self.rpm_db4 = e.rpm_db4;
        self.spin_rear_db8 = e.spin_rear_db8;
        self.spin_front_dbc = e.spin_front_dbc;
        self.slip_ratio_dc0 = e.slip_ratio_dc0;
        for (w, g) in self.wheels.iter_mut().zip(e.grip_ac) {
            w.tyre.grip_ac = g;
        }
        self.globals.throttle_628fb0 = env.throttle_628fb0;
        self.globals.rear_contact_628f98 = env.rear_contact_628f98;
    }

    // -- V2 view ------------------------------------------------------------
    pub(crate) fn tyre_view(&self) -> (TyreCar, TyreEnv, [WheelRecord; 4]) {
        let g = &self.globals;
        let car = TyreCar {
            field_84: self.field_84,
            ground_up_3a0: self.ground_up_3a0,
            ground_normal_y_4a0: self.ground_490[1][1],
            flags_52c: self.flags_52c,
            accel_lat_d4c: self.accel_d4c[0],
            accel_long_d54: self.accel_d4c[2],
            vel_fwd_d60: self.vel_fwd_d60(),
            gas_d7f: self.gas_eff_d7f,
            brake_d80: self.brake_eff_d80,
            gear_d82: self.gear_d82,
            handbrake_d85: self.handbrake_d85,
            ai_da8: self.ai_da8,
            spin_rear_db8: self.spin_rear_db8,
            spin_front_dbc: self.spin_front_dbc,
            slip_ratio_dc0: self.slip_ratio_dc0,
            res_20: self.res_20,
            res_24: self.res_24,
        };
        let env = TyreEnv {
            throttle_628fb0: g.throttle_628fb0,
            sched_step_606ac4: g.sched_step_606ac4,
            handling_mode_6573f8: g.handling_mode_6573f8,
            autobahn_606ae0: g.autobahn_606ae0,
            track_query_473330: g.track_query_473330,
            damage_tyres: self.damage.tyres,
            damage_aero: self.damage.aero,
        };
        (car, env, self.tyres())
    }

    pub(crate) fn store_tyres(&mut self, car: &TyreCar, wheels: [WheelRecord; 4]) {
        self.field_84 = car.field_84;
        self.set_tyres(wheels);
    }
}

#[cfg(test)]
mod damage_tests {
    use super::DamageHelpers;

    #[test]
    fn no_parts_engine_damage_is_zone_1() {
        // Car_DamageEngine 0x4938a0: (zone1 + sum over kind-1 parts) / (1 + parts).
        let mut zones = [0.0f32; 10];
        assert_eq!(DamageHelpers::no_parts(&zones), DamageHelpers::default());
        zones[1] = 40.0;
        zones[0] = 90.0;
        let d = DamageHelpers::no_parts(&zones);
        assert_eq!(d.engine, 40.0);
        assert_eq!((d.gearbox, d.brakes, d.tyres, d.aero, d.alignment), (0.0, 0.0, 0.0, 0.0, 0.0));
    }
}
