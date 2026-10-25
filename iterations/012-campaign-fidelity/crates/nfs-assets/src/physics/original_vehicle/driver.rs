//! Engine integration of the original vehicle model (task 6.2): the
//! `vehicle_model` switch, the fixed 64 Hz step clock, the keyboard-to-input
//! word mapping, the collision view of [`OriginalCar`], and
//! [`OriginalVehicle`], the player car runtime the renderer drives.
//!
//! The step order itself is [`scheduler_step`]; the renderer supplies the
//! ground (`Car_GroundContactTick` through the [`GroundAdapter`] on its road
//! query), the collision pass and the camera through [`StepQueue`].

use super::car::{Axes, OriginalCar};
use super::ground::{GroundAdapter, GroundQuery};
use super::sim::{derive, CarSpecs, Tuning, SIM_FILE_SIZE};
use super::step::{scheduler_step, update_world_points, StepQueue};
use crate::physics::original_contact::PreparedContactState;
use crate::physics::original_lcg::OriginalLcg;
use crate::physics::original_pair::{PairCar, PairGlobals};
use crate::physics::original_support_owner::OriginalSupportOwner;
use crate::race::original_collision::collide_pass;

/// Which vehicle model drives the player car.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VehicleModel {
    /// Upstream's 6 DOF placeholder model (`physics::vehicle`).
    #[default]
    Prototype,
    /// The recovered original model ([`OriginalCar`]).
    Original,
}

/// Scheduler rate (`Sched_Step` `0x414ff0`).
pub const STEP_HZ: f64 = 64.0;
/// Longest frame the clock catches up on (as the camera clock).
pub const MAX_FRAME_S: f64 = 0.25;

/// Fixed-step clock: frame time to whole 64 Hz scheduler steps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StepClock {
    pending: f64,
}

impl StepClock {
    /// Advance by `dt` seconds (clamped to [`MAX_FRAME_S`]); returns the
    /// number of steps due.
    pub fn advance(&mut self, dt: f32) -> u32 {
        let step = 1.0 / STEP_HZ;
        self.pending += (dt as f64).clamp(0.0, MAX_FRAME_S);
        let mut steps = 0;
        while self.pending >= step {
            self.pending -= step;
            steps += 1;
        }
        steps
    }

    /// Fraction of the next step already elapsed (0..1), the render interpolation alpha.
    pub fn fraction(&self) -> f32 {
        (self.pending * STEP_HZ).clamp(0.0, 1.0) as f32
    }
}

/// Input word `[0x5e9664]` from the engine's controls (`Input_PollControllers`
/// `0x412e70` packing, `docs/re/vehicle.md` section 2): byte 0 bit 0
/// handbrake, byte 1 steer (`(right − left)/2`, ±127, positive = right),
/// byte 2 gas/2, byte 3 brake/2 (0..127). Pedals and steer are clamped to
/// 0..1 and −1..1; a digital key gives full scale. No gear events (the box
/// is automatic).
pub fn input_word(gas: f32, brake: f32, steer: f32, handbrake: bool) -> u32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 127.0).round() as u32;
    let steer = ((steer.clamp(-1.0, 1.0) * 127.0).round() as i32 as i8) as u8 as u32;
    handbrake as u32 | steer << 8 | byte(gas) << 16 | byte(brake) << 24
}

/// Collision view of the car (`Car_CollideTick` `0x498ff0` works on the car
/// struct itself; [`PairCar`] holds the fields it reads). The box centre is
/// `+0x330`, the mass `sim+0x40 / 128` [VA 0x410730], `I = 6m` [VA 0x49b510].
/// Angular velocity stays in turns/s.
pub fn pair_car(car: &OriginalCar, half_extents: [f32; 3]) -> PairCar {
    let mass = car.specs.mass_40 / 128.0;
    let inertia = 6.0 * mass;
    let [r, u, f] = car.axes_364;
    let mut damage = [0.0; 9];
    damage.copy_from_slice(&car.damage_4e4[..9]);
    PairCar {
        position_330: car.position_330,
        velocity_33c: car.velocity_33c,
        mass_354: mass,
        inv_mass_358: 1.0 / mass,
        speed_35c: car.speed_35c,
        axis_r_364: r,
        axis_u_370: u,
        axis_f_37c: f,
        angular_388: car.angular_388,
        inertia_394: inertia,
        inv_inertia_398: 1.0 / inertia,
        extents_3a8: half_extents,
        radius_3b4: (half_extents[0] * half_extents[0]
            + half_extents[1] * half_extents[1]
            + half_extents[2] * half_extents[2])
            .sqrt(),
        active_7c: true,
        field_424: car.height_424,
        flags_52c: car.flags_52c as u32,
        resource_518: car.res_518 != 0,
        field_b50: car.field_b50,
        local_angular_d64: car.angular_ground_d64,
        field_dac: car.remote_dac,
        field_db0: car.start_db0,
        damage_4e4: damage,
        ..PairCar::default()
    }
}

/// Writes a collision result back into the car (the fields the pair and
/// barrier responses change).
pub fn store_pair_car(car: &mut OriginalCar, pair: &PairCar) {
    car.position_330 = pair.position_330;
    car.velocity_33c = pair.velocity_33c;
    car.speed_35c = pair.speed_35c;
    car.angular_388 = pair.angular_388;
    car.angular_ground_d64 = pair.local_angular_d64;
}

/// Barrier-response state (`Car_CollideTrackEdges` `0x495020`) of the car
/// while its collision view is `pair`: the ground frame, wheel points
/// `w+0x40`, skid `w+0xb8` (`+0x870`), `+0xdb8`, `+0xdc0` come from the car
/// (no neutral values as in the 6 DOF adapter A5). `+0x434` is not modelled
/// (0).
pub fn barrier_state(car: &OriginalCar, pair: &PairCar) -> PreparedContactState {
    PreparedContactState {
        position_330: pair.position_330,
        velocity_33c: pair.velocity_33c,
        speed_35c: pair.speed_35c,
        basis_364: pair.axis_r_364,
        basis_370: pair.axis_u_370,
        basis_37c: pair.axis_f_37c,
        field_38c: pair.angular_388[1],
        field_434: 0.0,
        basis_490: car.ground_490[0],
        basis_4a8: car.ground_490[2],
        local_velocity_d58: car.vel_ground_d58,
        wheel_points_7f8: car.wheels.map(|w| w.point_40),
        wheel_fields_870: car.wheels.map(|w| w.tyre.skid_b8),
        field_db8: car.spin_rear_db8 as f32,
        field_dc0: car.slip_ratio_dc0,
    }
}

/// Writes a barrier response back: into the collision view (position,
/// velocity, speed, yaw rate) and into the car (local velocity, wheel points
/// and the wheel fields it may clear).
pub fn store_barrier_state(car: &mut OriginalCar, pair: &mut PairCar, s: &PreparedContactState) {
    pair.position_330 = s.position_330;
    pair.velocity_33c = s.velocity_33c;
    pair.speed_35c = s.speed_35c;
    pair.angular_388[1] = s.field_38c;
    car.vel_ground_d58 = s.local_velocity_d58;
    for (w, (p, skid)) in car
        .wheels
        .iter_mut()
        .zip(s.wheel_points_7f8.iter().zip(s.wheel_fields_870))
    {
        w.point_40 = *p;
        w.tyre.skid_b8 = skid;
    }
    car.spin_rear_db8 = s.field_db8 as i32;
    car.slip_ratio_dc0 = s.field_dc0;
}

/// One `Car_CollideTick` pass with the player first (`collide_pass`): the
/// player's barrier response through `barrier` (return `false` for no
/// contact), the other cars (`others`, kinematic AI, A6) without one. The
/// pair response draws from the shared random generator.
pub fn collide_player(
    car: &mut OriginalCar,
    half_extents: [f32; 3],
    others: &mut [PairCar],
    globals: &mut PairGlobals,
    lcg: &mut OriginalLcg,
    mut barrier: impl FnMut(&mut PreparedContactState) -> bool,
) -> usize {
    let mut cars = Vec::with_capacity(others.len() + 1);
    cars.push(pair_car(car, half_extents));
    cars.extend_from_slice(others);
    globals.rand_state_5d1028 = lcg.state_5d1028;
    globals.rand_mul_5d102c = lcg.multiplier_5d102c;
    let pairs = collide_pass(&mut cars, globals, |index, pair| {
        if index != 0 {
            return;
        }
        let mut state = barrier_state(car, pair);
        if barrier(&mut state) {
            store_barrier_state(car, pair, &state);
        }
    });
    lcg.state_5d1028 = globals.rand_state_5d1028;
    store_pair_car(car, &cars[0]);
    others.clone_from_slice(&cars[1..]);
    pairs
}

/// Scheduler step at the start of a race: the race-start gating of
/// `Car_ApplyControls` (`docs/re/vehicle.md` V4 results) has run its
/// counter, and the automatic box requests 1st on steps 0x1c1..0x1c3.
pub const START_SCHED_STEP: i32 = 0x1c0;
/// `+0xdb0` at [`START_SCHED_STEP`]: past the 0x40 steps that hold the brake.
pub const START_DB0: i32 = 0x40;

/// The player car on the original model: car state, random generator,
/// ground adapter state, the per-wheel support owners and the step clock.
#[derive(Clone, Debug)]
pub struct OriginalVehicle {
    pub car: OriginalCar,
    pub lcg: OriginalLcg,
    pub ground: GroundAdapter,
    pub owners: [OriginalSupportOwner; 4],
    pub clock: StepClock,
    /// Collision box half extents (`+0x3a8`).
    pub half_extents: [f32; 3],
}

/// Model name (`res+0x2a8`, the AICarData `.ais` stem) for a `.sim` file
/// stem, as the V1 recorder chose it (longest prefix first; hypothesis).
pub fn model_for_sim(stem: &str) -> &'static str {
    const PREFIXES: [(&str, &str); 23] = [
        ("356A", "356A"),
        ("356B", "356B"),
        ("356", "356"),
        ("550", "550a"),
        ("901", "901"),
        ("914", "914"),
        ("928", "928"),
        ("930T", "930t"),
        ("935", "935"),
        ("944", "944"),
        ("959", "959"),
        ("964", "964"),
        ("965T", "965t"),
        ("9936", "993"),
        ("993T", "993t"),
        ("993", "993"),
        ("996T", "996t"),
        ("996", "996"),
        ("GT1", "Gt1"),
        ("GT2", "Gt2"),
        ("GT3", "Gt3"),
        ("boxster", "boxster"),
        ("default", "default"),
    ];
    let lower = stem.to_ascii_lowercase();
    PREFIXES
        .iter()
        .find(|(p, _)| lower.starts_with(&p.to_ascii_lowercase()))
        .map_or("default", |&(_, m)| m)
}

impl OriginalVehicle {
    /// Car from a `.sim` file (V1 with default tuning for `model`, no parts),
    /// its collision box half extents and its four wheel ground points
    /// relative to the box centre in the car's axes (R, U, F), wheel order
    /// FL, FR, RL, RR (the quadrant order of `0x49abc0`: `−R +F`, `+R +F`,
    /// `−R −F`, `+R −F`). Resource flags: `res+0x14 = 1` (steering rate limit,
    /// as in the V5 recordings), others 0; automatic box.
    pub fn from_sim(
        raw_sim: &[u8],
        model: &str,
        half_extents: [f32; 3],
        wheel_points: [[f32; 3]; 4],
    ) -> Result<Self, String> {
        if raw_sim.len() < SIM_FILE_SIZE {
            return Err(format!(
                "SIM is {} bytes, need {SIM_FILE_SIZE}",
                raw_sim.len()
            ));
        }
        let specs = derive(&raw_sim[..SIM_FILE_SIZE], &Tuning::for_model(model));
        if !specs_usable(&specs) {
            return Err("SIM is not usable for the original model".into());
        }
        Ok(Self::from_specs(specs, half_extents, wheel_points))
    }

    /// Car from derived carSpecs (see [`Self::from_sim`]).
    pub fn from_specs(
        specs: CarSpecs,
        half_extents: [f32; 3],
        wheel_points: [[f32; 3]; 4],
    ) -> Self {
        let mut v = Self {
            car: OriginalCar::new(specs),
            lcg: OriginalLcg::default(),
            ground: GroundAdapter::default(),
            owners: Default::default(),
            clock: StepClock::default(),
            half_extents,
        };
        v.car.body_points_8c = wheel_points.to_vec();
        v.restart();
        v
    }

    /// Back to the start state (the race start): a fresh car with the same
    /// specs and body points, at rest in neutral, the scheduler at
    /// [`START_SCHED_STEP`]. The pose is kept.
    pub fn restart(&mut self) {
        let old = &self.car;
        let mut car = OriginalCar::new(old.specs.clone());
        car.body_points_8c = old.body_points_8c.clone();
        car.half_extents_3a8 = self.half_extents;
        car.position_330 = old.position_330;
        car.axes_364 = old.axes_364;
        car.res_14 = 1;
        car.start_db0 = START_DB0;
        car.globals.sched_step_606ac4 = START_SCHED_STEP;
        car.globals.auto_box_658098 = 1;
        update_world_points(&mut car);
        self.car = car;
        self.lcg = OriginalLcg::default();
        self.ground = GroundAdapter::default();
        self.owners = Default::default();
        self.clock = StepClock::default();
    }

    /// Mid-race reset of the drive state, before a track reset moves the car
    /// (`Car_ResetToTrack 0x40f540`): `Car_ResetDriveState 0x49e220` (inputs,
    /// gear N, wheel records, rpm, force sums, `+0x540`) and `0x40f400`
    /// (angular velocity, `+0x418`, `+0x420`, `+0x40c`, speed). The velocity
    /// is cleared by `0x40f540` itself. Unlike [`Self::restart`], the
    /// scheduler step, the random generator and the race fields (`+0xb50`,
    /// damage, flags, light switches) stay. `+0xdb0` becomes 0x3c (a short
    /// brake hold) or, if a roll had cut the controls, 1 (a full one).
    /// Gear: `0x49e220` selects 1st when `res+0xc == 1`, else N. `res+0xc`
    /// is not ported; it is taken as the automatic box flag (approximation
    /// A98), so the automatic car gets 1st (in N it would wait for the
    /// race-start request, which comes only before step 0x1c4).
    pub fn reset_drive_state(&mut self) {
        let old = &self.car;
        let mut car = OriginalCar::new(old.specs.clone());
        car.res_14 = old.res_14;
        car.res_20 = old.res_20;
        car.res_24 = old.res_24;
        car.res_518 = old.res_518;
        car.damage = old.damage;
        car.body_surface_2c = old.body_surface_2c;
        car.remote_steer_38 = old.remote_steer_38;
        car.remote_brake_3a = old.remote_brake_3a;
        car.remote_bits_3b = old.remote_bits_3b;
        car.ground_on_7c = old.ground_on_7c;
        car.frozen_7d = old.frozen_7d;
        car.active_7f = old.active_7f;
        car.reortho_80 = old.reortho_80;
        car.body_points_8c = old.body_points_8c.clone();
        car.position_330 = old.position_330;
        car.axes_364 = old.axes_364;
        car.half_extents_3a8 = old.half_extents_3a8;
        car.alt_normal_3e8 = old.alt_normal_3e8;
        car.alt_base_400 = old.alt_base_400;
        car.damage_4e4 = old.damage_4e4;
        car.player_520 = old.player_520;
        car.flags_52c = old.flags_52c;
        car.flags_52d = old.flags_52d;
        car.field_544 = old.field_544;
        car.field_b50 = old.field_b50;
        car.toggles_d87 = old.toggles_d87;
        car.ai_da8 = old.ai_da8;
        car.remote_dac = old.remote_dac;
        car.events_1110 = old.events_1110.clone();
        car.lights_1230 = old.lights_1230;
        car.lights_1232 = old.lights_1232;
        car.globals = old.globals;
        car.start_db0 = if old.start_db0 != 0 { 0x3c } else { 1 };
        if car.globals.auto_box_658098 == 1 {
            car.gear_d82 = 2;
            car.gear_req_d7e = 2;
        }
        update_world_points(&mut car);
        self.car = car;
        self.ground = GroundAdapter::default();
        self.owners = Default::default();
    }

    /// Moves the car to a pose (box centre, axes R/U/F, velocity, angular
    /// velocity in turns/s), keeping the model state, and places it on the
    /// ground there. Used at spawn and when switching from the prototype;
    /// the caller resets the support owners ([`Self::owners`]) for a jump.
    pub fn set_pose(
        &mut self,
        position: [f32; 3],
        axes: Axes,
        velocity: [f32; 3],
        angular: [f32; 3],
        ground: &mut impl GroundQuery,
    ) {
        let car = &mut self.car;
        car.position_330 = position;
        car.axes_364 = axes;
        car.velocity_33c = velocity;
        car.angular_388 = angular;
        self.ground = GroundAdapter::default();
        self.ground.place(car, ground, &mut self.lcg);
        // Ground-frame velocities for the first step's consumers.
        let [r, n, f] = car.ground_490;
        let d = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        car.vel_ground_d58 = [d(velocity, r), d(velocity, n), d(velocity, f)];
    }

    /// Selects a gear (`+0xd82`, 0 = R, 1 = N, 2 = 1st..) with no shift in
    /// progress, and the engine rpm (model switch).
    pub fn set_gear(&mut self, gear: i8, rpm: f32) {
        let top = (self.car.specs.gear_count_50 - 1).clamp(1, 7) as i8;
        let g = gear.clamp(0, top);
        let c = &mut self.car;
        c.gear_d82 = g;
        c.gear_req_d7e = g;
        c.prev_gear_d81 = g;
        c.shift_timer_d84 = 0;
        c.downshift_d83 = 0;
        c.rpm_db4 = rpm.max(0.0);
    }

    /// Runs the scheduler steps due after `dt` seconds with the same input
    /// word (read on even steps). Returns the number of steps.
    pub fn advance(&mut self, dt: f32, input_word: u32, queue: &mut impl StepQueue) -> u32 {
        let steps = self.clock.advance(dt);
        for _ in 0..steps {
            scheduler_step(&mut self.car, input_word, &mut self.lcg, queue);
        }
        steps
    }

    /// Gear for the HUD: −1 R, 0 N, 1.. forward.
    pub fn hud_gear(&self) -> i32 {
        self.car.gear_d82 as i32 - 1
    }

    /// Suspension pose of drawn wheel `i` (FL, FR, RL, RR) as the original
    /// car draw applies it: `(offset, camber)`.
    /// - `offset`: metres along the body up. `0x43e670` (R3DCarLOD, from
    ///   `0x439f90`) sets the wheel pivot `R3DCar+0x6d0+12·i` y to the
    ///   compression `w+0x68` of physics wheel `[0x5b36f0 + 4·i]` = i
    ///   (`[0x5b3700]` = 1, 0, 3, 2 in mirror mode, not supported), and
    ///   `0x43c830` translates the wheel part by that pivot: up when
    ///   compressed, down when drooping, 0 at rest.
    /// - `camber`: turns; `0x438440` puts the arm angle `w+0x70` into the
    ///   wheel channel's z (`car+0x828 + 0xc4·i`), which `0x43c830` turns
    ///   by `ch.z·360°` about the part's forward axis.
    pub fn draw_wheel_pose(&self, wheel: usize) -> (f32, f32) {
        self.car
            .wheels
            .get(wheel)
            .map_or((0.0, 0.0), |w| (w.compression_68, w.angle_70))
    }

    /// Front wheel steer angle for drawing, in turns: `sim+0xfc·steer/17`
    /// 1/256 turn (`Wheel_TyreForce` `0x4a20d0`) of the smoothed copy
    /// `+0xac8/+0xacc`. Positive turns toward `R`.
    pub fn draw_steer_turns(&self, wheel: usize) -> f32 {
        let s = self.car.steer_smooth_ac8.get(wheel).copied().unwrap_or(0.0);
        self.car.specs.steer_lock_fc * s / 17.0 / 256.0
    }
}

/// Camera view of the car (`Camera_UpdateAll` reads `+0x330`, the axes, the
/// half extents and the ground-frame speeds `+0xd58`/`+0xd60`). No track
/// nodes: node 0, travel direction +1 (approximation A12).
pub fn camera_car(car: &OriginalCar, half_extents: [f32; 3]) -> crate::camera::CarView {
    crate::camera::CarView {
        node_08: 0,
        pos_330: car.position_330,
        axes_364: car.axes_364,
        half_extents_3a8: half_extents,
        direction_d30: 1,
        lateral_speed_d58: car.vel_ground_d58[0],
        forward_speed_d60: car.vel_ground_d58[2],
    }
}

/// Whether `spec` holds a usable SIM (finite mass, gears) for the original
/// model.
pub fn specs_usable(specs: &CarSpecs) -> bool {
    specs.mass_40.is_finite()
        && specs.mass_40 > 0.0
        && (3..=8).contains(&specs.gear_count_50)
        && specs.tyre_circumference_148.is_finite()
        && specs.tyre_circumference_148 > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::original_vehicle::step::Phase;
    use crate::physics::original_vehicle::FlatGround;

    #[test]
    fn clock_runs_64_steps_per_second() {
        for fps in [30u32, 60, 64, 144] {
            let mut clock = StepClock::default();
            let n: u32 = (0..fps).map(|_| clock.advance(1.0 / fps as f32)).sum();
            assert!((63..=64).contains(&n), "{fps} fps: {n}");
        }
        assert_eq!(StepClock::default().advance(5.0), 16);
    }

    #[test]
    fn input_word_packs_full_scale_keys() {
        assert_eq!(input_word(1.0, 0.0, 0.0, false), 127 << 16);
        assert_eq!(input_word(0.0, 1.0, 0.0, true), 127 << 24 | 1);
        assert_eq!(input_word(0.0, 0.0, 1.0, false), 127 << 8);
        assert_eq!(input_word(0.0, 0.0, -1.0, false), 0x81 << 8);
    }

    #[test]
    fn model_names_follow_the_recorder() {
        assert_eq!(model_for_sim("GT2race"), "Gt2");
        assert_eq!(model_for_sim("356Acoupe16"), "356A");
        assert_eq!(model_for_sim("356coupe11"), "356");
        assert_eq!(model_for_sim("boxster25"), "boxster");
        assert_eq!(model_for_sim("unknown"), "default");
    }

    #[derive(Default)]
    struct Recorder {
        log: Vec<(i32, Phase)>,
        ground: GroundAdapter,
    }

    impl StepQueue for Recorder {
        fn ground(&mut self, car: &mut OriginalCar, lcg: &mut OriginalLcg) {
            self.ground.ground_step(
                car,
                &mut FlatGround {
                    height: 0.0,
                    surface: 1,
                },
                lcg,
            );
        }
        fn trace(&mut self, phase: Phase, car: &OriginalCar) {
            self.log.push((car.globals.sched_step_606ac4, phase));
        }
    }

    /// Step order of `Sched_Step` for the player car: on even steps the
    /// 32 Hz queue (inputs 0x15, dynamics and ground 0x1e, collision 0x28),
    /// then every step the integrations (0x1e) and the camera (100).
    #[test]
    fn scheduler_runs_the_original_order() {
        use Phase::*;
        let mut car = OriginalCar::new(CarSpecs::from_bytes(&[0u8; 0x1c4]));
        car.globals.sched_step_606ac4 = 10;
        let mut q = Recorder::default();
        let mut lcg = OriginalLcg::default();
        for _ in 0..4 {
            scheduler_step(&mut car, 0, &mut lcg, &mut q);
        }
        let even = [
            ReadInputs,
            Dynamics,
            Ground,
            Collide,
            IntegratePosition,
            IntegrateOrientation,
            Camera,
        ];
        let odd = [IntegratePosition, IntegrateOrientation, Camera];
        let mut expected = Vec::new();
        for step in 10..14 {
            let phases: &[Phase] = if step % 2 == 0 { &even } else { &odd };
            expected.extend(phases.iter().map(|&p| (step, p)));
        }
        assert_eq!(q.log, expected);
        assert_eq!(car.globals.sched_step_606ac4, 14);
    }
}
